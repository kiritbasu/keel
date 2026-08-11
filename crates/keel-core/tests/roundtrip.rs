//! Round-trip every entity type through real storage: create, read, update,
//! archive.
//!
//! Phase 0's exit criteria name this first. Nothing here mocks the store —
//! these open a real DuckDB file with the Lance extension loaded, because the
//! bugs worth catching at this layer live in the mapping between Rust structs
//! and SQL columns, and a mock has no columns to get wrong.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use chrono::{NaiveDate, Utc};
use keel_core::*;
use serde_json::json;

/// A store in a fresh temporary directory, plus the directory's guard.
fn store() -> (DuckStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("create a temp dir");
    let store = DuckStore::open(dir.path()).expect("open the store");
    (store, dir)
}

fn claude() -> Provenance {
    Provenance::anonymous(Actor::Claude)
        .with_session("ses_test")
        .with_surface(Surface::Code)
}

/// Create a project and return its id.
fn project(store: &mut DuckStore) -> EntityId {
    let created = store
        .create(Project::new("keel", "Keel").into(), &claude())
        .expect("create the project");
    created.entity.id().clone()
}

/// One fully populated instance of every type, so the round-trip exercises
/// optional columns rather than only the required ones.
fn one_of_each(project_id: &EntityId, metric_id: &EntityId) -> Vec<Entity> {
    let mut milestone = Milestone::new(
        project_id.clone(),
        "Phase 0 — Spine",
        "Storage, schema, event log, graph, search, backup.",
    );
    milestone.kind = MilestoneKind::Release;
    milestone.summary = Some("The storage spine".into());
    milestone.status = MilestoneStatus::Active;
    milestone.target_date = NaiveDate::from_ymd_opt(2026, 9, 30);
    milestone.version_string = Some("0.1.0".into());
    milestone.sort_order = Some(1);

    let mut task = Task::new(
        project_id.clone(),
        "Wire up the DuckDB schema",
        "A row this test needs in the store.",
    );
    task.kind = TaskKind::Chore;
    task.body = Some("Forward-only migrations, tested.".into());
    task.priority = TaskPriority::P0;
    task.labels = vec!["storage".into(), "phase-0".into()];
    task.external_refs = vec!["https://github.com/kb/keel/pull/1".into()];

    let mut spec = Spec::new(project_id.clone(), "Storage specification");
    spec.kind = SpecKind::DesignDoc;
    spec.status = SpecStatus::Approved;
    spec.mirror_path = Some(".keel/specs/storage.md".into());

    let mut decision = Decision::new(project_id.clone(), "DuckDB and Lance, not SQLite");
    decision.status = DecisionStatus::Accepted;
    decision.decided_at = Some(Utc::now());

    let mut question = Question::new(project_id.clone(), "Where does the store live?");
    question.kind = QuestionKind::Risk;
    question.severity = Some(RiskSeverity::High);

    let mut term = Term::new(
        Some(project_id.clone()),
        "Digest",
        "The compact project summary returned by keel_context",
    );
    term.aliases = vec!["context digest".into()];

    let mut feedback = Feedback::new(project_id.clone(), "Onboarding felt slow");
    feedback.kind = FeedbackKind::Interview;
    feedback.source = Some("Customer A".into());
    feedback.contact = Some("a@example.com".into());
    feedback.sentiment = Some(Sentiment::Negative);
    feedback.occurred_at = Some(Utc::now());

    let mut design = Design::new(project_id.clone(), "Home screen");
    design.state = DesignState::Approved;
    design.figma_ref = Some("figma:node/123".into());

    let mut environment = Environment::new(project_id.clone(), "production");
    environment.url = Some("https://keel.local".into());
    environment.deployed_version = Some("0.1.0".into());
    environment.deployed_commit = Some("abc1234".into());
    environment.status = EnvironmentStatus::Healthy;
    environment.last_deployed_at = Some(Utc::now());

    let mut metric = Metric::new(project_id.clone(), "Sessions where Claude writes to Keel");
    metric.id = metric_id.clone();
    metric.unit = Some("%".into());
    metric.target_value = Some(80.0);
    metric.direction = MetricDirection::Up;

    let mut observation =
        MetricObservation::new(metric_id.clone(), project_id.clone(), 62.5, Utc::now());
    observation.note = Some("Before the skill landed".into());

    let mut artifact = Artifact::new(project_id.clone(), "Competitor teardown");
    artifact.kind = ArtifactKind::Link;
    artifact.url = Some("https://example.com/teardown".into());

    vec![
        milestone.into(),
        task.into(),
        spec.into(),
        decision.into(),
        question.into(),
        term.into(),
        feedback.into(),
        design.into(),
        environment.into(),
        metric.into(),
        observation.into(),
        artifact.into(),
    ]
}

