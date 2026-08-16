//! Fault injection, for tests that need a crash rather than a bug.
//!
//! Every correctness fix in Phase 11 shares a shape: the code is right when
//! nothing goes wrong, and wrong when the process dies between two statements
//! or the disk fills up between two writes. There was no way to write that
//! test, which is why those failure modes went unnoticed rather than
//! unaddressed — a reviewer can only find what a test can reach.
//!
//! Four primitives, all of them rusqlite's own, none of them needing a line of
//! production code:
//!
//! - [`deny_insert_after`] — fail the *n*th `INSERT` into a table. This is a
//!   crash between two statements, expressed as something a test can assert on:
//!   the second statement fails, and what survives is whatever the first one
//!   left behind.
//! - [`cap_pages`] — a full disk, as `SQLITE_FULL`, without filling one.
//! - [`interrupt_after`] — a kill signal mid-statement.
//! - [`assert_atomic_write`] — a file is the old bytes or the new bytes, and
//!   never a prefix of either.
//!
//! # Where the faults land
//!
//! The authorizer runs when a statement is **prepared**, not when it runs.
//! Specline's write path calls `Connection::execute`, which prepares every time, so
//! "the nth insert" and "the nth prepare of an insert" are the same number
//! here. A caller that prepares once and steps many times would see the
//! authorizer once — worth knowing before writing a test that seems not to
//! fire.
//!
//! Denying is `SQLITE_DENY`, which raises an error, rather than `SQLITE_IGNORE`,
//! which makes the statement quietly do nothing. Both simulate something, but
//! only the first simulates the thing being tested: a write that failed and
//! said so.
//!
//! # Usage
//!
//! Not a test target itself — `tests/support/` has no `main.rs`, so cargo does
//! not build it as one. Pull it in with a path attribute:
//!
//! ```ignore
//! #[path = "support/faults.rs"]
//! mod faults;
//! ```

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use rusqlite::Connection;
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many times an injected fault matched.
///
/// Handed back so a test can assert the fault it armed actually fired. A test
/// that passes because its fault never triggered is worse than no test: it
/// reports that a failure mode is handled when the failure never happened.
#[derive(Clone, Debug)]
pub struct FaultCounter(Arc<AtomicUsize>);

impl FaultCounter {
    fn new() -> Self {
        FaultCounter(Arc::new(AtomicUsize::new(0)))
    }

    /// How many matching statements have been seen so far, denied or not.
    pub fn matched(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    /// Assert the fault actually fired.
    pub fn assert_fired(&self, at_least: usize) {
        assert!(
            self.matched() >= at_least,
            "the injected fault never fired: {} matching statements, expected at least {at_least}. \
             The test is passing for a reason that has nothing to do with what it claims to check.",
            self.matched()
        );
    }
}

/// Fail every `INSERT` into `table` from the `nth` one onwards (1-based).
///
/// `nth = 1` fails the first. `nth = 2` lets one through and fails everything
/// after it, which is the shape most of these tests want: the row lands, the
/// event that must accompany it does not, and the assertion is that neither
/// survives.
///
/// The counter it returns keeps counting after the deny, so a test can tell
/// "the fault fired once" from "the fault fired and then the code retried".
pub fn deny_insert_after(conn: &Connection, table: &str, nth: usize) -> FaultCounter {
    let table = table.to_owned();
    let counter = FaultCounter::new();
    let seen = counter.0.clone();

    conn.authorizer(Some(move |ctx: AuthContext<'_>| match ctx.action {
        AuthAction::Insert { table_name } if table_name == table => {
            let n = seen.fetch_add(1, Ordering::SeqCst) + 1;
            if n >= nth {
                Authorization::Deny
            } else {
                Authorization::Allow
            }
        }
        _ => Authorization::Allow,
    }))
    .expect("install an authorizer");

    counter
}

