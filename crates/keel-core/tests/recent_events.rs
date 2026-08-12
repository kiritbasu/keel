//! "Recently" has to mean recently.
//!
//! Four readers wanted the newest events and all four asked the *feed* for a
//! generous number of rows and reversed the answer in Rust. The feed goes
//! oldest-first on purpose — a cursor-following caller must see every event
//! exactly once — so a limit there keeps the beginning. That is right until the
//! log passes the limit, and then it is silently wrong in the worst available
//! way: it keeps the oldest rows and presents them as the latest news.
//!
//! One of the four was already broken in the live store when this was written.
//! The 409-conflict payload read the oldest 500 events out of 804, so an agent
//! trying to resolve a stale write was shown history from before the conflict
//! and nothing near it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::store::EventScope;
use keel_core::{
    Actor, Cursor, EntityId, EntityStore, NewEvent, Project, Provenance, Store, Task, changes,
    render_status,
};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_recent")
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

/// Write `n` events against one row, each numbered so the newest is nameable.
fn noisy_history(store: &mut Store, project: &EntityId, n: usize) -> EntityId {
    let task = store
        .create(
            Task::new(project.clone(), "A noisy task", "It has a long history.").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    for i in 0..n {
        store
            .append_event(
                NewEvent::new(
                    task.clone(),
                    keel_core::Action::Updated,
                    format!("change number {i}"),
                )
                .in_project(Some(project.clone())),
                &prov(),
            )
            .unwrap();
    }
    task
}

#[test]
fn recent_events_returns_the_newest_and_says_how_many_there_were() {
    let (_d, mut store, project) = fixture();
    noisy_history(&mut store, &project, 50);

    let page = store
        .recent_events(EventScope::Project(&project), 5)
        .unwrap();

    assert_eq!(page.items.len(), 5);
    assert_eq!(
        page.items[0].summary, "change number 49",
        "the first row should be the most recent event, not the oldest"
    );
    assert!(page.truncated, "a cut page must say it was cut");
    assert!(page.total > 50, "the total counts everything in scope");
}

/// The bug, at its own boundary: the cap plus one.
#[test]
fn the_newest_event_survives_a_cap_smaller_than_the_log() {
    let (_d, mut store, project) = fixture();
    noisy_history(&mut store, &project, 20);

    let capped = store
        .recent_events(EventScope::Project(&project), 3)
        .unwrap();
    assert!(
        capped.items.iter().any(|e| e.summary == "change number 19"),
        "the newest event fell off a cap that is supposed to keep it: {:?}",
        capped.items.iter().map(|e| &e.summary).collect::<Vec<_>>()
    );

    // And the feed still behaves the opposite way, deliberately.
    let feed = store.events(&Cursor::Beginning, Some(&project), 3).unwrap();
    assert!(
        !feed.items.iter().any(|e| e.summary == "change number 19"),
        "the cursor feed must still return the oldest first, or paging skips rows"
    );
}

#[test]
fn scoping_to_one_row_returns_only_that_rows_history() {
    let (_d, mut store, project) = fixture();
    let noisy = noisy_history(&mut store, &project, 10);
    let quiet = store
        .create(
            Task::new(project.clone(), "A quiet task", "Nothing happens to it.").into(),
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let page = store.recent_events(EventScope::Entity(&quiet), 20).unwrap();
    assert!(
        page.items.iter().all(|e| e.entity_id == quiet),
        "an entity-scoped read returned another row's events"
    );
    assert_eq!(page.total, 1, "the quiet task has only its creation event");

    let noisy_page = store.recent_events(EventScope::Entity(&noisy), 20).unwrap();
    assert_eq!(noisy_page.items[0].summary, "change number 9");
}

/// The generated changelog is derived from the log, so it inherits whatever the
/// read gets wrong.
#[test]
fn the_rendered_changelog_shows_the_newest_change_not_the_oldest() {
    let (_d, mut store, project) = fixture();
    noisy_history(&mut store, &project, 60);

    let markdown = render_status::render(&store, &project).unwrap();
    assert!(
        markdown.contains("change number 59"),
        "the changelog is missing the most recent change"
    );
    assert!(
        !markdown.contains("| change number 0 |"),
        "the changelog is showing the oldest changes as the latest ones"
    );
}

/// "What changed", grouped by session, reads the same log.
#[test]
fn the_change_log_groups_the_newest_changes() {
    let (_d, mut store, project) = fixture();
    noisy_history(&mut store, &project, 40);

    let log = changes::by_session(
        &store,
        &changes::ChangeQuery {
            project_id: Some(project.clone()),
            limit: 5,
            ..Default::default()
        },
    )
    .unwrap();

    let summaries: Vec<&str> = log
        .sessions
        .iter()
        .flat_map(|s| s.changes.iter().map(|c| c.summary.as_str()))
        .collect();
    assert!(
        summaries.contains(&"change number 39"),
        "the newest change is missing from the change log: {summaries:?}"
    );
    assert!(log.truncated, "a cut log must say it was cut");
}
