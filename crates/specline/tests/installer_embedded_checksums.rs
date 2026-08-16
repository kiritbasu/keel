//! A release's installer has to carry the checksum of the archive it will
//! download, and it has to be the checksum of the archive being published.
//!
//! Specline 0.1.2's did not. `dist` fills those digests in from the per-target
//! `dist-manifest.json` files it finds beside the archives; the hand-written
//! release workflow never wrote any, so the installer came out with no
//! `_checksum_style` at all. That is not an error, is not warned about, and
//! installs anyway (KEEL-228).
//!
//! Three checks were green at the time. `scripts/patch-installer.sh` passed —
//! it was fixing a different hole in the same file. Both release-verification
//! tiers passed their "installer refuses a corrupt archive" check, because a
//! corrupted archive *does* fail — at `tar` — and their grep for checksum
//! language matched the words "no checksums to verify".
//!
//! So `scripts/check-installer-checksums.sh` is written not to be satisfiable
//! by wording. It reads the hex out of the installer's own case statement,
//! hashes the file on disk, and compares. These tests are what say it is.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<root>/crates/specline`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the manifest directory has a grandparent")
        .to_path_buf()
}

fn check(installer: &Path, artifacts: &Path) -> Output {
    Command::new(repo_root().join("scripts/check-installer-checksums.sh"))
        .arg(installer)
        .arg(artifacts)
        .output()
        .expect("the check script runs")
}

fn check_embedded_only(installer: &Path) -> Output {
    Command::new(repo_root().join("scripts/check-installer-checksums.sh"))
        .arg("--embedded-only")
        .arg(installer)
        .output()
        .expect("the check script runs")
}

const ARCHIVE: &str = "specline-aarch64-apple-darwin.tar.xz";

/// Write an archive of arbitrary bytes and return its real sha256.
fn archive(dir: &Path, bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    std::fs::write(dir.join(ARCHIVE), bytes).expect("the archive is writable");
    specline_core::hex::encode(&Sha256::digest(bytes))
}

/// The shape `dist` emits, cut down to the part this script reads: one arm per
/// archive in `install()`'s destructuring case statement.
///
/// `checksum: None` is the 0.1.2 installer — the two lines simply absent rather
/// than present and empty, which is why the check looks for arms and then asks
/// each one, rather than looking for checksums and counting them.
fn installer(dir: &Path, checksum: Option<&str>) -> PathBuf {
    let embedded = match checksum {
        Some(value) => format!(
            "            _checksum_style=\"sha256\"\n            _checksum_value=\"{value}\"\n"
        ),
        None => String::new(),
    };
    let text = format!(
        "#!/bin/sh\n\
         # a stand-in for the generated installer\n\
         \n\
         case \"$_artifact_name\" in \n\
         \x20       \"{ARCHIVE}\")\n\
         \x20           _arch=\"aarch64-apple-darwin\"\n\
         \x20           _zip_ext=\".tar.xz\"\n\
         {embedded}\
         \x20           _bins=\"specline specline-daemon\"\n\
         \x20           ;;\n\
         \x20       *)\n\
         \x20           err \"internal installer error\"\n\
         \x20           ;;\n\
         esac\n"
    );
    let path = dir.join("specline-installer.sh");
    std::fs::write(&path, text).expect("the installer is writable");
    path
}

#[test]
fn an_installer_carrying_the_right_checksum_passes() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let digest = archive(dir.path(), b"pretend this is a tarball\n");
    let script = installer(dir.path(), Some(&digest));

    let output = check(&script, dir.path());

    assert!(
        output.status.success(),
        "a matching checksum must pass: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains(&digest),
        "and it should say which digest it compared: {said}"
    );
}

/// The 0.1.2 release, caught. This is the test that would have stopped it.
#[test]
fn an_installer_with_no_checksum_embedded_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    archive(dir.path(), b"pretend this is a tarball\n");
    let script = installer(dir.path(), None);

    let output = check(&script, dir.path());

    assert!(
        !output.status.success(),
        "an installer that would download this archive with nothing to check it against \
         must not be publishable"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains(ARCHIVE) && complaint.contains("embeds no checksum"),
        "and it must name the archive and the reason: {complaint}"
    );
}

/// The archive and the installer disagreeing is the case the whole check is
/// built to notice — somebody rebuilt one after the other was made.
#[test]
fn a_checksum_that_does_not_match_the_archive_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let digest = archive(dir.path(), b"pretend this is a tarball\n");
    let script = installer(dir.path(), Some(&digest));
    // Rebuild the archive after the installer was written.
    archive(dir.path(), b"a different tarball entirely\n");

    let output = check(&script, dir.path());

    assert!(!output.status.success(), "a mismatch must not ship");
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("hashes to"),
        "and it must show both digests: {complaint}"
    );
}

#[test]
fn an_archive_the_installer_offers_but_the_release_lacks_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let digest = archive(dir.path(), b"pretend this is a tarball\n");
    let script = installer(dir.path(), Some(&digest));
    std::fs::remove_file(dir.path().join(ARCHIVE)).expect("the archive is removable");

    let output = check(&script, dir.path());

    assert!(
        !output.status.success(),
        "an installer pointing at an archive nobody is publishing must not ship"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("is not in"),
        "and it must say what is missing: {complaint}"
    );
}

/// The secondary check: the silent-skip wording must not survive into a shipped
/// installer even if the digests happen to be right.
#[test]
fn the_silent_skip_wording_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let digest = archive(dir.path(), b"pretend this is a tarball\n");
    let script = installer(dir.path(), Some(&digest));
    let mut text = std::fs::read_to_string(&script).expect("readable");
    text.push_str("say \"no checksums to verify\" 1>&2\n");
    std::fs::write(&script, text).expect("writable");

    let output = check(&script, dir.path());

    assert!(!output.status.success(), "the skip branch must not ship");
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("no checksums to verify"),
        "and it must quote the branch it found: {complaint}"
    );
}

/// A script with nothing recognisable in it fails rather than passing on an
/// empty parse. The failure mode being guarded against here is the check
/// itself: a template change that made the regex match nothing would otherwise
/// turn this into a check that always passes.
#[test]
fn an_installer_this_check_cannot_parse_is_refused() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let script = dir.path().join("not-an-installer.sh");
    std::fs::write(&script, "#!/bin/sh\necho hello\n").expect("writable");

    let output = check(&script, dir.path());

    assert!(
        !output.status.success(),
        "parsing no archives must fail, not pass vacuously"
    );
    let complaint = String::from_utf8_lossy(&output.stderr);
    assert!(
        complaint.contains("found no archive arms"),
        "and it must say it parsed nothing: {complaint}"
    );
}

/// `--embedded-only` still catches a missing checksum, and says in its own
/// output that it compared nothing — the weaker claim must not read like the
/// stronger one.
#[test]
fn embedded_only_checks_less_and_says_so() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let digest = archive(dir.path(), b"pretend this is a tarball\n");

    let with = installer(dir.path(), Some(&digest));
    let output = check_embedded_only(&with);
    assert!(
        output.status.success(),
        "an embedded digest passes without the archive: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.contains("NOT compared"),
        "and it must say what it did not establish: {said}"
    );

    let dir2 = tempfile::tempdir().expect("a temporary directory");
    let without = installer(dir2.path(), None);
    assert!(
        !check_embedded_only(&without).status.success(),
        "and a missing checksum is still a refusal in this mode"
    );
}