#[test]
fn a_fresh_store_opens_and_migrates() {
    let (store, _dir) = store();
    let applied: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM _keel_migrations", [], |r| r.get(0))
        .expect("read the migration table");
    assert!(
        applied >= 3,
        "expected at least three migrations, got {applied}"
    );

    // The Lance datasets must be queryable, not merely created.
    let docs: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM lancedb.documents", [], |r| r.get(0))
        .expect("query the attached Lance documents dataset");
    assert_eq!(docs, 0);
}

#[test]
fn opening_an_existing_store_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut s = DuckStore::open(dir.path()).unwrap();
        project(&mut s);
    }
    let s = DuckStore::open(dir.path()).expect("re-open");
    let applied: i64 = s
        .connection()
        .query_row("SELECT count(*) FROM _keel_migrations", [], |r| r.get(0))
        .unwrap();
    // Counted against the migration list rather than a literal: the point is
    // that a re-open applies each migration once, not that there are three of
    // them, and hard-coding the number makes every new migration break a test
    // that is not about it.
    assert_eq!(
        applied,
        keel_core::store::schema::migrations().len() as i64,
        "migrations must not re-run"
    );
    let projects: i64 = s
        .connection()
        .query_row("SELECT count(*) FROM projects", [], |r| r.get(0))
        .unwrap();
    assert_eq!(projects, 1, "data must survive a re-open");
}