/// Fail every write to `table` — `INSERT`, `UPDATE` or `DELETE` — from the
/// `nth` matching statement onwards.
///
/// The broader sibling of [`deny_insert_after`], for the paths whose second
/// step is an update rather than an insert: archiving a row and then archiving
/// the links that touch it, say.
pub fn deny_write_after(conn: &Connection, table: &str, nth: usize) -> FaultCounter {
    let table = table.to_owned();
    let counter = FaultCounter::new();
    let seen = counter.0.clone();

    conn.authorizer(Some(move |ctx: AuthContext<'_>| {
        let hit = match ctx.action {
            AuthAction::Insert { table_name }
            | AuthAction::Delete { table_name }
            | AuthAction::Update { table_name, .. } => table_name == table,
            _ => false,
        };
        if !hit {
            return Authorization::Allow;
        }
        let n = seen.fetch_add(1, Ordering::SeqCst) + 1;
        if n >= nth {
            Authorization::Deny
        } else {
            Authorization::Allow
        }
    }))
    .expect("install an authorizer");

    counter
}

/// Take the fault off a connection, so the rest of a test can read the damage.
///
/// Needed more often than it looks: with an authorizer still armed, the query
/// that checks what survived is itself denied, and the test fails with an error
/// about the assertion rather than about the code.
pub fn clear(conn: &Connection) {
    conn.authorizer::<fn(AuthContext<'_>) -> Authorization>(None)
        .expect("remove the authorizer");
}

/// Cap the database at its current size plus `slack` pages, so the next few
/// writes get `SQLITE_FULL`.
///
/// A real full disk, without one. The cap has to be at or above the pages
/// already in use — SQLite refuses to shrink below what exists — so this reads
/// the current count rather than taking an absolute number from the caller,
/// which would work on an empty store and quietly do nothing on a seeded one.
pub fn cap_pages(conn: &Connection, slack: i64) -> i64 {
    let current: i64 = conn
        .query_row("PRAGMA page_count", [], |r| r.get(0))
        .expect("read the page count");
    let cap = current + slack;
    let applied: i64 = conn
        .query_row(&format!("PRAGMA max_page_count = {cap}"), [], |r| r.get(0))
        .expect("set the page cap");
    assert_eq!(
        applied, cap,
        "the page cap did not take: asked for {cap}, got {applied}"
    );
    cap
}

/// Lift the page cap set by [`cap_pages`].
pub fn uncap_pages(conn: &Connection) {
    // SQLite's documented maximum. Setting 0 reads the current cap rather than
    // clearing it, which is the kind of API that makes a cleanup silently not
    // clean up.
    conn.query_row("PRAGMA max_page_count = 1073741823", [], |r| {
        r.get::<_, i64>(0)
    })
    .expect("lift the page cap");
}

/// Interrupt whatever this connection is doing once it has run `ops` virtual
/// machine steps.
///
/// This is the killed process: the statement stops in the middle and returns
/// `SQLITE_INTERRUPT`, and whatever transaction it was inside rolls back. The
/// step count is approximate by design — SQLite checks between instructions, so
/// a test asserts on the outcome (the write did not land) rather than on where
/// exactly it stopped.
pub fn interrupt_after(conn: &Connection, ops: i32) -> FaultCounter {
    let counter = FaultCounter::new();
    let seen = counter.0.clone();
    conn.progress_handler(
        ops,
        Some(move || {
            seen.fetch_add(1, Ordering::SeqCst);
            true
        }),
    )
    .expect("install a progress handler");
    counter
}

/// Stop interrupting.
pub fn stop_interrupting(conn: &Connection) {
    conn.progress_handler(0, None::<fn() -> bool>)
        .expect("remove the progress handler");
}

/// Assert a file holds exactly the old content or exactly the new content.
///
/// The property a torn write breaks. `specline generate` truncates and rewrites, so
/// a crash halfway leaves a prefix of the new content — and one of the files it
/// writes is `product/CLAUDE.md`, whose prefix is a standing contract with the
/// second half missing. A prefix is the failure this asserts against, and it is
/// called out separately in the message because "the file is wrong" and "the
/// file is half-written" want different fixes.
pub fn assert_atomic_write(path: &Path, old: &str, new: &str) {
    let actual = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {} back: {e}", path.display()));

    if actual == old || actual == new {
        return;
    }

    let torn = new.starts_with(&actual) || old.starts_with(&actual);
    assert!(
        !torn,
        "{} is a torn write: {} bytes, a prefix of content that should have arrived whole",
        path.display(),
        actual.len()
    );
    panic!(
        "{} holds neither the old content ({} bytes) nor the new ({} bytes); it has {} bytes",
        path.display(),
        old.len(),
        new.len(),
        actual.len()
    );
}
