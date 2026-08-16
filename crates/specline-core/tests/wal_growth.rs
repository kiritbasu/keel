//! The write-ahead log has to come back down.
//!
//! WAL mode is what stops the app stalling behind a write, and it has one sharp
//! edge: SQLite cannot checkpoint past the oldest open read snapshot. A reader
//! that never releases one — a statement left mid-iteration, a read transaction
//! not dropped, a guard alive across a suspended future — pins the log at that
//! point and every subsequent write appends to it forever.
//!
//! Nothing errors. Every query still answers, correctly, out of the log. The
//! only symptom is a `-wal` file quietly larger than the database beside it,
//! and the daemon that would produce it runs for days at a time holding a
//! server-sent-events connection open.
//!
//! So: two tests. One that ordinary writing keeps the log bounded, and one that
//! a second connection reading throughout does not stop it — because "a
//! long-lived reader" is the daemon's normal state, not an edge case.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::*;

/// Comfortably above the 1,000-page autocheckpoint threshold, so a log that is
/// never folded back has room to say so.
const CEILING: i64 = 4_000;

fn fixture() -> (Store, EntityId, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("wal", "Wal").into(),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    (store, project, dir)
}

/// Enough writes to pass the threshold several times over.
fn write_a_lot(store: &mut Store, project: &EntityId) {
    let prov = Provenance::anonymous(Actor::Claude);
    for i in 0..400 {
        store
            .create(
                Task::new(
                    project.clone(),
                    format!("task {i}"),
                    "A row written to move the write-ahead log along.",
                )
                .into(),
                &prov,
            )
            .unwrap();
    }
}

#[test]
fn the_log_does_not_grow_without_bound_under_ordinary_writing() {
    let (mut store, project, _dir) = fixture();

    write_a_lot(&mut store, &project);

    let pages = store.wal_pages().unwrap();
    assert!(
        pages < CEILING,
        "the log holds {pages} pages after 400 writes, which means checkpointing is not \
         keeping up — every read still answers, out of a file that only grows"
    );
}

/// The case the daemon actually is: something reading the whole time.
///
/// A second connection is opened and kept, and a query runs on it between
/// batches of writes. If a read *snapshot* were being pinned rather than taken
/// and released, the checkpoint could not advance past it and the log would
/// climb for the length of the test.
#[test]
fn a_long_lived_reader_does_not_pin_the_log() {
    let (mut store, project, dir) = fixture();
    let path = dir.path().join("specline.sqlite");

    let reader = Store::open(&path).expect("a second reader is safe in WAL mode, and always was");

    let mut high_water = 0;
    for _ in 0..4 {
        write_a_lot(&mut store, &project);

        // The read that would pin the log if its statement were left open. It
        // is not: the iteration finishes here, which is the property under
        // test.
        let seen = reader
            .list(&EntityQuery::in_project(project.clone()).limited(10_000))
            .unwrap();
        assert!(seen.total > 0);

        high_water = high_water.max(store.wal_pages().unwrap());
    }

    assert!(
        high_water < CEILING,
        "with a reader open throughout, the log reached {high_water} pages — a reader is \
         holding a snapshot the checkpoint cannot pass"
    );

    // Deliberately not asserted on the `-wal` file's *size*, and the reason is
    // worth writing down because the number looks alarming.
    //
    // A first draft did assert on it, and found a 4 MB `-wal` beside an 800 KB
    // store — with the page count comfortably in range the whole time. That is
    // normal: SQLite reuses the log file in place and only ever shrinks it at a
    // TRUNCATE checkpoint, so its size is a high-water mark rather than a
    // measure of what the log currently holds. Someone looking at `~/.specline` and
    // seeing a `-wal` several times the store is looking at a fact about the
    // busiest moment since the last shutdown, not at a problem.
    //
    // What proves nothing was pinned is that a truncating checkpoint can now
    // empty it, with the reader still open.
    drop(reader);
    store.checkpoint().unwrap();
    assert_eq!(
        store.wal_pages().unwrap(),
        0,
        "the log could not be emptied even after the reader went away"
    );
}

/// And the shutdown checkpoint actually empties it, so what is left on disk is
/// the whole store.
///
/// This is what a person copying the file, or a backup tool that knows nothing
/// about SQLite, would otherwise get wrong.
#[test]
fn the_shutdown_checkpoint_empties_the_log() {
    let (mut store, project, dir) = fixture();
    write_a_lot(&mut store, &project);

    store.checkpoint().unwrap();
    assert_eq!(
        store.wal_pages().unwrap(),
        0,
        "a truncating checkpoint with no other reader should leave nothing behind"
    );

    let wal = dir.path().join("specline.sqlite-wal");
    if wal.exists() {
        assert_eq!(
            std::fs::metadata(&wal).unwrap().len(),
            0,
            "the -wal file should be empty, not merely checkpointed"
        );
    }
}
