//! The fault-injection testkit, tested.
//!
//! A testkit is a safety mechanism, and this codebase has already been bitten
//! once by a safety mechanism that quietly did nothing — the `PostToolUse` hook
//! that called a renamed command and swallowed the failure, silently losing
//! every edit it claimed to capture. A fault that never fires makes its test
//! pass, and a passing test is a claim that a failure mode is handled.
//!
//! So each primitive is asserted here against plain SQL, where the expected
//! outcome is obvious, before any correctness test relies on it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/faults.rs"]
mod faults;

use keel_core::store::Store;
use rusqlite::Connection;

/// A table with nothing to do with Keel's schema, so these tests assert on the
/// kit rather than on whatever the store happens to write.
fn scratch() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT NOT NULL) STRICT;")
        .unwrap();
    conn
}

fn rows(conn: &Connection) -> i64 {
    conn.query_row("SELECT count(*) FROM t", [], |r| r.get(0))
        .unwrap()
}

#[test]
fn denying_the_second_insert_lets_the_first_through() {
    let conn = scratch();
    let fault = faults::deny_insert_after(&conn, "t", 2);

    conn.execute("INSERT INTO t (v) VALUES ('one')", [])
        .expect("the first insert is allowed");
    let second = conn.execute("INSERT INTO t (v) VALUES ('two')", []);

    assert!(second.is_err(), "the second insert should have been denied");
    fault.assert_fired(2);

    faults::clear(&conn);
    assert_eq!(rows(&conn), 1, "exactly one row should have survived");
}

#[test]
fn a_deny_only_touches_the_named_table() {
    let conn = scratch();
    conn.execute_batch("CREATE TABLE other (id INTEGER PRIMARY KEY) STRICT;")
        .unwrap();
    faults::deny_insert_after(&conn, "t", 1);

    conn.execute("INSERT INTO other (id) VALUES (1)", [])
        .expect("a write to another table is untouched");
    assert!(conn.execute("INSERT INTO t (v) VALUES ('x')", []).is_err());
    faults::clear(&conn);
}

#[test]
fn deny_write_catches_updates_as_well_as_inserts() {
    let conn = scratch();
    conn.execute("INSERT INTO t (v) VALUES ('one')", [])
        .unwrap();

    let fault = faults::deny_write_after(&conn, "t", 1);
    let update = conn.execute("UPDATE t SET v = 'two' WHERE id = 1", []);
    assert!(update.is_err(), "the update should have been denied");
    fault.assert_fired(1);

    faults::clear(&conn);
    let v: String = conn
        .query_row("SELECT v FROM t WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(v, "one", "the denied update must not have landed");
}

#[test]
fn clearing_a_fault_lets_writes_through_again() {
    let conn = scratch();
    faults::deny_insert_after(&conn, "t", 1);
    assert!(conn.execute("INSERT INTO t (v) VALUES ('x')", []).is_err());

    faults::clear(&conn);
    conn.execute("INSERT INTO t (v) VALUES ('y')", [])
        .expect("writes resume once the fault is cleared");
    assert_eq!(rows(&conn), 1);
}

/// A full disk, without one. Against a file store, because `max_page_count` is
/// about pages on disk and an in-memory database grows differently.
#[test]
fn a_page_cap_produces_a_full_disk_error() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Connection::open(dir.path().join("t.sqlite")).unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT NOT NULL) STRICT;")
        .unwrap();

    faults::cap_pages(&conn, 1);

    // One page of slack does not hold a megabyte of text, so this must fail —
    // and it must fail as a full disk rather than as anything else, or a test
    // built on it would be asserting against the wrong error.
    let big = "x".repeat(1_000_000);
    let err = conn
        .execute("INSERT INTO t (v) VALUES (?1)", [&big])
        .expect_err("a capped store should refuse a write it has no room for");
    assert!(
        err.to_string().contains("full"),
        "expected a disk-full error, got: {err}"
    );

    faults::uncap_pages(&conn);
    conn.execute("INSERT INTO t (v) VALUES (?1)", [&big])
        .expect("lifting the cap should let the same write through");
}

#[test]
fn an_interrupt_stops_a_statement_and_rolls_it_back() {
    let conn = scratch();
    conn.execute_batch(
        "WITH RECURSIVE seq(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM seq WHERE n < 500) \
         INSERT INTO t (v) SELECT 'seed' FROM seq;",
    )
    .unwrap();
    let before = rows(&conn);

    let fault = faults::interrupt_after(&conn, 1);
    let killed = conn.execute("UPDATE t SET v = 'changed'", []);
    assert!(
        killed.is_err(),
        "the statement should have been interrupted"
    );
    fault.assert_fired(1);

    faults::stop_interrupting(&conn);
    assert_eq!(rows(&conn), before, "an interrupted statement rolls back");
    let changed: i64 = conn
        .query_row("SELECT count(*) FROM t WHERE v = 'changed'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(changed, 0, "no partial update should have survived");
}

#[test]
fn the_atomic_write_asserter_accepts_either_whole_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.md");

    std::fs::write(&path, "old").unwrap();
    faults::assert_atomic_write(&path, "old", "new content");

    std::fs::write(&path, "new content").unwrap();
    faults::assert_atomic_write(&path, "old", "new content");
}

#[test]
fn the_atomic_write_asserter_rejects_a_prefix() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("f.md");
    std::fs::write(&path, "new con").unwrap();

    let caught = std::panic::catch_unwind(|| {
        faults::assert_atomic_write(&path, "old", "new content");
    });
    assert!(caught.is_err(), "a half-written file should have failed");
}

/// The kit has to work against the real store's connection, not only a scratch
/// one — that is the whole point, and `Store::connection()` is a `&Connection`
/// while `authorizer` wants a `&self`, which is exactly the combination that
/// could have made this impossible.
#[test]
fn faults_can_be_armed_on_a_real_store() {
    let store = Store::in_memory().unwrap();
    let fault = faults::deny_insert_after(store.connection(), "events", 1);

    let denied = store
        .connection()
        .execute("INSERT INTO events (id) VALUES ('evt_x')", []);
    assert!(denied.is_err());
    fault.assert_fired(1);

    faults::clear(store.connection());
}
