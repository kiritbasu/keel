//! One row's story: its history, and the labels on what it is linked to.
//!
//! Both exist because of the same defect in different clothes. A traversal that
//! returns bare ids, and an activity feed that can only be read a project at a
//! time, both push the same work onto every caller — go and look up what these
//! ids are, or go and page a whole log and filter it. Every caller then does it
//! slightly differently, and the ones that give up show a ULID where a name
//! belongs.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use keel_core::{
    Actor, Direction, EntityId, EntityStore, GraphStore, NewLink, Project, Provenance, Relation,
    Spec, SqliteStore, Task,
};

fn store() -> (tempfile::TempDir, SqliteStore, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = SqliteStore::open(dir.path().join("keel.sqlite")).unwrap();
    let prov = Provenance::anonymous(Actor::Human);
    let project = store
        .create(Project::new("keel", "Keel").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

fn task(store: &mut SqliteStore, project: &EntityId, title: &str) -> EntityId {
    store
        .create(
            Task::new(
                project.clone(),
                title,
                "A row this test needs in the store.",
            )
            .into(),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap()
        .entity
        .id()
        .clone()
}

// --- Labels on traversal results ----------------------------------------

#[test]
fn a_neighbour_says_what_it_is_called() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let blocker = task(&mut store, &project, "A real design system");
    let blocked = task(&mut store, &project, "One page shell for every screen");

    store
        .link(
            NewLink::new(blocker.clone(), Relation::Blocks, blocked.clone()),
            &prov,
        )
        .unwrap();

    let inbound = store
        .neighbours(&blocked, Direction::Inbound, &[], 1)
        .unwrap();
    assert_eq!(inbound.len(), 1);
    assert_eq!(inbound[0].id, blocker);
    assert_eq!(
        inbound[0].label, "A real design system",
        "a traversal result that is only an id cannot be rendered without a second lookup"
    );

    let outbound = store
        .neighbours(&blocker, Direction::Outbound, &[], 1)
        .unwrap();
    assert_eq!(outbound[0].label, "One page shell for every screen");
}

#[test]
fn a_label_is_resolved_whatever_the_type_calls_its_name_column() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let t = task(&mut store, &project, "Implement hybrid search");
    // `specs` calls it `title`, `projects` calls it `name`; the vertex view is
    // what makes one traversal able to label both.
    let spec = store
        .create(
            Spec::new(project.clone(), "Keel — Technical Specification").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    store
        .link(NewLink::new(t.clone(), Relation::Implements, spec), &prov)
        .unwrap();

    let out = store.neighbours(&t, Direction::Outbound, &[], 1).unwrap();
    assert_eq!(out[0].label, "Keel — Technical Specification");
}

// Failure case. A dangling edge must still come back — losing the row would
// hide exactly the breakage `fsck`'s dangling-link check exists to report, and
// would turn a visible integrity problem into a silently shorter graph.
#[test]
fn an_edge_pointing_at_nothing_keeps_the_edge_and_loses_only_the_label() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let from = task(&mut store, &project, "Still here");
    let to = task(&mut store, &project, "About to vanish");
    store
        .link(
            NewLink::new(from.clone(), Relation::Blocks, to.clone()),
            &prov,
        )
        .unwrap();

    // Hard-delete the target behind the store's back. Nothing in Keel does
    // this — that is the point: this is the state a crash or a bad restore
    // leaves, and it is what fsck audits for.
    store
        .connection()
        .execute("DELETE FROM tasks WHERE id = ?", [to.as_str()])
        .unwrap();

    let out = store
        .neighbours(&from, Direction::Outbound, &[], 1)
        .unwrap();
    assert_eq!(out.len(), 1, "the broken edge must remain visible");
    assert_eq!(out[0].id, to);
    assert_eq!(
        out[0].label, "",
        "with nothing invented in place of the name"
    );
}

// --- One row's history ---------------------------------------------------

#[test]
fn a_rows_history_is_its_own_and_reads_forwards() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let mine = task(&mut store, &project, "The task detail view");
    let other = task(&mut store, &project, "Something else entirely");

    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("in_progress"));
    store.update(&mine, 1, &changes, &prov).unwrap();

    let mut changes = serde_json::Map::new();
    changes.insert("priority".to_owned(), serde_json::json!("p0"));
    store.update(&mine, 2, &changes, &prov).unwrap();

    // `review` rather than `done`: the subject here is whose history an event
    // lands in, and closing would drag in the reason, message and evidence a
    // terminal transition now demands.
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("review"));
    store.update(&other, 1, &changes, &prov).unwrap();

    let page = store.events_for(&mine, 50).unwrap();
    assert!(
        page.items.iter().all(|e| e.entity_id == mine),
        "another row's changes must not appear in this row's history"
    );
    assert_eq!(page.items.len(), 3, "created, then two updates");
    assert_eq!(page.items[0].action, keel_core::Action::Created);

    let ids: Vec<_> = page
        .items
        .iter()
        .map(|e| e.id.as_str().to_owned())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted, "oldest first — a history reads forwards");
}

// Failure case. A row nothing has happened to is an empty history, not an
// error: the detail view renders it as "nothing yet", and an error there would
// read as a broken page.
#[test]
fn a_row_with_no_history_returns_an_empty_page() {
    let (_d, store, _project) = store();
    let nobody = EntityId::parse("tsk_01KZKMPVJDS6QYBKSTNA938HDV").unwrap();
    let page = store.events_for(&nobody, 50).unwrap();
    assert!(page.items.is_empty());
    assert_eq!(page.total, 0);
}

// Failure case for hard constraint 4: a cut list says it was cut, with a total.
#[test]
fn a_truncated_history_reports_the_total_it_was_cut_from() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let t = task(&mut store, &project, "Much amended");
    for version in 1..=5 {
        let mut changes = serde_json::Map::new();
        changes.insert(
            "title".to_owned(),
            serde_json::json!(format!("Rename {version}")),
        );
        store.update(&t, version, &changes, &prov).unwrap();
    }

    let page = store.events_for(&t, 2).unwrap();
    assert_eq!(page.items.len(), 2);
    assert!(page.truncated, "two of six is a cut list and must say so");
    assert_eq!(page.total, 6, "created, then five renames");
}
