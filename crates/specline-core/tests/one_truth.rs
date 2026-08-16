//! The tracker stops contradicting itself.
//!
//! Three defects that shared a shape: a fact held in two places that could
//! disagree, or a fact held nowhere at all.
//!
//! - **Blocked** was a status *and* a graph edge, and two tasks in Specline's own
//!   store were marked blocked with nothing linked to them.
//! - **`closed_at`** was written by no live path, so every finished task had no
//!   completion date and "what closed this week" was unanswerable.
//! - **The tracker file** was overwritten unconditionally, so a render pointed
//!   at the wrong project would silently replace it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Entity, EntityId, EntityStore, NewLink, Project, Provenance, Relation, Store, Task,
    TaskStatus, next::blocked_tasks,
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

/// Move a task's status, supplying whatever a close now demands.
///
/// A terminal status needs a reason, a message and — for `done` — evidence, so
/// the helper carries them. Written here rather than in each test because the
/// subject of these tests is `closed_at`, and a close that says why is now
/// simply what a close looks like.
fn set_status(store: &mut Store, t: &Task, status: &str) -> Task {
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!(status));
    if status == "done" || status == "wont_do" {
        changes.insert("close_reason".to_owned(), serde_json::json!(status));
        changes.insert(
            "close_message".to_owned(),
            serde_json::json!("Closed by a test that is about the completion date."),
        );
        changes.insert(
            "evidence".to_owned(),
            serde_json::json!(["test:cargo test -p specline-core --test one_truth"]),
        );
    }
    match store
        .update(&t.id, t.audit.version, &changes, &prov())
        .unwrap()
    {
        Entity::Task(t) => t,
        other => panic!("expected a task, got {}", other.entity_type()),
    }
}

// --- One definition of blocked -------------------------------------------

#[test]
fn blocked_means_an_edge_and_only_an_edge() {
    let (_d, mut store, project) = store();
    let blocker = task(&mut store, &project, "Must happen first");
    let waiting = task(&mut store, &project, "Waiting on it");
    let free = task(&mut store, &project, "Nothing in the way");

    assert!(
        blocked_tasks(&store, &project).unwrap().is_empty(),
        "no edges yet, so nothing is blocked"
    );

    store
        .link(
            NewLink::new(blocker.id.clone(), Relation::Blocks, waiting.id.clone()),
            &prov(),
        )
        .unwrap();

    let blocked = blocked_tasks(&store, &project).unwrap();
    assert!(blocked.contains(&waiting.id));
    assert!(!blocked.contains(&free.id));
    assert!(
        !blocked.contains(&blocker.id),
        "the blocker is not itself blocked"
    );
}

// The failure this replaced. A finished blocker is not a blocker, and treating
// it as one freezes work forever behind something already done.
#[test]
fn a_finished_blocker_stops_blocking() {
    let (_d, mut store, project) = store();
    let blocker = task(&mut store, &project, "Will be finished");
    let waiting = task(&mut store, &project, "Waiting on it");
    store
        .link(
            NewLink::new(blocker.id.clone(), Relation::Blocks, waiting.id.clone()),
            &prov(),
        )
        .unwrap();
    assert!(
        blocked_tasks(&store, &project)
            .unwrap()
            .contains(&waiting.id)
    );

    store
        .archive(&blocker.id, blocker.audit.version, &prov())
        .unwrap();
    assert!(
        !blocked_tasks(&store, &project)
            .unwrap()
            .contains(&waiting.id),
        "an archived blocker is not in the way"
    );
}

// The contradiction is now unrepresentable rather than merely detectable: there
// is no `blocked` to write into the status column.
#[test]
fn there_is_no_blocked_status_to_disagree_with_the_edges() {
    assert!(
        TaskStatus::parse("blocked").is_err(),
        "`blocked` must not be a status — it is derived from the graph (TQ-25)"
    );
    assert!(!TaskStatus::ALL.iter().any(|s| s.as_str() == "blocked"));
}

// --- closed_at -----------------------------------------------------------

#[test]
fn finishing_a_task_records_when() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Gets finished");
    assert!(t.closed_at.is_none());

    let done = set_status(&mut store, &t, "done");
    assert!(
        done.closed_at.is_some(),
        "every finished task in the store had no completion date, which made throughput \
         unanswerable"
    );
}

#[test]
fn abandoning_a_task_counts_as_closing_it() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Gets dropped");
    assert!(set_status(&mut store, &t, "wont_do").closed_at.is_some());
}

// Failure case: a reopened task must not keep a completion date, or every
// question that filters on one counts it as closed.
#[test]
fn reopening_a_task_clears_the_date() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Finished, then not");
    let done = set_status(&mut store, &t, "done");
    assert!(done.closed_at.is_some());

    let reopened = set_status(&mut store, &done, "todo");
    assert!(
        reopened.closed_at.is_none(),
        "a task that is open again was not closed"
    );
}

#[test]
fn an_explicit_date_wins_over_the_derived_one() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Closed earlier than recorded");

    // Backfilling a historical close is the case this exists for: the status
    // and the real date arrive together and the store must not overwrite it
    // with `now`.
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("done"));
    changes.insert(
        "closed_at".to_owned(),
        serde_json::json!("2026-01-01T00:00:00Z"),
    );
    changes.insert("close_reason".to_owned(), serde_json::json!("done"));
    changes.insert(
        "close_message".to_owned(),
        serde_json::json!("Backfilled from the event log, with the date it actually closed."),
    );
    changes.insert(
        "evidence".to_owned(),
        serde_json::json!(["test:cargo test -p specline-core --test one_truth"]),
    );
    let updated = match store
        .update(&t.id, t.audit.version, &changes, &prov())
        .unwrap()
    {
        Entity::Task(t) => t,
        other => panic!("expected a task, got {}", other.entity_type()),
    };
    assert_eq!(
        updated.closed_at.map(|d| d.format("%Y").to_string()),
        Some("2026".to_owned())
    );
}

#[test]
fn a_status_change_that_stays_open_leaves_the_date_alone() {
    let (_d, mut store, project) = store();
    let t = task(&mut store, &project, "Still going");
    let moved = set_status(&mut store, &t, "in_progress");
    assert!(moved.closed_at.is_none());
}
