//! Archiving, and what it must not break.
//!
//! Written while chasing a live failure: archiving a project left the running
//! daemon returning storage errors for every project list and task lookup until
//! it was restarted. These assert the *store* is fine — which it is, and that is
//! what localised the bug to the daemon's process state rather than the data.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use specline_core::{
    Actor, EntityQuery, EntityStore, EntityType, Project, Provenance, Store, Task,
};

#[test]
fn listing_projects_still_works_after_one_is_archived() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
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
        .create(
            Task::new(
                drop.clone(),
                "Left behind",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
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
    // The p0, which happened under the engine this store replaced: an UPDATE
    // on a store whose index disagreed with its table raised a fatal error,
    // poisoning the connection so every later query failed with whatever
    // operation happened to be running — "count matching rows", "run a question
    // lookup". Reads on a fresh process worked, `fsck` reported clean, and both
    // were true, which is why it took an evening.
    //
    // The cause was SIGKILL mid-write, because SIGTERM could not stop a daemon
    // holding an open SSE stream. The daemon now checkpoints on shutdown; this
    // asserts the store survives that cycle and still takes a write.
    let dir = tempfile::tempdir().unwrap();
    let prov = Provenance::anonymous(Actor::Human);

    let id = {
        let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
        let id = store
            .create(Project::new("p", "P").into(), &prov)
            .unwrap()
            .entity
            .id()
            .clone();
        store
            .create(
                Task::new(
                    id.clone(),
                    "Something to update",
                    "A row this test needs in the store.",
                )
                .into(),
                &prov,
            )
            .unwrap();
        store.checkpoint().unwrap();
        id
    };

    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
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

#[test]
fn a_task_claimed_and_left_is_reported_as_stale() {
    // `in_progress` had never been used once across 66 tasks, so the board's
    // middle column was always empty. Asking sessions to claim a task before
    // starting fills it — and creates the opposite failure: a claim left by a
    // session that ended hours ago, still reading as active work.
    //
    // A stale claim is worse than an empty column. Empty says "nothing is
    // tracked here"; stale says "this is happening right now" and is wrong.
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let prov = Provenance::anonymous(Actor::Human);
    let project = store
        .create(Project::new("p", "P").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    let task = store
        .create(
            Task::new(
                project,
                "Claimed and abandoned",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("in_progress"));
    store.update(&task, 1, &changes, &prov).unwrap();

    // Freshly claimed: not stale, and must not be reported. Nagging about work
    // started a minute ago is how a check gets ignored.
    let report = specline_core::fsck::check(&store).unwrap();
    assert!(
        !report
            .findings
            .iter()
            .any(|f| f.check == "stale_in_progress"),
        "a claim made just now is not stale"
    );

    // Backdate past the threshold, which is the only part a test can force.
    //
    // The timestamp is computed here rather than in SQL. Timestamps are TEXT
    // in this store and the comparison is lexicographic, so date arithmetic
    // belongs on the Rust side where the format is guaranteed to match what
    // was written.
    let five_days_ago = (chrono::Utc::now() - chrono::Duration::days(5))
        .naive_utc()
        .to_string();
    store
        .connection()
        .execute(
            "UPDATE tasks SET updated_at = ? WHERE status = 'in_progress'",
            [five_days_ago],
        )
        .unwrap();

    let report = specline_core::fsck::check(&store).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.check == "stale_in_progress")
        .expect("a five-day-old claim is reported");
    assert_eq!(finding.count, 1);
    assert_eq!(
        finding.severity,
        specline_core::Severity::Warning,
        "a warning, not an error — it is untidy, not broken"
    );
}
