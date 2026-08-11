//! Idempotency, optimistic concurrency, and the event log.
//!
//! REQ-7 is the requirement under test: "concurrent agent writes are safe:
//! creates are idempotent by key, updates use optimistic concurrency and
//! reject stale writes." Agents retry, and they retry in parallel.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::*;
use serde_json::json;

fn store() -> (DuckStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = DuckStore::open(dir.path()).unwrap();
    (store, dir)
}

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_a")
}

fn project(store: &mut DuckStore) -> EntityId {
    store
        .create(Project::new("keel", "Keel").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone()
}

// --- Idempotency (SPEC §7.2) --------------------------------------------

#[test]
fn the_same_create_twice_returns_the_same_entity_with_created_false() {
    let (mut s, _d) = store();
    let p = project(&mut s);

    let first = s
        .create(
            Task::new(
                p.clone(),
                "Add the login page",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    let second = s
        .create(
            Task::new(
                p,
                "Add the login page",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();

    assert!(first.created);
    assert!(!second.created, "a retry must not create a second row");
    assert_eq!(first.entity.id(), second.entity.id());
}

#[test]
fn idempotency_survives_trivial_title_differences() {
    // R-6, write amplification: an over-eager agent creating dozens of
    // near-identical tasks would make the store worse than useless.
    let (mut s, _d) = store();
    let p = project(&mut s);

    let a = s
        .create(
            Task::new(
                p.clone(),
                "Add the login page",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    let b = s
        .create(
            Task::new(
                p,
                "  add   THE   Login   Page  ",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    assert_eq!(a.entity.id(), b.entity.id());
    assert!(!b.created);
}

#[test]
fn a_caller_supplied_key_overrides_the_derived_one() {
    let (mut s, _d) = store();
    let p = project(&mut s);

    // Two genuinely different tasks that happen to share a title.
    let mut one: Entity =
        Task::new(p.clone(), "Deploy", "A row this test needs in the store.").into();
    one.set_idempotency_key("deploy-staging");
    let mut two: Entity = Task::new(p, "Deploy", "A row this test needs in the store.").into();
    two.set_idempotency_key("deploy-production");

    let a = s.create(one, &prov()).unwrap();
    let b = s.create(two, &prov()).unwrap();
    assert!(a.created);
    assert!(
        b.created,
        "distinct explicit keys must produce distinct rows"
    );
    assert_ne!(a.entity.id(), b.entity.id());
}

#[test]
fn idempotency_does_not_leak_across_projects() {
    let (mut s, _d) = store();
    let p1 = project(&mut s);
    let p2 = s
        .create(Project::new("other", "Other").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();

    let a = s
        .create(
            Task::new(p1, "Ship it", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap();
    let b = s
        .create(
            Task::new(p2, "Ship it", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap();
    assert!(a.created && b.created);
    assert_ne!(a.entity.id(), b.entity.id());
}

#[test]
fn recreating_an_archived_entity_returns_the_archived_one() {
    // Deliberate: the unique index covers archived rows, so minting a second
    // row beside an archived one is how a store fills with near-duplicates.
    // The agent gets the original back and can restore or rename it.
    let (mut s, _d) = store();
    let p = project(&mut s);

    let original = s
        .create(
            Task::new(
                p.clone(),
                "Retire the importer",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap()
        .entity;
    s.archive(original.id(), 1, &prov()).unwrap();

    let again = s
        .create(
            Task::new(
                p,
                "Retire the importer",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    assert!(!again.created);
    assert_eq!(again.entity.id(), original.id());
    assert!(again.entity.audit().is_archived());
}

// --- Optimistic concurrency (SPEC §7.3) ---------------------------------

#[test]
fn a_stale_update_is_rejected_and_reports_the_current_version() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Ship the daemon", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;

    // Two agents both read version 1.
    let seen_by_a = task.audit().version;
    let seen_by_b = task.audit().version;

    s.update(
        task.id(),
        seen_by_a,
        json!({"status": "in_progress"}).as_object().unwrap(),
        &prov(),
    )
    .expect("the first writer wins");

    let err = s
        .update(
            task.id(),
            seen_by_b,
            json!({"status": "done"}).as_object().unwrap(),
            &prov(),
        )
        .unwrap_err();

    assert!(
        err.is_conflict(),
        "should be a conflict, not a generic error"
    );
    match err {
        Error::StaleVersion {
            supplied, latest, ..
        } => {
            assert_eq!(supplied, 1);
            assert_eq!(latest, 2, "the caller must be told what to re-read");
        }
        other => panic!("expected StaleVersion, got {other}"),
    }

    // And the losing write did not land.
    let current = s.get(task.id()).unwrap().unwrap();
    assert_eq!(current.status(), Some("in_progress"));
}

#[test]
fn the_loser_can_merge_by_re_reading_and_retrying() {
    // The whole point of returning `latest_version`: an agent can usually
    // resolve the conflict itself rather than clobbering or giving up.
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Ship the daemon", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;

    s.update(
        task.id(),
        1,
        json!({"status": "in_progress"}).as_object().unwrap(),
        &prov(),
    )
    .unwrap();

    let fresh = s.get(task.id()).unwrap().unwrap();
    let merged = s
        .update(
            task.id(),
            fresh.audit().version,
            json!({"priority": "p0"}).as_object().unwrap(),
            &prov(),
        )
        .unwrap();

    assert_eq!(merged.audit().version, 3);
    assert_eq!(
        merged.status(),
        Some("in_progress"),
        "the first write survives"
    );
}

#[test]
fn a_stale_archive_is_rejected_too() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Temp", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;

    s.update(
        task.id(),
        1,
        json!({"priority": "p0"}).as_object().unwrap(),
        &prov(),
    )
    .unwrap();

    let err = s.archive(task.id(), 1, &prov()).unwrap_err();
    assert!(err.is_conflict());
    assert!(!s.get(task.id()).unwrap().unwrap().audit().is_archived());
}

#[test]
fn updating_something_that_never_existed_is_not_a_conflict() {
    // "Missing" and "stale" need different responses from an agent, so they
    // must be distinguishable.
    let (mut s, _d) = store();
    let ghost = EntityId::generate(EntityType::Task);
    let err = s
        .update(
            &ghost,
            1,
            json!({"status": "done"}).as_object().unwrap(),
            &prov(),
        )
        .unwrap_err();
    assert!(!err.is_conflict());
    assert!(err.to_string().contains(ghost.as_str()));
}

// --- Event log (SPEC §3.4, REQ-6) ---------------------------------------

#[test]
fn every_mutation_writes_an_event() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p.clone(), "Ship it", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;
    s.update(
        task.id(),
        1,
        json!({"status": "in_progress"}).as_object().unwrap(),
        &prov(),
    )
    .unwrap();
    s.archive(task.id(), 2, &prov()).unwrap();

    let events = s.events(&Cursor::Beginning, Some(&p), 100).unwrap();
    let actions: Vec<Action> = events.items.iter().map(|e| e.action).collect();

    assert!(actions.contains(&Action::Created));
    assert!(
        actions.contains(&Action::StatusChanged),
        "a status change gets its own action, not a generic update: {actions:?}"
    );
    assert!(actions.contains(&Action::Archived));
}

#[test]
fn an_event_records_the_actor_and_session_of_the_write_that_caused_it() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Ship it", "A row this test needs in the store.").into(),
            &Provenance::anonymous(Actor::Human)
                .with_session("ses_human")
                .with_surface(Surface::Ui),
        )
        .unwrap()
        .entity;

    let events = s.events(&Cursor::Beginning, None, 100).unwrap();
    let created = events
        .items
        .iter()
        .find(|e| e.entity_id == *task.id() && e.action == Action::Created)
        .unwrap();

    assert_eq!(created.actor, Actor::Human);
    assert_eq!(created.session_id.as_deref(), Some("ses_human"));
    assert_eq!(created.surface, Some(Surface::Ui));
    assert_eq!(
        created.actor,
        task.audit().created_by,
        "SPEC §3.1: an entity's updated_by always equals the actor of the event that produced it"
    );
}

#[test]
fn a_field_change_records_both_sides() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Ship it", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;
    s.update(
        task.id(),
        1,
        json!({"status": "done"}).as_object().unwrap(),
        &prov(),
    )
    .unwrap();

    let events = s.events(&Cursor::Beginning, None, 100).unwrap();
    let change = events
        .items
        .iter()
        .find(|e| e.action == Action::StatusChanged)
        .unwrap();
    assert_eq!(change.field.as_deref(), Some("status"));
    assert_eq!(change.before, Some(json!("todo")));
    assert_eq!(change.after, Some(json!("done")));
}

#[test]
fn a_no_op_update_writes_no_event() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let task = s
        .create(
            Task::new(p, "Ship it", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;
    let before = s.events(&Cursor::Beginning, None, 1000).unwrap().total;

    s.update(
        task.id(),
        1,
        json!({"status": "todo"}).as_object().unwrap(),
        &prov(),
    )
    .unwrap();

    let after = s.events(&Cursor::Beginning, None, 1000).unwrap().total;
    assert_eq!(
        before, after,
        "re-sending the current value must not fill the feed"
    );
}

#[test]
fn events_can_be_read_from_a_cursor_without_gaps_or_repeats() {
    // The range scan SPEC §3.4 describes. It only works because ULIDs are
    // minted monotonically (DECISIONS B-9) — with plain random ULIDs, writes
    // inside one millisecond would sort arbitrarily and this would skip rows.
    let (mut s, _d) = store();
    let p = project(&mut s);
    for i in 0..30 {
        s.create(
            Task::new(
                p.clone(),
                format!("Task {i}"),
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    }

    let all = s.events(&Cursor::Beginning, None, 1000).unwrap();
    assert!(all.items.len() >= 31);

    // Walk the log in pages of seven, following the cursor.
    let mut collected: Vec<EventId> = Vec::new();
    let mut cursor = Cursor::Beginning;
    loop {
        let page = s.events(&cursor, None, 7).unwrap();
        if page.items.is_empty() {
            break;
        }
        let last = page.items[page.items.len() - 1].id.clone();
        collected.extend(page.items.into_iter().map(|e| e.id));
        cursor = Cursor::After(last);
    }

    let expected: Vec<EventId> = all.items.iter().map(|e| e.id.clone()).collect();
    assert_eq!(
        collected, expected,
        "cursor paging must visit every event exactly once, in order"
    );

    let mut deduped = collected.clone();
    deduped.dedup();
    assert_eq!(
        deduped.len(),
        collected.len(),
        "no event may be returned twice"
    );
}

#[test]
fn events_can_be_filtered_by_time() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    s.create(
        Task::new(p.clone(), "Early", "A row this test needs in the store.").into(),
        &prov(),
    )
    .unwrap();

    let boundary = chrono::Utc::now();
    std::thread::sleep(std::time::Duration::from_millis(5));
    s.create(
        Task::new(p, "Late", "A row this test needs in the store.").into(),
        &prov(),
    )
    .unwrap();

    let since = s.events(&Cursor::Since(boundary), None, 100).unwrap();
    assert_eq!(since.items.len(), 1);
    assert!(
        since.items[0].summary.contains("Late"),
        "{}",
        since.items[0].summary
    );
}

#[test]
fn events_are_scoped_by_project() {
    let (mut s, _d) = store();
    let p1 = project(&mut s);
    let p2 = s
        .create(Project::new("other", "Other").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    s.create(
        Task::new(p1.clone(), "One", "A row this test needs in the store.").into(),
        &prov(),
    )
    .unwrap();
    s.create(
        Task::new(p2.clone(), "Two", "A row this test needs in the store.").into(),
        &prov(),
    )
    .unwrap();

    let first = s.events(&Cursor::Beginning, Some(&p1), 100).unwrap();
    assert!(
        first
            .items
            .iter()
            .all(|e| e.project_id.as_ref() == Some(&p1)),
        "a project filter must not leak other projects"
    );
    assert_eq!(
        first.items.len(),
        2,
        "the project's own creation, plus the task"
    );
}

#[test]
fn a_link_writes_an_event_naming_the_stored_direction() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    let a = s
        .create(
            Task::new(p.clone(), "A", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;
    let b = s
        .create(
            Task::new(p, "B", "A row this test needs in the store.").into(),
            &prov(),
        )
        .unwrap()
        .entity;

    s.link(
        NewLink::new(a.id().clone(), Relation::DependsOn, b.id().clone()),
        &prov(),
    )
    .unwrap();

    let events = s.events(&Cursor::Beginning, None, 100).unwrap();
    let linked = events
        .items
        .iter()
        .find(|e| e.action == Action::Linked)
        .unwrap();
    assert!(
        linked.summary.contains("stored as"),
        "a normalised depends_on must say what was actually stored, or the next \
         reader will think the endpoints are backwards: {}",
        linked.summary
    );
}

#[test]
fn an_event_page_reports_its_total() {
    let (mut s, _d) = store();
    let p = project(&mut s);
    for i in 0..10 {
        s.create(
            Task::new(
                p.clone(),
                format!("T{i}"),
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    }
    let page = s.events(&Cursor::Beginning, None, 3).unwrap();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.total, 11);
    assert!(page.truncated);
}

// --- Concurrency (Phase 1's exit criterion) ------------------------------
//
// The placeholder that lived here through Phase 0 has been replaced by the
// real thing, in `keel-daemon/tests/concurrency.rs`. It could not stay in this
// crate: a `DuckStore` is one connection and deliberately not `Sync`, because
// D-5 makes the daemon the single write path. Driving two stores at one
// directory would have tested DuckDB's file locking rather than the claim.
//
// Sixteen concurrent sessions through the daemon now assert zero duplicates,
// zero lost updates, a gapless event log, and one edge from concurrent
// identical links.