#[test]
fn every_entity_type_round_trips() {
    let (mut store, _dir) = store();
    let project_id = project(&mut store);
    let metric_id = EntityId::generate(EntityType::Metric);

    let mut seen = std::collections::HashSet::new();
    seen.insert(EntityType::Project);

    for entity in one_of_each(&project_id, &metric_id) {
        let entity_type = entity.entity_type();
        seen.insert(entity_type);

        // --- create ---
        let created = store
            .create(entity.clone(), &claude())
            .unwrap_or_else(|e| panic!("create {entity_type}: {e}"));
        assert!(created.created, "{entity_type} should be newly created");
        let id = created.entity.id().clone();

        // --- read ---
        let fetched = store
            .get(&id)
            .unwrap_or_else(|e| panic!("get {entity_type}: {e}"))
            .unwrap_or_else(|| panic!("{entity_type} vanished after create"));

        assert_eq!(
            fetched, created.entity,
            "{entity_type} did not survive the storage round trip unchanged"
        );
        assert_eq!(fetched.audit().version, 1);
        assert_eq!(fetched.audit().created_by, Actor::Claude);
        assert_eq!(fetched.audit().session_id.as_deref(), Some("ses_test"));
        assert_eq!(fetched.audit().surface, Some(Surface::Code));
        assert!(!fetched.audit().is_archived());

        // --- update ---
        let field = match entity_type {
            EntityType::Term => ("definition", json!("a new definition")),
            EntityType::Feedback => ("triaged", json!(true)),
            EntityType::Design => ("state", json!("built")),
            EntityType::Metric => ("target_value", json!(90.0)),
            EntityType::MetricObservation => ("note", json!("after")),
            EntityType::Artifact => ("name", json!("Renamed artifact")),
            EntityType::Environment => ("status", json!("degraded")),
            EntityType::Milestone => ("status", json!("shipped")),
            EntityType::Task => ("status", json!("in_progress")),
            EntityType::Spec => ("status", json!("superseded")),
            // An accepted decision is immutable except for its status.
            EntityType::Decision => ("status", json!("superseded")),
            EntityType::Question => ("status", json!("answered")),
            EntityType::Project => ("name", json!("Keel")),
        };
        let changes = json!({ field.0: field.1 });
        let updated = store
            .update(
                &id,
                1,
                changes.as_object().unwrap(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap_or_else(|e| panic!("update {entity_type}.{}: {e}", field.0));

        assert_eq!(
            updated.audit().version,
            2,
            "{entity_type} version must bump"
        );
        assert_eq!(updated.audit().updated_by, Actor::Human);
        assert_eq!(
            updated.audit().created_by,
            Actor::Claude,
            "{entity_type} original author must not change"
        );

        let refetched = store.get(&id).unwrap().unwrap();
        assert_eq!(refetched, updated, "{entity_type} update did not persist");

        // --- archive ---
        let archived = store
            .archive(&id, 2, &claude())
            .unwrap_or_else(|e| panic!("archive {entity_type}: {e}"));
        assert!(archived.audit().is_archived(), "{entity_type} not archived");

        // Soft delete: the row is still there.
        let after = store.get(&id).unwrap().unwrap();
        assert!(after.audit().is_archived());
    }

    assert_eq!(
        seen.len(),
        13,
        "all thirteen artifact types must round-trip; saw {seen:?}"
    );
}

#[test]
fn archived_entities_are_excluded_from_lists_but_not_deleted() {
    let (mut store, _dir) = store();
    let project_id = project(&mut store);

    let task = store
        .create(
            Task::new(
                project_id.clone(),
                "Temporary",
                "A row this test needs in the store.",
            )
            .into(),
            &claude(),
        )
        .unwrap()
        .entity;
    store.archive(task.id(), 1, &claude()).unwrap();

    let visible = store
        .list(&EntityQuery::in_project(project_id.clone()).of_type(EntityType::Task))
        .unwrap();
    assert_eq!(
        visible.items.len(),
        0,
        "archived tasks must not appear by default"
    );

    let including = store
        .list(&EntityQuery {
            include_archived: true,
            ..EntityQuery::in_project(project_id).of_type(EntityType::Task)
        })
        .unwrap();
    assert_eq!(including.items.len(), 1, "the row must still exist (D-9)");

    let count: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "nothing is ever DELETEd");
}

#[test]
fn a_global_term_and_a_project_term_of_the_same_name_coexist() {
    // Q-4: per-project terms override globals, so both rows must be storable.
    let (mut store, _dir) = store();
    let project_id = project(&mut store);

    let global = store
        .create(
            Term::new(None, "Digest", "generic meaning").into(),
            &claude(),
        )
        .unwrap();
    assert!(global.created);

    let scoped = store
        .create(
            Term::new(Some(project_id.clone()), "Digest", "keel_context output").into(),
            &claude(),
        )
        .unwrap();
    assert!(scoped.created);

    // A project's glossary includes globals as well as its own overrides.
    let listed = store
        .list(&EntityQuery::in_project(project_id).of_type(EntityType::Term))
        .unwrap();
    assert_eq!(listed.items.len(), 2);
}

#[test]
fn a_second_global_term_of_the_same_name_is_refused() {
    let (mut store, _dir) = store();
    // The same term twice hits the idempotency key first and returns the
    // original — which is the desired behaviour, not an error.
    let first = store
        .create(Term::new(None, "Digest", "one").into(), &claude())
        .unwrap();
    let second = store
        .create(Term::new(None, "Digest", "two").into(), &claude())
        .unwrap();
    assert!(first.created);
    assert!(
        !second.created,
        "the duplicate must not create a second row"
    );
    assert_eq!(first.entity.id(), second.entity.id());
}

#[test]
fn a_page_that_is_cut_reports_the_total() {
    // Hard constraint 4: no silent truncation.
    let (mut store, _dir) = store();
    let project_id = project(&mut store);
    for i in 0..12 {
        store
            .create(
                Task::new(
                    project_id.clone(),
                    format!("Task {i}"),
                    "A row this test needs in the store.",
                )
                .into(),
                &claude(),
            )
            .unwrap();
    }

    let page = store
        .list(
            &EntityQuery::in_project(project_id)
                .of_type(EntityType::Task)
                .limited(5),
        )
        .unwrap();
    assert_eq!(page.items.len(), 5);
    assert_eq!(page.total, 12, "the caller must be told how many exist");
    assert!(page.truncated);
}

/// An accepted decision can be corrected, and the correction is recorded.
///
/// This used to be refused (`DecisionImmutable`), and the refusal was on the
/// wrong door: it blocked a title change while `write_revision` replaced the
/// body — the actual reasoning — unchecked. Seven titles truncated at import
/// could not be fixed; twenty-five bodies were rewritten without objection.
///
/// The test that guarded the old rule asserted only that the *error* named the
/// remedy — never that the body it was protecting was actually protected. It
/// replaced by this one, which asserts the behaviour KB chose: permit the edit,
/// and rely on the revision chain to make it visible (TQ-27, B-43).
#[test]
fn an_accepted_decision_can_be_corrected_and_the_change_is_recorded() {
    let (mut store, _dir) = store();
    let prov = claude();
    let project_id = project(&mut store);

    let mut decision = Decision::new(project_id.clone(), "Surface carries five values: chat \\");
    decision.status = DecisionStatus::Accepted;
    let created = store.create(decision.into(), &prov).unwrap().entity;

    let mut changes = serde_json::Map::new();
    changes.insert(
        "title".to_owned(),
        serde_json::json!("Surface carries five values: chat, cowork, code, ui, cli"),
    );
    let updated = store
        .update(created.id(), created.audit().version, &changes, &prov)
        .expect("a truncated title must be correctable");

    assert_eq!(
        updated.label(),
        "Surface carries five values: chat, cowork, code, ui, cli"
    );

    // The guard that replaces the refusal: the change is attributed, not silent.
    let events = store.events_for(created.id(), 20).unwrap();
    assert!(
        events
            .items
            .iter()
            .any(|e| e.field.as_deref() == Some("title")),
        "the correction must leave an event: {:?}",
        events.items
    );

    // Optimistic concurrency still applies — removing the immutability guard
    // must not have removed the stale-write guard with it.
    let stale = store.update(created.id(), created.audit().version, &changes, &prov);
    assert!(stale.is_err(), "a stale version must still be rejected");
}

/// One row with no readable number must not make the whole table unreadable.
///
/// Reported from another project on 2026-08-10: `keel_create` with
/// `type: "decision"` failed reproducibly with
/// `read column number of decisions: Invalid column type Null`.
///
/// The cause was a window every schema change opens. Migration 10 added
/// `number` and backfilled everything that existed; 84 seconds later a daemon
/// that had the column but not the field inserted a decision, which got a NULL.
/// Reading that as a hard error meant one row took down *every* read of that
/// type in that project — including the idempotency lookup, which is why a
/// create failed rather than merely a list.
///
/// The fix is proportion: an unnumbered row costs its own label, nothing more.
#[test]
fn a_row_with_no_number_is_still_readable_and_gets_repaired() {
    let (mut store, _dir) = store();
    let project_id = project(&mut store);

    let first = store
        .create(
            Decision::new(project_id.clone(), "Use DuckDB").into(),
            &claude(),
        )
        .unwrap()
        .entity;
    store
        .create(
            Decision::new(project_id.clone(), "Bundle it").into(),
            &claude(),
        )
        .unwrap();

    // Exactly what the old daemon left behind.
    store
        .connection()
        .execute(
            "UPDATE decisions SET number = NULL WHERE id = ?",
            duckdb::params![first.id().as_str()],
        )
        .expect("blank the number");

    // Reading it must work, and must not be mistaken for a real identifier.
    let read = store
        .get(first.id())
        .expect("one unnumbered row must not make the type unreadable")
        .expect("the row is still there");
    match &read {
        Entity::Decision(d) => assert_eq!(d.number, 0, "zero means unassigned"),
        other => panic!("expected a decision, got {other:?}"),
    }

    // Listing the project must not fail either — this is what the idempotency
    // lookup does, and it failing is what turned a bad row into a failed create.
    let page = store
        .list(&EntityQuery::in_project(project_id.clone()).of_type(EntityType::Decision))
        .expect("listing must survive an unnumbered row");
    assert_eq!(page.items.len(), 2);

    // Creating alongside it must work. This is the reported symptom.
    let third = store
        .create(
            Decision::new(project_id.clone(), "Vendor Lance").into(),
            &claude(),
        )
        .expect("a create must not be blocked by another row's missing number");
    assert!(third.created);

    // And an update repairs the blank rather than writing the zero back, which
    // would collide with the unique index the moment a second row did it.
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("accepted"));
    let repaired = store
        .update(read.id(), read.audit().version, &changes, &claude())
        .expect("update the unnumbered row");
    match &repaired {
        Entity::Decision(d) => assert!(d.number > 0, "the write path assigns one: {}", d.number),
        other => panic!("expected a decision, got {other:?}"),
    }
}

