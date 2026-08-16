//! Migrating is a decision, not something that happens to you.
//!
//! The store used to apply pending migrations from whichever process opened it
//! first. With one migration shipped that is invisible; with two it is how a
//! newer CLI alters the schema underneath a running older daemon, which is the
//! corruption the newer-store guard was written after, arriving through the
//! front door.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{Store, pending_migrations_at, shipped_schema_version};

/// Make a store look like one an older binary wrote: the tables are there, the
/// ledger has forgotten a migration, so this binary has something to apply.
///
/// Forging the ledger rather than checking in a second migration, because the
/// behaviour under test is "there is something outstanding" and pinning it to a
/// migration that exists would make this test expire the moment that migration
/// became the newest one.
fn forget_a_migration(path: &std::path::Path) {
    let conn = rusqlite::Connection::open(path).unwrap();
    let n = conn
        .execute("DELETE FROM _keel_migrations WHERE id = 1", [])
        .unwrap();
    assert_eq!(n, 1, "the fixture did not have migration 1 to forget");
}

#[test]
fn a_store_this_call_creates_is_migrated() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");

    let store =
        Store::open(&path).expect("a store that does not exist yet is created and migrated");

    assert_eq!(store.schema_version().unwrap(), shipped_schema_version());
    assert!(store.pending_migrations().unwrap().is_empty());
}

#[test]
fn an_existing_store_with_migrations_pending_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");
    drop(Store::open(&path).unwrap());
    forget_a_migration(&path);

    let error = Store::open(&path).expect_err("a pending migration must not be applied by a read");
    let message = error.to_string();

    assert!(
        message.contains("specline migrate"),
        "the refusal has to name the command that fixes it: {message}"
    );
    assert!(
        message.contains("initial_schema"),
        "and say what is outstanding: {message}"
    );
}

/// An existing file with nothing applied: the shape a store has after a
/// restore from a snapshot an older binary wrote, and the case where the two
/// doors have to differ. `Store::open` sees a file that is already there and
/// refuses; `open_and_migrate` builds it out.
///
/// A zero-byte file rather than a forged ledger, because migration 1 creates
/// tables without `IF NOT EXISTS` — re-applying it over its own tables fails,
/// which is correct and is why the refusal fixture cannot be reused here.
#[test]
fn the_owner_may_apply_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");
    std::fs::File::create(&path).unwrap();

    assert_eq!(
        pending_migrations_at(&path).unwrap().len(),
        shipped_schema_version() as usize
    );
    assert!(
        Store::open(&path).is_err(),
        "the file exists, so nothing outstanding may be applied through this door"
    );

    let store = Store::open_and_migrate(&path).expect("the owner's door applies what is pending");
    assert!(store.pending_migrations().unwrap().is_empty());
    assert_eq!(store.schema_version().unwrap(), shipped_schema_version());
    drop(store);

    // And the ordinary door opens it now that nothing is outstanding.
    Store::open(&path).expect("a migrated store opens normally");
    assert!(pending_migrations_at(&path).unwrap().is_empty());
}

/// The reason `specline migrate` can say what is pending at all: it cannot open a
/// `Store` to ask, because the store it is about to migrate is the one
/// `Store::open` refuses.
#[test]
fn what_is_pending_can_be_read_without_opening_a_store() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");
    drop(Store::open(&path).unwrap());
    forget_a_migration(&path);

    assert!(Store::open(&path).is_err());

    let pending = pending_migrations_at(&path).unwrap();
    assert_eq!(pending, vec![(1, "initial_schema".to_owned())]);
}

/// A path that is not a store at all reports everything as pending rather than
/// erroring. It is the honest answer for a store that has never been made, and
/// a harmless one for a file that is not a store — migrating it fails on its
/// own terms a moment later, saying so.
#[test]
fn a_file_with_no_ledger_has_applied_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("never-made.sqlite");

    let pending = pending_migrations_at(&path).unwrap();
    assert_eq!(pending.len(), shipped_schema_version() as usize);
}

/// Forge a ledger entry from the future: a migration this binary has never
/// heard of, already applied.
///
/// The number is deliberately far ahead rather than `shipped + 1`, so the test
/// keeps meaning the same thing as migrations are added and never accidentally
/// collides with a real one.
fn pretend_a_newer_binary_migrated_it(path: &std::path::Path) -> i32 {
    let future = shipped_schema_version() + 1000;
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute(
        "INSERT INTO _keel_migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![
            future,
            "written_by_a_binary_from_the_future",
            "2099-01-01T00:00:00Z"
        ],
    )
    .unwrap();
    future
}

/// The guard that turns a silent corruption into a startup error.
///
/// This is the direction that actually cost this project data. A daemon built
/// before a migration kept running, found every migration it knew about already
/// applied, concluded it was up to date, and went on inserting rows with the new
/// column left NULL — surfacing two days later as an unrelated-looking read
/// error.
///
/// It was lost once already and nobody noticed at the time: DuckDB's engine
/// refused such an open on its own, so the guard was never the only thing
/// holding the line. SQLite opens it happily. The behaviour came back because
/// repointed tests caught it, which is luck rather than coverage — hence this
/// test, which fails if it is ever lost again.
#[test]
fn a_store_newer_than_this_binary_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");
    drop(Store::open(&path).unwrap());
    let future = pretend_a_newer_binary_migrated_it(&path);

    let error = Store::open(&path).expect_err("a store from the future must not be opened");
    let message = error.to_string();

    assert!(
        message.contains(&future.to_string()),
        "the refusal should name the schema it found: {message}"
    );
    assert!(
        message.contains(&shipped_schema_version().to_string()),
        "and the one this binary understands, so the direction is unambiguous: {message}"
    );
    assert!(
        message.contains("older than the store"),
        "and say which way round it is, because the opposite case has a different fix: {message}"
    );
}

/// Opening for write must refuse it too.
///
/// The read path and the write path reach the ledger by different routes, and a
/// guard that only covered one of them would leave the daemon — the process
/// that actually writes — free to do the damage.
#[test]
fn a_store_newer_than_this_binary_is_refused_to_a_writer_as_well() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("keel.sqlite");
    drop(Store::open(&path).unwrap());
    pretend_a_newer_binary_migrated_it(&path);

    Store::open_and_migrate(&path)
        .expect_err("migrating a store from the future is the worst version of this");
    Store::open_and_migrate_exclusive(&path)
        .expect_err("and the daemon's own entry point must refuse it too");
}
