//! One writer at a time, and the ways that must not go wrong (B-60, KEEL-180).
//!
//! The lock exists because of a real incident: a second daemon opened the live
//! store and applied a schema migration while the first was serving it. So the
//! tests that matter here are the failure ones — a second writer refused, a
//! reader unaffected, and nothing left behind when a holder dies.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::Store;

#[test]
fn a_second_writer_is_refused_while_the_first_holds_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");

    let first = Store::open_exclusive(&path).unwrap();
    let err = Store::open_exclusive(&path).unwrap_err().to_string();

    assert!(
        err.contains("already has this store open for writing"),
        "the refusal must say what is wrong: {err}"
    );
    drop(first);
}

/// The incident this was built for, in miniature: the second opener is the one
/// that would migrate.
#[test]
fn a_second_writer_cannot_migrate_under_the_first() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");

    let serving = Store::open_exclusive(&path).unwrap();
    let err = Store::open_and_migrate_exclusive(&path)
        .unwrap_err()
        .to_string();

    assert!(
        err.contains("already has this store open for writing"),
        "{err}"
    );
    drop(serving);
}

/// Reading must not contend. `doctor`, `fsck` and the desktop app all look at
/// the store while the daemon writes to it, and a lock that stopped them would
/// make inspecting a busy store impossible — which is when you most want to.
#[test]
fn a_reader_opens_happily_alongside_the_writer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");

    let writer = Store::open_exclusive(&path).unwrap();
    let reader = Store::open(&path).expect("reading must not need the lock");
    drop((writer, reader));
}

/// Releasing has to happen on drop, or the second command of any pair fails.
#[test]
fn the_store_can_be_claimed_again_once_the_holder_is_gone() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");

    drop(Store::open_exclusive(&path).unwrap());
    Store::open_exclusive(&path).expect("the claim should have been released on drop");
}

/// Two stores are two claims. A lock keyed on the wrong thing — a fixed path, a
/// directory — would serialise unrelated work and be discovered as a mysterious
/// hang rather than as an error.
#[test]
fn two_stores_do_not_block_each_other() {
    let dir = tempfile::tempdir().unwrap();
    let a = Store::open_exclusive(dir.path().join("a.sqlite")).unwrap();
    let b = Store::open_exclusive(dir.path().join("b.sqlite")).unwrap();
    drop((a, b));
}

/// The lock file is not the database, and must never be mistaken for it.
#[test]
fn the_lock_is_a_file_of_its_own_beside_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");
    let held = Store::open_exclusive(&path).unwrap();

    let lock = specline_core::lock::lock_path(&path);
    assert!(lock.exists(), "the lock file should exist while held");
    assert_ne!(lock, path, "the lock must not be the database itself");
    drop(held);

    // And it stays on disk afterwards, holding nothing. That is not litter to
    // clean up: deleting it between the two opens of a pair is how a lock file
    // scheme develops a race.
    assert!(lock.exists());
    Store::open_exclusive(&path).expect("a leftover lock file must not block anything");
}

/// Held by a child process so this one can kill it.
///
/// Ignored, because it is not a test — it is the other half of the one below,
/// and it sleeps. Run directly it does nothing at all.
#[test]
#[ignore = "spawned as a child by a_killed_holder_leaves_nothing_behind"]
fn hold_the_store_until_killed() {
    use std::io::Write;
    let Ok(path) = std::env::var("SPECLINE_TEST_HOLD") else {
        return;
    };
    let _held = Store::open_exclusive(&path).expect("the child should get the claim");
    println!("HELD");
    std::io::stdout().flush().ok();
    std::thread::sleep(std::time::Duration::from_secs(120));
}

/// The claim that the whole decision rests on.
///
/// TQ-36 rejected a lock file because "a stale lock after a crash is a store
/// nobody can open". That is true of a PID file and false of an advisory lock,
/// and B-60 overturned the objection on that basis — so it is worth proving
/// through `Store::open_exclusive` rather than through a toy program that only
/// resembles it. `Child::kill` is `SIGKILL` on unix: no unwinding, no `Drop`,
/// no chance to tidy up. If anything here depended on a graceful exit, this is
/// where it would show.
#[test]
fn a_killed_holder_leaves_nothing_behind() {
    use std::io::{BufRead, BufReader};
    use std::process::{Command, Stdio};

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("specline.sqlite");

    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "hold_the_store_until_killed",
            "--ignored",
            "--nocapture",
        ])
        .env("SPECLINE_TEST_HOLD", &path)
        .stdout(Stdio::piped())
        .spawn()
        .expect("spawn the holder");

    // Wait for it to say it has the claim, rather than sleeping and hoping.
    let stdout = child.stdout.take().unwrap();
    let mut lines = BufReader::new(stdout).lines();
    let held = lines
        .by_ref()
        .take(20)
        .filter_map(std::result::Result::ok)
        .any(|l| l.contains("HELD"));
    assert!(held, "the child never reported holding the store");

    // With the child alive, this process must be refused.
    assert!(
        Store::open_exclusive(&path).is_err(),
        "the child holds the claim, so this must not get it"
    );

    child.kill().expect("kill the holder");
    child.wait().ok();

    Store::open_exclusive(&path)
        .expect("a SIGKILLed holder must leave the store claimable, with nothing to clean up");
}