/// A binary older than the store refuses to open it.
///
/// The failure this prevents is the one reported on 2026-08-10. A migration
/// added `decisions.number`; a daemon built before it kept running, found every
/// migration it knew about already applied, concluded it was up to date, and
/// inserted rows leaving the new column NULL. Nothing said the binary was
/// behind, and the corruption surfaced later as an unrelated-looking read error
/// in a different project.
///
/// Simulated the only way that is honest without shipping a second binary: a
/// migration id from the future in the bookkeeping table, which is exactly what
/// an old binary sees when it opens a store a newer one has migrated.
#[test]
fn a_binary_older_than_the_store_refuses_to_open_it() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let store = DuckStore::open(dir.path()).expect("open a fresh store");
        store
            .connection()
            .execute(
                "INSERT INTO _keel_migrations (id, name, applied_at) VALUES (?, ?, ?)",
                duckdb::params![9_999, "from_the_future", chrono::Utc::now().naive_utc()],
            )
            .expect("record a migration this binary does not ship");
    }

    let err = DuckStore::open(dir.path())
        .expect_err("a store newer than the binary must not open")
        .to_string();

    // The message has to be actionable, because whoever hits this is holding a
    // binary and has no idea it is the old one.
    assert!(err.contains("9999"), "name the store's schema: {err}");
    assert!(
        err.contains("install.sh") || err.contains("Rebuild"),
        "say how to fix it: {err}"
    );
    assert!(
        err.contains("--home"),
        "and name the deliberate escape: {err}"
    );
}

