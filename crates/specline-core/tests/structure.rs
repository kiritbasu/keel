//! Rank, sub-tasks, and more than one link out.
//!
//! Two of these carry a hazard the other fields do not. A rank that collides
//! makes an order that looks deliberate and is arbitrary; a parent that closes
//! a cycle makes a tree that nothing can finish walking. Both are rejected on
//! the way in, because the store is the only place that can see the whole set.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Entity, EntityId, EntityStore, Project, Provenance, Store, Task, types::MAX_PARENT_DEPTH,
};

fn store() -> (tempfile::TempDir, Store, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("specline", "Specline").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude)
}

fn task(store: &mut Store, project: &EntityId, title: &str) -> Task {
    match store
        .create(
            Task::new(
                project.clone(),
                title,
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap()
        .entity
    {
        Entity::Task(t) => t,
        other => panic!("expected a task, got {}", other.entity_type()),
    }
}

fn reload(store: &Store, id: &EntityId) -> Task {
    match store.get(id).unwrap() {
        Some(Entity::Task(t)) => t,
        _ => panic!("task {id} vanished"),
    }
}

fn set(
    store: &mut Store,
    task: &Task,
    field: &str,
    value: serde_json::Value,
) -> specline_core::Result<Entity> {
    let mut changes = serde_json::Map::new();
    changes.insert(field.to_owned(), value);
    store.update(&task.id, task.audit.version, &changes, &prov())
}

// --- Rank ----------------------------------------------------------------

#[test]
fn new_work_lands_at_the_end_of_the_order() {
    let (_d, mut store, project) = store();
    let first = task(&mut store, &project, "First");
    let second = task(&mut store, &project, "Second");
    assert!(
        second.rank > first.rank,
        "a task nobody has placed has not been prioritised; putting it at the top would be the \
         store making that claim on their behalf"
    );
}

#[test]
fn a_place_between_two_neighbours_is_their_midpoint() {
    let (_d, store, _project) = store();
    assert_eq!(store.rank_between(Some(1.0), Some(2.0)).unwrap(), 1.5);
    assert_eq!(store.rank_between(Some(2.0), None).unwrap(), 3.0);
    assert_eq!(store.rank_between(None, Some(2.0)).unwrap(), 1.0);
    assert_eq!(store.rank_between(None, None).unwrap(), 1.0);
}

// Failure case. Two neighbours with the same rank have no space between them,
// and inventing one would produce a third task tied with both — an order that
// looks deliberate and is arbitrary.
#[test]
fn there_is_no_space_between_two_identical_ranks() {
    let (_d, store, _project) = store();
    let err = store.rank_between(Some(1.0), Some(1.0)).unwrap_err();
    assert!(err.to_string().contains("no space"), "{err}");
}

#[test]
fn a_midpoint_actually_reorders_the_list() {
    let (_d, mut store, project) = store();
    let a = task(&mut store, &project, "A");
    let b = task(&mut store, &project, "B");
    let c = task(&mut store, &project, "C");

    // Move C between A and B.
    let between = store.rank_between(Some(a.rank), Some(b.rank)).unwrap();
    set(&mut store, &c, "rank", serde_json::json!(between)).unwrap();

    let mut order = [
        reload(&store, &a.id),
        reload(&store, &b.id),
        reload(&store, &c.id),
    ];
    order.sort_by(|x, y| x.rank.total_cmp(&y.rank));
    assert_eq!(
        order.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(),
        vec!["A", "C", "B"]
    );
}

// --- Sub-tasks -----------------------------------------------------------

#[test]
fn a_task_can_be_part_of_another() {
    let (_d, mut store, project) = store();
    let parent = task(&mut store, &project, "The epic");
    let child = task(&mut store, &project, "A piece of it");

    set(
        &mut store,
        &child,
        "parent_id",
        serde_json::json!(parent.id.as_str()),
    )
    .unwrap();
    assert_eq!(reload(&store, &child.id).parent_id, Some(parent.id));
}

// Failure cases. Each of these produces a tree that cannot be rendered or
// rolled up, and only the store can see enough to refuse it.
#[test]
fn a_task_cannot_be_its_own_parent() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Self-referential");
    let err = set(
        &mut store,
        &t,
        "parent_id",
        serde_json::json!(t.id.as_str()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("its own parent"), "{err}");
}

#[test]
fn a_cycle_is_refused_however_long_the_way_round() {
    let (_d, mut store, project) = store();
    let a = task(&mut store, &project, "A");
    let b = task(&mut store, &project, "B");
    let c = task(&mut store, &project, "C");

    set(
        &mut store,
        &b,
        "parent_id",
        serde_json::json!(a.id.as_str()),
    )
    .unwrap();
    let c = match set(
        &mut store,
        &c,
        "parent_id",
        serde_json::json!(b.id.as_str()),
    )
    .unwrap()
    {
        Entity::Task(t) => t,
        other => panic!("expected a task, got {}", other.entity_type()),
    };
    let _ = c;

    // A under C would close the loop A → B → C → A.
    let a = reload(&store, &a.id);
    let err = set(
        &mut store,
        &a,
        "parent_id",
        serde_json::json!(c.id.as_str()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("own ancestor"), "{err}");
}

#[test]
fn a_parent_in_another_project_is_refused() {
    let (_d, mut store, project) = store();
    let elsewhere = match store
        .create(Project::new("other", "Other").into(), &prov())
        .unwrap()
        .entity
    {
        Entity::Project(p) => p.id,
        other => panic!("expected a project, got {}", other.entity_type()),
    };
    let parent = task(&mut store, &elsewhere, "Somewhere else");
    let child = task(&mut store, &project, "Here");

    let err = set(
        &mut store,
        &child,
        "parent_id",
        serde_json::json!(parent.id.as_str()),
    )
    .unwrap_err();
    assert!(err.to_string().contains("different project"), "{err}");
}

#[test]
fn a_parent_that_does_not_exist_is_refused() {
    let (_d, mut store, project) = store();
    let child = task(&mut store, &project, "Orphan");
    let err = set(
        &mut store,
        &child,
        "parent_id",
        serde_json::json!("tsk_01KZKMPVJDS6QYBKSTNA938HDV"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("no task with id"), "{err}");
}

#[test]
fn the_tree_has_a_depth_limit() {
    let (_d, mut store, project) = store();
    let mut previous: Option<EntityId> = None;
    let mut last_error = None;

    for depth in 0..(MAX_PARENT_DEPTH + 3) {
        let t = task(&mut store, &project, &format!("Level {depth}"));
        if let Some(parent) = &previous {
            match set(
                &mut store,
                &t,
                "parent_id",
                serde_json::json!(parent.as_str()),
            ) {
                Ok(_) => previous = Some(t.id),
                Err(e) => {
                    last_error = Some(e);
                    break;
                }
            }
        } else {
            previous = Some(t.id);
        }
    }

    let err = last_error.expect("a chain deeper than the limit is refused");
    assert!(err.to_string().contains("deeper than"), "{err}");
}

// --- External links ------------------------------------------------------

#[test]
fn a_task_holds_more_than_one_link() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Spans a PR and an issue");
    set(
        &mut store,
        &t,
        "external_refs",
        serde_json::json!([
            "https://github.com/kb/specline/pull/1",
            "https://github.com/kb/specline/issues/2"
        ]),
    )
    .unwrap();
    assert_eq!(reload(&store, &t.id).external_refs.len(), 2);
}

#[test]
fn a_new_task_has_no_links_rather_than_one_empty_one() {
    let (_d, mut store, project) = store();
    assert!(task(&mut store, &project, "Fresh").external_refs.is_empty());
}
