//! The release installer must actually verify what it downloads.
//!
//! `dist` generates a shell installer whose sha256 path reads:
//!
//! ```text
//! if ! check_cmd sha256sum; then
//!     say "skipping sha256 checksum verification (it requires the 'sha256sum' command)"
//!     return 0
//! fi
//! ```
//!
//! A missing tool returns success, so a corrupted archive installs. Measured on
//! 2026-08-14: current macOS does ship `/sbin/sha256sum`, which is a correction
//! to what PHASE-10 §10 claims — but `/sbin` is not on the `PATH=/usr/bin:/bin`
//! that `scripts/verify-release-tier1.sh` installs under, and older macOS has it
//! nowhere. `/usr/bin/shasum` is present in both cases.
//!
//! `scripts/patch-installer.sh` rewrites that block. These tests are what say it
//! worked, and the one that matters is
//! [`the_unpatched_installer_waves_a_corrupted_file_through`] — it pins the bug,
//! so if `dist` fixes this upstream that test fails and the patch can be
//! deleted with evidence rather than on a hunch.
//!
//! Everything here runs under `env -i PATH=/usr/bin:/bin`, deliberately: the
//! whole defect is invisible on a full `PATH`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<root>/crates/keel`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the manifest directory has a grandparent")
        .to_path_buf()
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dist-installer-checksum.sh")
}

/// The path `scripts/verify-release-tier1.sh` installs under.
///
/// On macOS this has `shasum` and no `sha256sum` — `/sbin/sha256sum` exists on
/// current macOS but `/sbin` is not here, which is the whole defect. On Linux
/// it has `sha256sum` from coreutils. Both are real user environments and the
/// patched script has to work on both.
const TIER_ONE_PATH: &str = "PATH=/usr/bin:/bin";

/// Run `verify_checksum` from `script` against `payload` and `digest`.
///
/// `env -i` rather than `Command::env_clear`, because the point is to reproduce
/// exactly the environment `verify-release-tier1.sh` uses.
fn verify_on(path: &str, script: &Path, payload: &Path, digest: &str) -> Output {
    Command::new("/usr/bin/env")
        .arg("-i")
        .arg(path)
        .arg("/bin/sh")
        .arg(script)
        .arg(payload)
        .arg(digest)
        .output()
        .expect("the checksum harness runs")
}

fn verify(script: &Path, payload: &Path, digest: &str) -> Output {
    verify_on(TIER_ONE_PATH, script, payload, digest)
}

/// A `PATH=` argument naming a directory that holds only the tools listed.
///
/// Needed because "no digest tool available" is a *platform* fact otherwise:
/// macOS reaches it with the ordinary tier-1 path and Linux never does, since
/// coreutils puts `sha256sum` in `/usr/bin`. A test that only holds on one of
/// them is not testing the behaviour, it is testing the runner — which is
/// exactly how the first CI run under the self-hosted runners found this,
/// green on macOS and red on Linux.
///
/// Symlinks rather than copies, so this stays cheap and so the tools are the
/// real ones.
fn path_containing(dir: &Path, tools: &[&str]) -> String {
    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).expect("a scratch bin directory");
    for tool in tools {
        let located = Command::new("/bin/sh")
            .arg("-c")
            .arg(format!("command -v {tool}"))
            .output()
            .expect("look up a tool");
        let real = String::from_utf8_lossy(&located.stdout).trim().to_owned();
        assert!(
            !real.is_empty(),
            "{tool} is needed by the installer's checksum path and is not on this machine"
        );
        std::os::unix::fs::symlink(&real, bin.join(tool)).expect("symlink the tool");
    }
    format!("PATH={}", bin.display())
}

/// Copy the fixture into `dir` so a test can patch it without touching the
/// checked-in copy.
fn staged(dir: &Path) -> PathBuf {
    let dest = dir.join("installer.sh");
    std::fs::copy(fixture(), &dest).expect("the fixture is readable");
    dest
}

fn patch(script: &Path) -> Output {
    Command::new(repo_root().join("scripts/patch-installer.sh"))
        .arg(script)
        .output()
        .expect("the patch script runs")
}

/// A file and its correct sha256, computed by something other than the code
/// under test.
fn payload(dir: &Path, bytes: &[u8]) -> (PathBuf, String) {
    use sha2::{Digest, Sha256};
    let path = dir.join("payload.bin");
    std::fs::write(&path, bytes).expect("the payload is writable");
    let digest = format!("{:x}", Sha256::digest(bytes));
    (path, digest)
}