/// A store at the binary's own schema still opens.
///
/// The guard must refuse *newer*, not *different*. A version check that also
/// blocks the equal case is a check nobody can ship past.
#[test]
fn a_store_at_the_current_schema_opens_normally() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let mut store = DuckStore::open(dir.path()).expect("first open");
        store
            .create(Project::new("keel", "Keel").into(), &claude())
            .expect("write to it");
    }
    let store = DuckStore::open(dir.path()).expect("reopening at the same schema must work");
    let page = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .expect("read it back");
    assert_eq!(page.items.len(), 1);
}

/// A milestone cannot reach storage without a plain-English explainer.
///
/// The rule lives in `keel-core` rather than in the MCP layer so that the CLI,
/// the daemon and `keel import` cannot disagree about what is storable — two
/// surfaces with their own opinion of a valid row is how a rule becomes a
/// convention. These assert against the real store for that reason.
mod milestone_summary {
    use super::*;

    #[test]
    fn round_trips_to_storage_and_back() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        let created = store
            .create(
                Milestone::new(p, "Phase 8", "Make the everyday loop work.").into(),
                &claude(),
            )
            .expect("create a milestone with an explainer");

        let read = store.get(created.entity.id()).unwrap().unwrap();
        let Entity::Milestone(m) = read else {
            panic!("expected a milestone");
        };
        assert_eq!(m.summary.as_deref(), Some("Make the everyday loop work."));
    }

    // Failure case: the create path refuses.
    #[test]
    fn create_refuses_an_empty_explainer() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        let mut m = Milestone::new(p, "Phase 8", "placeholder");
        m.summary = None;

        let err = store.create(m.into(), &claude()).unwrap_err();
        assert!(err.to_string().contains("summary"), "{err}");
    }

    // Failure case: and so does the update path. Enforcing on create alone
    // would mean the requirement holds for the first write and a later call
    // can blank the explainer back out — a rule with a door in it.
    #[test]
    fn update_cannot_blank_an_explainer_that_was_required_on_create() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        let created = store
            .create(
                Milestone::new(p, "Phase 8", "Make the everyday loop work.").into(),
                &claude(),
            )
            .unwrap();

        let mut changes = serde_json::Map::new();
        changes.insert("summary".to_owned(), json!(""));
        let err = store
            .update(
                created.entity.id(),
                created.entity.audit().version,
                &changes,
                &claude(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("summary"), "{err}");

        // And the stored row is untouched, rather than half-written.
        let read = store.get(created.entity.id()).unwrap().unwrap();
        let Entity::Milestone(m) = read else {
            panic!("expected a milestone");
        };
        assert_eq!(m.summary.as_deref(), Some("Make the everyday loop work."));
    }
}

