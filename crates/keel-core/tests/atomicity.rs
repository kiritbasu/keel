//! A write lands whole, or it does not land.
//!
//! Every entity mutation runs several statements: a row, sometimes its links,
//! and one or more events. Until Phase 11 none of them were bracketed, so a
//! process killed between two statements left a row with no event — and the
//! idempotent retry returns `created: false` before re-writing anything, so
//! that history was gone permanently. Nothing looked broken. The store simply
//! contained an entity that had, as far as any reader could tell, always been
//! there.
//!
//! The faults here are the same crash, made deterministic: deny the `events`
//! insert and the process may as well have died between the two statements.
//! Every one of these tests fails against the pre-transaction code.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/faults.rs"]
mod faults;

use chrono::Utc;
use keel_core::{
    Action, Actor, Cursor, Direction, Document, EntityId, EntityStore, EntityType, GraphStore,
    NewLink, Project, Provenance, Relation, Spec, Store, Surface, Task,
};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_atomicity")
}

fn fixture() -> (tempfile::TempDir, Store, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let project = store
        .create(Project::new("keel", "Keel").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

fn task(store: &mut Store, project: &EntityId, title: &str) -> EntityId {
    store
        .create(
            Task::new(project.clone(), title, "A row this test needs.").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone()
}

/// How many rows exist in a table, asked directly so an assertion cannot be
/// fooled by a filter in the read path.
fn rows(store: &Store, table: &str, predicate: &str) -> i64 {
    store
        .connection()
        .query_row(
            &format!("SELECT count(*) FROM {table} WHERE {predicate}"),
            [],
            |r| r.get(0),
        )
        .unwrap()
}

// --- Create --------------------------------------------------------------

#[test]
fn a_create_that_cannot_write_its_event_writes_no_row_either() {
    let (_d, mut store, project) = fixture();

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let attempt = store.create(
        Task::new(project.clone(), "A task nobody will see", "Summary.").into(),
        &prov(),
    );
    faults::clear(store.connection());

    assert!(attempt.is_err(), "the create should have failed outright");
    fault.assert_fired(1);
    assert_eq!(
        rows(&store, "tasks", "title = 'A task nobody will see'"),
        0,
        "the row survived without its event — this is the orphan the transaction exists to prevent"
    );
}

/// The reason an orphaned row is unrecoverable rather than merely untidy.
///
/// A retry finds the row by idempotency key and returns it with
/// `created: false` without ever reaching the event append, so the history is
/// not repaired on the second attempt. It is gone.
#[test]
fn a_retry_after_a_failed_create_does_not_backfill_the_missing_event() {
    let (_d, mut store, project) = fixture();
    let title = "A task written twice";

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let _ = store.create(
        Task::new(project.clone(), title, "Summary.").into(),
        &prov(),
    );
    faults::clear(store.connection());

    fault.assert_fired(1);
    let again = store
        .create(
            Task::new(project.clone(), title, "Summary.").into(),
            &prov(),
        )
        .expect("the retry succeeds");

    let events = store.events_for(again.entity.id(), 100).unwrap();
    assert!(
        events.items.iter().any(|e| e.action == Action::Created),
        "the task has no creation event, and no future call will ever write one"
    );
}

// --- Update --------------------------------------------------------------

#[test]
fn an_update_that_cannot_write_its_event_does_not_move_the_row() {
    let (_d, mut store, project) = fixture();
    let id = task(&mut store, &project, "Original title");
    let before = store.get(&id).unwrap().unwrap();

    let mut changes = serde_json::Map::new();
    changes.insert("title".into(), serde_json::json!("Changed title"));

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let attempt = store.update(&id, before.audit().version, &changes, &prov());
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);

    let after = store.get(&id).unwrap().unwrap();
    assert_eq!(
        after.label(),
        "Original title",
        "the row moved while its event was lost"
    );
    assert_eq!(
        after.audit().version,
        before.audit().version,
        "the version bumped with no event to explain it, which the next \
         optimistic-concurrency check would happily accept"
    );
}

// --- Archive -------------------------------------------------------------

#[test]
fn an_archive_that_cannot_write_its_event_leaves_the_links_live() {
    let (_d, mut store, project) = fixture();
    let blocker = task(&mut store, &project, "The blocker");
    let blocked = task(&mut store, &project, "The blocked thing");
    store
        .link(
            NewLink::new(blocker.clone(), Relation::Blocks, blocked.clone()),
            &prov(),
        )
        .unwrap();

    let version = store.get(&blocker).unwrap().unwrap().audit().version;
    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let attempt = store.archive(&blocker, version, &prov());
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);

    assert!(
        !store.get(&blocker).unwrap().unwrap().audit().is_archived(),
        "the row was archived without the event that records it"
    );
    let live = store.links_of(&blocker, Direction::Outbound).unwrap();
    assert_eq!(
        live.len(),
        1,
        "the links were archived by a write that then failed, leaving the graph \
         and the row disagreeing about whether this task exists"
    );
}

// --- Link and unlink -----------------------------------------------------

#[test]
fn a_link_that_cannot_write_its_event_creates_no_edge() {
    let (_d, mut store, project) = fixture();
    let a = task(&mut store, &project, "One end");
    let b = task(&mut store, &project, "The other end");

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let attempt = store.link(
        NewLink::new(a.clone(), Relation::Blocks, b.clone()),
        &prov(),
    );
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);
    assert_eq!(
        store.links_of(&a, Direction::Outbound).unwrap().len(),
        0,
        "an edge with no event is an edge nothing in the changelog explains"
    );
}

