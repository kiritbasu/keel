//! A store that will not open must not become a restart storm.
//!
//! The daemon runs under launchd with `KeepAlive`/`SuccessfulExit: false`,
//! which restarts it on a *non-zero* exit. A store that cannot be opened will
//! not open on the next attempt either, so exiting non-zero produces a fresh
//! process every few seconds — each one re-running migration, re-attempting the
//! model download, and writing another copy of the same error until the real
//! one is thousands of lines up. From the outside it looks like a crashing
//! daemon rather than a store that needs attention, which sends whoever is
//! debugging it after the wrong thing entirely.
//!
//! So an unrecoverable store error exits zero. Not because it succeeded — the
//! log says plainly that it did not — but because zero is how you tell launchd
//! to stay down.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::process::Command;

/// A home the daemon cannot open a store in: the store path is a directory.
///
/// SQLite will not open a directory as a database, and no retry changes that,
/// which is exactly the shape of condition under test.
fn unopenable_home() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("specline.sqlite")).unwrap();
    dir
}

#[test]
fn an_unopenable_store_exits_zero_and_says_why() {
    let home = unopenable_home();

    let output = Command::new(env!("CARGO_BIN_EXE_specline-daemon"))
        .arg("--home")
        .arg(home.path())
        // Port 1 is privileged, so a bind would fail — but this must never get
        // as far as binding. If it does, the test fails on the exit code, which
        // is the assertion that matters.
        .args(["--bind", "127.0.0.1:1"])
        .env_remove("SPECLINE_HOME")
        .env_remove("SPECLINE_BIND")
        .env("RUST_LOG", "specline_daemon=error")
        .output()
        .expect("run the daemon binary");

    assert!(
        output.status.success(),
        "a store that cannot be opened must exit zero so launchd leaves it down; \
         got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );

    // `tracing_subscriber::fmt()` writes to stdout, so both streams are read
    // rather than assuming which one carries the message.
    let logged = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        logged.contains("could not be opened"),
        "exiting zero is only defensible if the log says what happened: {logged}"
    );
    assert!(
        logged.contains("specline doctor"),
        "and points at the thing that explains it: {logged}"
    );
}

/// The store's location is reported before it is opened, so a corruption
/// investigation finds the warning above the failure rather than below it.
#[test]
fn a_synced_home_is_warned_about_at_startup() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("Dropbox").join(".specline");
    std::fs::create_dir_all(home.join("specline.sqlite")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_specline-daemon"))
        .arg("--home")
        .arg(&home)
        .args(["--bind", "127.0.0.1:1"])
        .env_remove("SPECLINE_HOME")
        .env_remove("SPECLINE_BIND")
        .env("RUST_LOG", "specline_daemon=warn")
        .output()
        .expect("run the daemon binary");

    let logged = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        logged.contains("Dropbox"),
        "the warning should name the service: {logged}"
    );
    assert!(
        logged.contains("specline backup"),
        "and say to take a consistent snapshot before moving anything: {logged}"
    );
}