/// A task carries a summary, and cannot be created without one.
///
/// TQ-34: required on the create path, nullable in storage. The asymmetry is
/// the point — ninety-four rows predate the rule and a NOT NULL column would
/// make them unreadable rather than merely unlabelled.
mod task_summary {
    use super::*;

    fn task_with(project: EntityId, title: &str, summary: &str) -> Task {
        let mut t = Task::new(project, title, "A row this test needs in the store.");
        t.summary = Some(summary.to_owned());
        t
    }

    #[test]
    fn round_trips_to_storage_and_back() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        let created = store
            .create(
                task_with(
                    p,
                    "Show the milestone on every row",
                    "The board never says which phase a task is in, so you have to open each \
                     one. Done when every row shows it.",
                )
                .into(),
                &claude(),
            )
            .expect("create a task with a summary");

        let Entity::Task(read) = store.get(created.entity.id()).unwrap().unwrap() else {
            panic!("expected a task");
        };
        assert!(read.summary.unwrap().starts_with("The board never says"));
    }

    // Failure case: the requirement itself.
    #[test]
    fn create_refuses_a_task_with_no_summary() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        // Cleared explicitly rather than never set: `Task::new` takes the
        // summary positionally now, so "a task with no summary" is a state you
        // have to construct on purpose. That is the constructor doing its job —
        // this is the only place in the codebase that wants the bad state.
        let mut bare = Task::new(p, "Do the thing", "placeholder");
        bare.summary = None;
        let err = store.create(bare.into(), &claude()).unwrap_err();
        let message = err.to_string();
        assert!(message.contains("summary"), "{message}");
        // A model reading only the "Expected" half must be able to retry.
        assert!(message.contains("done looks like"), "{message}");
    }

    // Failure case: a summary that adds nothing.
    #[test]
    fn create_refuses_a_summary_that_only_restates_the_title() {
        let (mut store, _dir) = store();
        let p = project(&mut store);
        let err = store
            .create(
                task_with(p, "Fix the board filter", "Board filter fix").into(),
                &claude(),
            )
            .unwrap_err();
        assert!(err.to_string().contains("reorders the title"), "{err}");
    }

    #[test]
    fn a_summary_that_adds_anything_is_accepted() {
        // Containment, not similarity: added words pass. An overlap score would
        // have refused this, and it is a perfectly good summary.
        let (mut store, _dir) = store();
        let p = project(&mut store);
        store
            .create(
                task_with(
                    p,
                    "Fix the board filter",
                    "Fix the board filter so it survives a reload, which it does not today.",
                )
                .into(),
                &claude(),
            )
            .expect("added detail is not a restatement");
    }

    /// The exemption, and the reason validation runs on create and not update.
    ///
    /// Ninety-four rows predate the rule. If the check ran on every write, none
    /// of them could ever be touched again — moving one to `done` would be
    /// refused for a summary nobody was being asked to write. Freezing a third
    /// of the tracker is a worse failure than the hole this leaves.
    #[test]
    fn a_task_that_predates_the_rule_can_still_be_updated() {
        let (mut store, _dir) = store();
        let p = project(&mut store);

        // Stand in for a row written before the column existed.
        let created = store
            .create(
                task_with(p, "An older task", "Written before the rule existed.").into(),
                &claude(),
            )
            .unwrap();
        let mut blank = serde_json::Map::new();
        blank.insert("summary".to_owned(), json!(null));
        let cleared = store
            .update(
                created.entity.id(),
                created.entity.audit().version,
                &blank,
                &claude(),
            )
            .expect("clearing is possible, which is what makes the old rows reachable");

        let mut changes = serde_json::Map::new();
        changes.insert("status".to_owned(), json!("done"));
        changes.insert("close_reason".to_owned(), json!("done"));
        changes.insert(
            "close_message".to_owned(),
            json!("Finished, and the row predates the summary requirement."),
        );
        changes.insert("evidence".to_owned(), json!(["commit:deadbeef"]));
        store
            .update(cleared.id(), cleared.audit().version, &changes, &claude())
            .expect("a summary-less task must still be closable");
    }
}
