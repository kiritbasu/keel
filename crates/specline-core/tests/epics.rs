//! Epics: a task whose kind is `feature`, holding the tasks that build it.
//!
//! **Most of what this file was going to assert already existed.** KEEL-326
//! said `parent_id` "has existed since Phase 0 and has never been used", and
//! that is true of the data and false of the code: `check_task_parent` has
//! always refused a self-parent, a parent that is not a task, a parent in
//! another project, a cycle anywhere up the chain, and a tree deeper than
//! `MAX_PARENT_DEPTH`. Two inline tests in `store::entity` exercise it.
//!
//! So the only thing this phase adds is the *kind*. These tests pin the part
//! that is new, and pin the pre-existing guarantees at the level an epic will
//! actually rely on — because a rollup over children is only meaningful if the
//! tree cannot loop, and that guarantee now has a second caller depending on
//! it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Entity, EntityId, EntityStore, Project, Provenance, Store, Task, TaskKind,
};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_epics")
}

fn fixture() -> (tempfile::TempDir, Store, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let project = store
        .create(Project::new("specline", "Specline").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

fn task(store: &mut Store, project: &EntityId, title: &str, kind: TaskKind) -> EntityId {
    let mut t = Task::new(
        project.clone(),
        title,
        "A row this test needs in the store.",
    );
    t.kind = kind;
    store.create(t.into(), &prov()).unwrap().entity.id().clone()
}

fn child_of(
    store: &mut Store,
    project: &EntityId,
    title: &str,
    parent: &EntityId,
) -> specline_core::Result<EntityId> {
    let mut t = Task::new(
        project.clone(),
        title,
        "A row this test needs in the store.",
    );
    t.parent_id = Some(parent.clone());
    Ok(store.create(t.into(), &prov())?.entity.id().clone())
}

#[test]
fn feature_is_a_task_kind_and_the_refusal_lists_it() {
    assert_eq!(TaskKind::parse("feature").unwrap(), TaskKind::Feature);
    let err = TaskKind::parse("epic").unwrap_err().to_string();
    assert!(
        err.contains("feature"),
        "a caller reaching for `epic` has to be shown the word that works: {err}"
    );
}

#[test]
fn an_epic_holds_children() {
    let (_d, mut store, project) = fixture();
    let epic = task(&mut store, &project, "Codex support", TaskKind::Feature);

    let one = child_of(&mut store, &project, "Reach the endpoint", &epic).unwrap();
    child_of(&mut store, &project, "Write the setup command", &epic).unwrap();

    let Some(Entity::Task(child)) = store.get(&one).unwrap() else {
        panic!("a task");
    };
    assert_eq!(child.parent_id.as_ref(), Some(&epic));
    assert_eq!(
        store.task_counts(&project).unwrap().0,
        3,
        "an epic is itself a task, so three rows are open — what a rollup does \
         with that is the board's problem, not the store's"
    );
}

/// An epic is a task, so a milestone holds it like any other. That is what
/// makes "a phase contains features, improvements and bug fixes" work without
/// a second containment mechanism.
#[test]
fn an_epic_belongs_to_a_phase_like_any_task() {
    let (_d, mut store, project) = fixture();
    let phase = store
        .create(
            specline_core::Milestone::new(
                project.clone(),
                "Phase 14",
                "Everything between somebody wanting something and it being work.",
            )
            .into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let mut epic = Task::new(project.clone(), "Codex support", "One decided feature.");
    epic.kind = TaskKind::Feature;
    epic.milestone_id = Some(phase.clone());
    let epic = store
        .create(epic.into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();

    let Some(Entity::Task(stored)) = store.get(&epic).unwrap() else {
        panic!("a task");
    };
    assert_eq!(stored.milestone_id.as_ref(), Some(&phase));
}

// --- The guarantees a rollup leans on, which predate this phase -----------
//
// Not written here as new behaviour. They are pinned because an epic is the
// first thing that will walk the tree, and a walk is only safe if these hold.

#[test]
fn a_cycle_cannot_be_written_so_a_rollup_cannot_loop() {
    let (_d, mut store, project) = fixture();
    let epic = task(&mut store, &project, "Codex support", TaskKind::Feature);
    let child = child_of(&mut store, &project, "Reach the endpoint", &epic).unwrap();

    let version = store.get(&epic).unwrap().unwrap().audit().version;
    let mut changes = serde_json::Map::new();
    changes.insert(
        "parent_id".to_owned(),
        serde_json::Value::String(child.to_string()),
    );
    let err = store
        .update(&epic, version, &changes, &prov())
        .unwrap_err()
        .to_string();
    assert!(err.contains("ancestor"), "{err}");
}

#[test]
fn a_parent_that_does_not_exist_is_refused_rather_than_stored() {
    let (_d, mut store, project) = fixture();
    let absent = EntityId::generate(specline_core::EntityType::Task);

    let err = child_of(&mut store, &project, "An orphan", &absent)
        .unwrap_err()
        .to_string();
    assert!(err.contains("no task with id"), "{err}");
    assert_eq!(
        store.task_counts(&project).unwrap().0,
        0,
        "a refused create leaves no row behind"
    );
}

/// A child whose parent is elsewhere is invisible in both projects.
#[test]
fn a_parent_in_another_project_is_refused() {
    let (_d, mut store, project) = fixture();
    let other = store
        .create(Project::new("harbour", "Harbour").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    let epic = task(
        &mut store,
        &other,
        "Somebody else's epic",
        TaskKind::Feature,
    );

    let err = child_of(&mut store, &project, "A stray child", &epic)
        .unwrap_err()
        .to_string();
    assert!(err.contains("different project"), "{err}");
}