/// The bug, pinned. Not a demonstration for its own sake: this is the test that
/// tells a future session the patch is still needed.
///
/// Run against a path with `awk` and deliberately no digest tool, so it asserts
/// the *behaviour* rather than whichever runner it happens to be on.
#[test]
fn the_unpatched_installer_waves_a_corrupted_file_through() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = staged(dir.path());
    let (path, _) = payload(dir.path(), b"the archive we asked for\n");
    let toolless = path_containing(dir.path(), &["awk"]);

    let wrong = "0".repeat(64);
    let output = verify_on(&toolless, &script, &path, &wrong);

    assert!(
        output.status.success(),
        "this test exists to record that the generated installer accepts a file whose \
         checksum is wrong when it cannot find a digest tool. If it has started refusing, \
         dist has fixed this upstream and scripts/patch-installer.sh should be deleted \
         rather than kept passing."
    );
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("skipping sha256 checksum verification"),
        "and it should say why it let it through: {said}"
    );
}

/// The branch the patch adds that upstream does not have: no digest tool is a
/// refusal, not a pass.
#[test]
fn the_patched_installer_refuses_when_it_has_no_way_to_check() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = staged(dir.path());
    assert!(patch(&script).status.success());
    let (path, digest) = payload(dir.path(), b"the archive we asked for\n");
    let toolless = path_containing(dir.path(), &["awk"]);

    // The *correct* digest, so the only reason to refuse is that it cannot
    // check — not that the bytes are wrong.
    let output = verify_on(&toolless, &script, &path, &digest);

    assert!(
        !output.status.success(),
        "a checksum that cannot be computed has established nothing, and installing \
         anyway is what this whole patch is about"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("neither 'sha256sum' nor 'shasum'"),
        "and it must name what is missing: {complaint}"
    );
}

/// The fix, on the environment the defect actually shows up in.
#[test]
fn the_patched_installer_accepts_the_file_it_asked_for() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = staged(dir.path());
    let patched = patch(&script);
    assert!(
        patched.status.success(),
        "patching failed: {}",
        String::from_utf8_lossy(&patched.stderr)
    );

    let (path, digest) = payload(dir.path(), b"the archive we asked for\n");
    let output = verify(&script, &path, &digest);

    assert!(
        output.status.success(),
        "a correct checksum must pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !String::from_utf8_lossy(&output.stdout).contains("skipping"),
        "and it must pass by having been checked, not by being skipped"
    );
}

/// The failure case, which is the only one anybody installs a checksum for.
#[test]
fn the_patched_installer_refuses_a_corrupted_file() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = staged(dir.path());
    assert!(patch(&script).status.success());

    let (path, digest) = payload(dir.path(), b"the archive we asked for\n");
    std::fs::write(&path, b"something else entirely\n").expect("the payload is writable");

    let output = verify(&script, &path, &digest);

    assert!(
        !output.status.success(),
        "a corrupted file must not install"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("checksum mismatch"),
        "and it must say so: {complaint}"
    );
}

/// Running the patch twice is a no-op, so a workflow that patches an already
/// patched file does not mangle it.
#[test]
fn patching_is_idempotent() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = staged(dir.path());
    assert!(patch(&script).status.success());
    let once = std::fs::read_to_string(&script).expect("readable");

    let again = patch(&script);
    assert!(again.status.success());
    assert!(
        String::from_utf8_lossy(&again.stdout).contains("already patched"),
        "the second run should say it had nothing to do"
    );
    assert_eq!(
        once,
        std::fs::read_to_string(&script).expect("readable"),
        "and it should not have changed the file"
    );
}

/// The loud half. A patch that silently does not apply is the same class of bug
/// as the one being fixed, so text the script does not recognise has to fail
/// rather than pass through.
#[test]
fn text_the_patch_does_not_recognise_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = dir.path().join("not-an-installer.sh");
    std::fs::write(&script, "#!/bin/sh\necho hello\n").expect("writable");

    let output = patch(&script);

    assert!(
        !output.status.success(),
        "an installer without the expected block must fail the patch, not be skipped"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("does not contain the block"),
        "and it must say what it was looking for: {complaint}"
    );
}
