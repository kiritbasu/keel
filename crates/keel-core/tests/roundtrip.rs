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
    let mut milestone = Milestone::new(project_id.clone(), "Phase 0 — Spine");
    milestone.kind = MilestoneKind::Release;
    milestone.summary = Some("The storage spine".into());
    milestone.status = MilestoneStatus::Active;
    milestone.target_date = NaiveDate::from_ymd_opt(2026, 9, 30);
    milestone.version_string = Some("0.1.0".into());
    milestone.sort_order = Some(1);

    let mut task = Task::new(project_id.clone(), "Wire up the DuckDB schema");
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
        .create(Task::new(project_id.clone(), "Temporary").into(), &claude())
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
                Task::new(project_id.clone(), format!("Task {i}")).into(),
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
