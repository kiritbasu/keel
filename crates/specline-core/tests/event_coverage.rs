//! Every `Action` variant is something the store actually emits.
//!
//! `Action::Revised` was declared on the first day and never once constructed.
//! Nothing failed: the enum compiled, the changelog rendered, and a session
//! that only wrote prose simply left no trace in it. That is the shape of
//! drift this file is for — a declaration that has quietly stopped
//! corresponding to behaviour, with no error anywhere to say so.
//!
//! One scenario exercises every kind of write, and the guard asserts that
//! `Action::ALL` and the actions the log actually contains are the same set. A
//! seventh variant added without an emitter fails here, which is the only place
//! it would.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::store::EventScope;
use specline_core::*;
use std::collections::HashSet;

#[test]
fn every_action_variant_is_emitted_by_some_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let prov = Provenance::anonymous(Actor::Claude).with_session("ses_event_coverage");

    // Created — any create.
    let project = store
        .create(Project::new("events", "Events").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    let spec = store
        .create(Spec::new(project.clone(), "A specification").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    let task = store
        .create(
            Task::new(project.clone(), "A task", "A row this test needs.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    let doomed = store
        .create(
            Task::new(
                project.clone(),
                "To be archived",
                "A row this test archives.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    // Updated — a field that is not `status`.
    let mut changes = serde_json::Map::new();
    changes.insert("priority".to_owned(), serde_json::json!("p1"));
    store.update(&task, 1, &changes, &prov).unwrap();

    // StatusChanged — `status` specifically, which is its own action because
    // the activity feed and the roadmap care about transitions and nothing
    // else.
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("in_progress"));
    store.update(&task, 2, &changes, &prov).unwrap();

    // Linked.
    store
        .link(
            NewLink::new(task.clone(), Relation::Implements, spec.clone()),
            &prov,
        )
        .unwrap();

    // Revised.
    store
        .write_revision(
            Document::first(
                EntityType::Spec,
                spec,
                Some(project.clone()),
                "A specification",
                "Some prose, so there is a revision.\n",
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();

    // Archived.
    store.archive(&doomed, 1, &prov).unwrap();

    let seen: HashSet<Action> = store
        .recent_events(EventScope::Project(&project), 1_000)
        .unwrap()
        .items
        .into_iter()
        .map(|e| e.action)
        .collect();

    let never_emitted: Vec<Action> = Action::ALL
        .into_iter()
        .filter(|a| !seen.contains(a))
        .collect();

    assert!(
        never_emitted.is_empty(),
        "these Action variants are declared and nothing above emitted them: {never_emitted:?}\n\
         Either add the write that produces one to this test, or delete the variant. An \
         action nothing constructs is a hole in the changelog and the live feed that \
         reports itself as an empty history rather than as an error."
    );
}
