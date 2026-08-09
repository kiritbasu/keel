//! Archiving, and what it must not break.
//!
//! Written while chasing a live failure: archiving a project left the running
//! daemon returning storage errors for every project list and task lookup until
//! it was restarted. These assert the *store* is fine — which it is, and that is
//! what localised the bug to the daemon's process state rather than the data.

#![allow(clippy::unwrap_used)]
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
