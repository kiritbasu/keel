//! Archiving, and what it must not break.
//!
//! Written while chasing a live failure: archiving a project left the running
//! daemon returning storage errors for every project list and task lookup until
//! it was restarted. These assert the *store* is fine — which it is, and that is
//! what localised the bug to the daemon's process state rather than the data.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use keel_core::{
    Actor, DuckStore, EntityQuery, EntityStore, EntityType, Project, Provenance, Task,
};

#[test]
fn listing_projects_still_works_after_one_is_archived() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DuckStore::open(dir.path()).unwrap();
    let prov = Provenance::anonymous(Actor::Human);

    let keep = store
        .create(Project::new("keep", "Keep").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    let drop = store
        .create(Project::new("drop", "Drop").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();

    // The live failure had a task left behind under the archived project.
    store
        .create(Task::new(drop.clone(), "Left behind").into(), &prov)
        .unwrap();

    store.archive(&drop, 1, &prov).unwrap();

    let page = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .unwrap();
    assert_eq!(page.items.len(), 1, "the archived project is hidden");
    assert_eq!(page.items[0].id(), &keep);
    let _ = page.total;
}

#[test]
fn a_checkpointed_store_reopens_and_accepts_writes() {
    // The p0. An UPDATE on a store whose ART index disagreed with its table
    // raised a DuckDB FATAL, which poisons the connection so every later query
    // fails with whatever operation happened to be running — "count matching
    // rows", "run a question lookup". Reads on a fresh process worked, `fsck`
    // reported clean, and both were true, which is why it took an evening.
    //
    // The cause was SIGKILL mid-write, because SIGTERM could not stop a daemon
    // holding an open SSE stream. The daemon now checkpoints on shutdown; this
    // asserts the store survives that cycle and still takes a write.
    let dir = tempfile::tempdir().unwrap();
    let prov = Provenance::anonymous(Actor::Human);

    let id = {
        let mut store = DuckStore::open(dir.path()).unwrap();
        let id = store
            .create(Project::new("p", "P").into(), &prov)
            .unwrap()
            .entity
            .id()
            .clone();
        store
            .create(Task::new(id.clone(), "Something to update").into(), &prov)
            .unwrap();
        store.checkpoint().unwrap();
        id
    };

    let mut store = DuckStore::open(dir.path()).unwrap();
    let page = store
        .list(&EntityQuery::in_project(id.clone()).of_type(EntityType::Task))
        .unwrap();
    assert_eq!(page.items.len(), 1, "the task survived the checkpoint");

    // The operation that used to raise FATAL.
    let task = page.items[0].clone();
    let mut changes = serde_json::Map::new();
    changes.insert("priority".to_owned(), serde_json::json!("p0"));
    store
        .update(task.id(), task.audit().version, &changes, &prov)
        .expect("an update after a checkpoint-and-reopen must not fail");

    // And the store is still usable afterwards, which a FATAL would prevent.
    store
        .list(&EntityQuery::in_project(id).of_type(EntityType::Task))
        .expect("the connection is not poisoned");
}