#[test]
fn an_unlink_that_cannot_write_its_event_leaves_the_edge_live() {
    let (_d, mut store, project) = fixture();
    let a = task(&mut store, &project, "One end");
    let b = task(&mut store, &project, "The other end");
    store
        .link(
            NewLink::new(a.clone(), Relation::Blocks, b.clone()),
            &prov(),
        )
        .unwrap();

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let attempt = store.unlink(&a, Relation::Blocks, &b, "", &prov());
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);
    assert_eq!(
        store.links_of(&a, Direction::Outbound).unwrap().len(),
        1,
        "the edge was archived and nothing recorded it"
    );
}

// --- Revisions -----------------------------------------------------------

fn write_body(store: &mut Store, id: &EntityId, project: &EntityId, body: &str) -> Document {
    let doc = Document::first(
        EntityType::Spec,
        id.clone(),
        Some(project.clone()),
        "A specification",
        body,
        Actor::Claude,
        Utc::now(),
    )
    .unwrap()
    .attributed(Some("ses_atomicity".to_owned()), Some(Surface::Code));
    store.write_revision(doc).unwrap()
}

/// `Action::Revised` was declared from the first day and never constructed, so
/// a session that only wrote prose left no trace in the changelog or the live
/// feed at all.
#[test]
fn writing_a_revision_appends_a_revised_event() {
    let (_d, mut store, project) = fixture();
    let spec = store
        .create(
            Spec::new(project.clone(), "A specification").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    write_body(&mut store, &spec, &project, "The first wording.");
    write_body(&mut store, &spec, &project, "The second wording.");

    let events = store.events_for(&spec, 100).unwrap();
    let revised: Vec<_> = events
        .items
        .iter()
        .filter(|e| e.action == Action::Revised)
        .collect();
    assert_eq!(
        revised.len(),
        2,
        "two revisions should have left two events; found {:?}",
        events.items.iter().map(|e| e.action).collect::<Vec<_>>()
    );
    assert!(
        revised[1].summary.contains("v2"),
        "the sentence should name the version it wrote: {:?}",
        revised[1].summary
    );
    assert_eq!(
        revised[0].session_id.as_deref(),
        Some("ses_atomicity"),
        "the event's provenance should be the revision's, not a fresh guess"
    );

    // And it reaches the project feed, which is what the changelog and the
    // desktop app actually read.
    let feed = store
        .events(&Cursor::Beginning, Some(&project), 1_000)
        .unwrap();
    assert!(
        feed.items
            .iter()
            .any(|e| e.action == Action::Revised && e.entity_id == spec),
        "a revision that never reaches the project feed is a session that \
         vanished from the changelog"
    );
}

/// Identical content is a no-op, so it must not manufacture an event either.
#[test]
fn rewriting_identical_content_appends_no_event() {
    let (_d, mut store, project) = fixture();
    let spec = store
        .create(
            Spec::new(project.clone(), "A specification").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    write_body(&mut store, &spec, &project, "The same wording.");
    write_body(&mut store, &spec, &project, "The same wording.");

    let revised = store
        .events_for(&spec, 100)
        .unwrap()
        .items
        .into_iter()
        .filter(|e| e.action == Action::Revised)
        .count();
    assert_eq!(revised, 1, "a no-op write invented a revision");
}

#[test]
fn a_revision_that_cannot_write_its_event_writes_no_revision() {
    let (_d, mut store, project) = fixture();
    let spec = store
        .create(
            Spec::new(project.clone(), "A specification").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    write_body(&mut store, &spec, &project, "The first wording.");

    let fault = faults::deny_insert_after(store.connection(), "events", 1);
    let doc = Document::first(
        EntityType::Spec,
        spec.clone(),
        Some(project.clone()),
        "A specification",
        "A wording that must not survive.",
        Actor::Claude,
        Utc::now(),
    )
    .unwrap();
    let attempt = store.write_revision(doc);
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);

    let current = store.revision(&spec, None).unwrap().unwrap();
    assert_eq!(
        current.version, 1,
        "the failed revision was committed anyway"
    );
    assert_eq!(
        current.body, "The first wording.",
        "the store is showing a body whose write did not complete"
    );
    assert_eq!(
        rows(&store, "documents", "status = 'current'"),
        1,
        "the demotion committed without its replacement, leaving no current revision"
    );
}
