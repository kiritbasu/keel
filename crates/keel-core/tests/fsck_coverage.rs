//! Every `fsck` check has a corruption that trips it.
//!
//! `fsck` is the thing that notices when the store has quietly gone wrong. A
//! check that has never been seen to fire is a check nobody knows works — and
//! the failure mode is the worst available here, because a check that silently
//! never fires reads exactly like a store with nothing wrong with it.
//!
//! So: one test per check, each corrupting the store in the specific way that
//! check exists to catch, plus a meta-test that fails when `fsck::CHECKS` grows
//! an entry this file does not cover. That last one is the part that lasts —
//! the same shape as `graph_direction.rs`, which is repetitive for the same
//! reason.
//!
//! The corruption is raw SQL on purpose. Every one of these states is
//! unreachable through `keel-core`, which is the point: the API refuses them,
//! and `fsck` is the audit for the paths that did not come through the API — a
//! crash between two writes, a restore from a half-good backup, a bug in a
//! future version.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::*;

/// Which checks this file corrupts the store to trip.
///
/// A name here without a test below is a lie, and a name in `fsck::CHECKS`
/// missing from here fails `every_check_has_a_test`.
const COVERED: &[&str] = &[
    "dangling_link_source",
    "dangling_link_target",
    "depends_on_stored",
    "link_type_mismatch",
    "orphan_document",
    "multiple_current_revisions",
    "document_without_passages",
    "stale_passage",
    "passages_from_mixed_models",
    "orphan_task",
    "stale_in_progress",
    "milestone_stores_a_derived_state",
    "shipped_without_a_date",
    "duplicate_task_number",
    "task_without_number",
    "project_without_key",
    "task_parent_cycle",
    "task_parent_dangling",
    "unresolved_id_reference",
    "event_without_actor",
    "event_without_session",
    "row_without_creation_event",
    "live_link_to_archived",
    "orphan_blob",
];

/// The one check no corruption here can trip, and why.
///
/// `page_integrity` runs SQLite's own `quick_check`, so tripping it means
/// writing damaged bytes into the middle of a database file. That is a test
/// about SQLite rather than about Keel, and one whose failure mode on a future
/// SQLite is a corrupted temp file rather than a useful signal. `keel doctor`
/// exercises the reporting path against a sound file, which is the part this
/// codebase owns.
const WAIVED: &[&str] = &["page_integrity"];

struct Corrupt {
    store: Store,
    project: EntityId,
    _dir: tempfile::TempDir,
}

impl Corrupt {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
        let project = store
            .create(
                Project::new("fsck", "Fsck").into(),
                &Provenance::anonymous(Actor::Claude),
            )
            .unwrap()
            .entity
            .id()
            .clone();
        Corrupt {
            store,
            project,
            _dir: dir,
        }
    }

    fn task(&mut self, title: &str) -> EntityId {
        self.store
            .create(
                Task::new(self.project.clone(), title, "A row for an fsck test.").into(),
                &Provenance::anonymous(Actor::Claude),
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn spec(&mut self, title: &str) -> EntityId {
        self.store
            .create(
                Spec::new(self.project.clone(), title).into(),
                &Provenance::anonymous(Actor::Claude),
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn sql(&self, statement: &str) {
        self.store.connection().execute_batch(statement).unwrap();
    }

    /// Assert `fsck` reports this check, and return the finding.
    fn trips(&self, check: &str) -> fsck::Finding {
        let report = fsck::check(&self.store).unwrap();
        report
            .findings
            .iter()
            .find(|f| f.check == check)
            .unwrap_or_else(|| {
                panic!(
                    "`{check}` did not fire against a store corrupted specifically for it. \
                     Findings: {:?}",
                    report.findings.iter().map(|f| &f.check).collect::<Vec<_>>()
                )
            })
            .clone()
    }

    /// Assert `fsck` does *not* report this check. The half that catches a
    /// check which fires against everything.
    fn quiet(&self, check: &str) {
        let report = fsck::check(&self.store).unwrap();
        assert!(
            !report.findings.iter().any(|f| f.check == check),
            "`{check}` fired against a store with nothing wrong of that kind"
        );
    }
}

/// The guard. A check added to `fsck` without a corruption test fails here.
#[test]
fn every_check_has_a_test() {
    let missing: Vec<&str> = fsck::CHECKS
        .iter()
        .copied()
        .filter(|c| !COVERED.contains(c) && !WAIVED.contains(c))
        .collect();
    assert!(
        missing.is_empty(),
        "these fsck checks have no corruption test that trips them: {missing:?}\n\
         Add one to this file, or add the name to WAIVED with a reason. A check nobody \
         has seen fire is a check nobody knows works, and one that silently never fires \
         reads exactly like a clean store."
    );

    // And the other direction: a name here that fsck no longer emits is a test
    // asserting on something that cannot happen.
    let stale: Vec<&str> = COVERED
        .iter()
        .chain(WAIVED)
        .copied()
        .filter(|c| !fsck::CHECKS.contains(c))
        .collect();
    assert!(
        stale.is_empty(),
        "these names are no longer fsck checks: {stale:?}"
    );
}

/// A store with nothing wrong reports nothing wrong. Without this, every test
/// below could pass against a check that fires unconditionally.
#[test]
fn a_healthy_store_trips_nothing() {
    let mut c = Corrupt::new();
    c.task("An ordinary task");
    c.spec("An ordinary spec");

    let report = fsck::check(&c.store).unwrap();
    let errors: Vec<&str> = report.errors().map(|f| f.check.as_str()).collect();
    assert!(
        errors.is_empty(),
        "a fresh store should be clean: {errors:?}"
    );
}

#[test]
fn dangling_link_source_and_target() {
    let mut c = Corrupt::new();
    let task = c.task("Real");
    let gone = EntityId::generate(EntityType::Spec);

    c.sql(&format!(
        "INSERT INTO links (id, project_id, from_id, from_type, to_id, to_type, rel, anchor,
             created_at, updated_at, version, created_by, updated_by)
         VALUES ('lnk_01H8XK4RPVBQ2N7DZM9C3FGTW1', '{p}', '{gone}', 'spec', '{task}', 'task',
                 'references', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1,
                 'claude', 'claude');
         INSERT INTO links (id, project_id, from_id, from_type, to_id, to_type, rel, anchor,
             created_at, updated_at, version, created_by, updated_by)
         VALUES ('lnk_01H8XK4RPVBQ2N7DZM9C3FGTW2', '{p}', '{task}', 'task', '{gone}', 'spec',
                 'references', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1,
                 'claude', 'claude');",
        p = c.project
    ));

    assert_eq!(
        c.trips("dangling_link_source").severity,
        fsck::Severity::Error
    );
    assert_eq!(
        c.trips("dangling_link_target").severity,
        fsck::Severity::Error
    );
}

/// `depends_on` is never stored — `keel-core` swaps the endpoints and stores
/// `blocks`. A stored `depends_on` means both directions exist, which is the
/// graph bug this codebase fears most.
#[test]
fn depends_on_stored() {
    let mut c = Corrupt::new();
    let a = c.task("A");
    let b = c.task("B");
    c.quiet("depends_on_stored");

    c.sql(&format!(
        "UPDATE links SET rel = 'depends_on' WHERE 1;
         INSERT INTO links (id, project_id, from_id, from_type, to_id, to_type, rel, anchor,
             created_at, updated_at, version, created_by, updated_by)
         VALUES ('lnk_01H8XK4RPVBQ2N7DZM9C3FGTW3', '{p}', '{a}', 'task', '{b}', 'task',
                 'depends_on', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1,
                 'claude', 'claude');",
        p = c.project
    ));

    c.trips("depends_on_stored");
}

/// The denormalised type on an edge disagreeing with the row it points at.
/// Traversal trusts the edge, so the result is confidently wrong.
#[test]
fn link_type_mismatch() {
    let mut c = Corrupt::new();
    let a = c.task("A");
    let b = c.task("B");
    c.store
        .link(
            NewLink::new(a, Relation::References, b),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap();
    c.quiet("link_type_mismatch");

    c.sql("UPDATE links SET from_type = 'spec'");
    c.trips("link_type_mismatch");
}

#[test]
fn orphan_document() {
    let mut c = Corrupt::new();
    let spec = c.spec("Has prose");
    c.store
        .write_revision(
            Document::first(
                EntityType::Spec,
                spec.clone(),
                Some(c.project.clone()),
                "Has prose",
                "Some words.\n",
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();
    c.quiet("orphan_document");

    // The row goes, the revision stays: prose nothing can reach.
    c.sql(&format!("DELETE FROM specs WHERE id = '{spec}'"));
    c.trips("orphan_document");
}

#[test]
fn multiple_current_revisions() {
    let mut c = Corrupt::new();
    let spec = c.spec("Two heads");
    for body in ["one\n", "two\n"] {
        c.store
            .write_revision(
                Document::first(
                    EntityType::Spec,
                    spec.clone(),
                    Some(c.project.clone()),
                    "Two heads",
                    body,
                    Actor::Claude,
                    chrono::Utc::now(),
                )
                .unwrap(),
            )
            .unwrap();
    }
    c.quiet("multiple_current_revisions");

    c.sql(&format!(
        "UPDATE documents SET status = 'current' WHERE entity_id = '{spec}'"
    ));
    c.trips("multiple_current_revisions");
}

#[test]
fn orphan_task() {
    let mut c = Corrupt::new();
    c.task("Left behind");
    c.quiet("orphan_task");

    // A project archived without its tasks: rows on a board nobody can find.
    c.sql(&format!(
        "UPDATE projects SET archived_at = '2026-01-01T00:00:00Z' WHERE id = '{}'",
        c.project
    ));
    c.trips("orphan_task");
}

#[test]
fn stale_in_progress() {
    let mut c = Corrupt::new();
    let task = c.task("Claimed and forgotten");
    c.quiet("stale_in_progress");

    c.sql(&format!(
        "UPDATE tasks SET status = 'in_progress', updated_at = '2020-01-01T00:00:00Z' \
         WHERE id = '{task}'"
    ));
    c.trips("stale_in_progress");
}

#[test]
fn duplicate_task_number() {
    let mut c = Corrupt::new();
    let a = c.task("First");
    let b = c.task("Second");
    c.quiet("duplicate_task_number");

    // The unique index is on (project_id, number), so the collision has to be
    // made by dropping it — which is what a bad migration would do.
    c.sql(&format!(
        "DROP INDEX tasks_number;
         UPDATE tasks SET number = (SELECT number FROM tasks WHERE id = '{a}') WHERE id = '{b}';"
    ));
    c.trips("duplicate_task_number");
}

#[test]
fn task_without_number() {
    let mut c = Corrupt::new();
    let task = c.task("Unnumbered");
    c.quiet("task_without_number");

    c.sql(&format!(
        "UPDATE tasks SET number = NULL WHERE id = '{task}'"
    ));
    c.trips("task_without_number");
}

#[test]
fn project_without_key() {
    let c = Corrupt::new();
    c.quiet("project_without_key");

    c.sql(&format!(
        "UPDATE projects SET key = NULL WHERE id = '{}'",
        c.project
    ));
    c.trips("project_without_key");
}

#[test]
fn task_parent_cycle() {
    let mut c = Corrupt::new();
    let a = c.task("A");
    let b = c.task("B");
    c.quiet("task_parent_cycle");

    c.sql(&format!(
        "UPDATE tasks SET parent_id = '{b}' WHERE id = '{a}';
         UPDATE tasks SET parent_id = '{a}' WHERE id = '{b}';"
    ));
    c.trips("task_parent_cycle");
}

#[test]
fn task_parent_dangling() {
    let mut c = Corrupt::new();
    let task = c.task("Child of nothing");
    c.quiet("task_parent_dangling");

    c.sql(&format!(
        "UPDATE tasks SET parent_id = 'tsk_01H8XK4RPVBQ2N7DZM9C3FGTWY' WHERE id = '{task}'"
    ));
    c.trips("task_parent_dangling");
}

/// A citation of an identifier no artifact answers to. Only checked for
/// identifier families the project actually uses in titles, so the fixture has
/// to establish one first.
#[test]
fn unresolved_id_reference() {
    let mut c = Corrupt::new();
    let real = c.spec("TQ-1 — A question this project titles that way");
    c.store
        .write_revision(
            Document::first(
                EntityType::Spec,
                real.clone(),
                Some(c.project.clone()),
                "TQ-1 — A question this project titles that way",
                "This is the one that exists.\n",
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();
    c.quiet("unresolved_id_reference");

    let citing = c.spec("Cites something absent");
    c.store
        .write_revision(
            Document::first(
                EntityType::Spec,
                citing,
                Some(c.project.clone()),
                "Cites something absent",
                "As decided in TQ-99, we do it the other way.\n",
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();
    c.trips("unresolved_id_reference");
}

#[test]
fn event_without_actor() {
    let mut c = Corrupt::new();
    c.task("Something that happened");
    c.quiet("event_without_actor");

    c.sql("UPDATE events SET actor = '' WHERE 1");
    c.trips("event_without_actor");
}

#[test]
fn event_without_session() {
    let mut c = Corrupt::new();
    c.task("Something that happened");

    // Anonymous provenance already leaves this null, so this check does not
    // need corrupting so much as observing — which is worth a test of its own,
    // because it is the one check whose trigger is the ordinary case.
    c.sql("UPDATE events SET session_id = NULL WHERE 1");
    let finding = c.trips("event_without_session");
    assert_eq!(
        finding.severity,
        fsck::Severity::Warning,
        "unattributed events are worth knowing about, not worth stopping for"
    );
}

#[test]
fn row_without_creation_event() {
    let mut c = Corrupt::new();
    let task = c.task("Appeared from nowhere");
    c.quiet("row_without_creation_event");

    c.sql(&format!(
        "DELETE FROM events WHERE entity_id = '{task}' AND op = 'created'"
    ));
    c.trips("row_without_creation_event");
}

#[test]
fn live_link_to_archived() {
    let mut c = Corrupt::new();
    let a = c.task("Live");
    let b = c.task("Archived");
    c.store
        .link(
            NewLink::new(a, Relation::References, b.clone()),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap();
    c.quiet("live_link_to_archived");

    c.sql(&format!(
        "UPDATE tasks SET archived_at = '2026-01-01T00:00:00Z' WHERE id = '{b}'"
    ));
    c.trips("live_link_to_archived");
}

#[test]
fn orphan_blob() {
    let c = Corrupt::new();
    c.quiet("orphan_blob");

    c.sql(
        "INSERT INTO blobs (blob_id, entity_id, project_id, media_type, byte_length, sha256,
             bytes, created_at)
         VALUES ('blb_01H8XK4RPVBQ2N7DZM9C3FGTWY', NULL, NULL, 'image/png', 3, 'abc',
                 x'000102', '2026-01-01T00:00:00Z')",
    );
    c.trips("orphan_blob");
}

/// A phase storing a state that is supposed to be derived.
///
/// `planned`, `active` and `blocked` stopped being storable in B-57 because the
/// column drifted from the work — five of twelve phases contradicted their own
/// tasks, and the digest spent a week naming a finished phase as the live one.
/// Migration 3 rewrites them and `apply_changes` refuses to write one; this is
/// the audit for a hand-edited row or a restore from an older store.
#[test]
fn milestone_stores_a_derived_state() {
    let c = Corrupt::new();
    c.sql(&format!(
        "INSERT INTO milestones (id, project_id, kind, name, status, idempotency_key,
             created_at, updated_at, version, created_by, updated_by)
         VALUES ('mst_01H8XK4RPVBQ2N7DZM9C3FGTWY', '{p}', 'milestone', 'Phase X', 'open',
                 'phase-x', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1,
                 'claude', 'claude');",
        p = c.project
    ));
    c.quiet("milestone_stores_a_derived_state");

    for derived in ["planned", "active", "blocked", "complete"] {
        c.sql(&format!(
            "UPDATE milestones SET status = '{derived}' WHERE id = 'mst_01H8XK4RPVBQ2N7DZM9C3FGTWY'"
        ));
        c.trips("milestone_stores_a_derived_state");
    }

    // The three that are a person's to declare are never flagged.
    for declared in ["open", "paused", "shipped", "cut"] {
        c.sql(&format!(
            "UPDATE milestones SET status = '{declared}', shipped_at = '2026-01-02T00:00:00Z'
              WHERE id = 'mst_01H8XK4RPVBQ2N7DZM9C3FGTWY'"
        ));
        c.quiet("milestone_stores_a_derived_state");
    }
}

/// A shipped phase with no date on it.
///
/// `status` and `shipped_at` are two fields saying one thing, and this is the
/// state they drift into — reached by a caller that set the field it was given
/// and nothing else, which is exactly how Phases 7 and 9 got here.
#[test]
fn shipped_without_a_date() {
    let c = Corrupt::new();
    c.sql(&format!(
        "INSERT INTO milestones (id, project_id, kind, name, status, shipped_at,
             idempotency_key, created_at, updated_at, version, created_by, updated_by)
         VALUES ('mst_01H8XK4RPVBQ2N7DZM9C3FGTWY', '{p}', 'milestone', 'Phase X', 'shipped',
                 '2026-01-02T00:00:00Z', 'phase-x', '2026-01-01T00:00:00Z',
                 '2026-01-01T00:00:00Z', 1, 'claude', 'claude');",
        p = c.project
    ));
    c.quiet("shipped_without_a_date");

    c.sql("UPDATE milestones SET shipped_at = NULL WHERE id = 'mst_01H8XK4RPVBQ2N7DZM9C3FGTWY'");
    c.trips("shipped_without_a_date");

    // An undeclared phase with no date is not a finding — there is nothing to
    // date until somebody says it shipped.
    c.sql("UPDATE milestones SET status = 'open' WHERE id = 'mst_01H8XK4RPVBQ2N7DZM9C3FGTWY'");
    c.quiet("shipped_without_a_date");
}

/// A store whose write path builds passages, which the default `Corrupt` has
/// no embedder for.
///
/// The other tests in this file deliberately have no model attached — they are
/// about rows, not vectors — so attaching one everywhere would make every
/// unrelated test slower for no reason.
fn embedded() -> Corrupt {
    let mut c = Corrupt::new();
    c.store
        .set_embedder(std::sync::Arc::new(HashEmbedder::new()));
    c
}

/// Write a spec with a body, through the real path, so it gets real passages.
fn spec_with_prose(c: &mut Corrupt, title: &str, body: &str) -> EntityId {
    let id = c.spec(title);
    c.store
        .write_revision(
            Document::first(
                EntityType::Spec,
                id.clone(),
                Some(c.project.clone()),
                title,
                body,
                Actor::Claude,
                chrono::Utc::now(),
            )
            .unwrap(),
        )
        .unwrap();
    id
}

#[test]
fn document_without_passages() {
    let mut c = embedded();
    let spec = spec_with_prose(&mut c, "Has prose", "Some prose worth embedding.\n");
    c.quiet("document_without_passages");

    // The shape this catches in the wild: a revision written while the daemon
    // had no embedder, so the document is keyword-searchable and invisible to
    // the semantic half with nothing saying so.
    c.sql(&format!(
        "DELETE FROM document_chunks WHERE entity_id = '{spec}'"
    ));
    c.trips("document_without_passages");
}

#[test]
fn stale_passage() {
    let mut c = embedded();
    let spec = spec_with_prose(&mut c, "Changing", "The original text.\n");
    c.quiet("stale_passage");

    // An in-place edit, which `write_revision` would never do — it writes a new
    // revision and the trigger takes the old passages with it. Doing it behind
    // the API is the only way to reach the state this check exists for, and it
    // is exactly what a crash between two writes or a bad restore would leave.
    c.sql(&format!(
        "UPDATE documents SET body = 'Entirely different text.', \
         body_hash = 'not-the-hash-the-passages-were-built-from' \
         WHERE entity_id = '{spec}'"
    ));
    c.trips("stale_passage");
}

#[test]
fn passages_from_mixed_models() {
    let mut c = embedded();
    spec_with_prose(&mut c, "First", "Prose about storage.\n");
    let second = spec_with_prose(&mut c, "Second", "Prose about retrieval.\n");
    c.quiet("passages_from_mixed_models");

    // As if half the corpus had been re-embedded and the pass was interrupted.
    // Not corruption — it is the ordinary state during a model change, and the
    // reason it matters is that vectors of a different width are skipped by
    // search rather than failing it, so those rows quietly stop being findable.
    c.sql(&format!(
        "UPDATE document_chunks SET embedding_model = 'some-newer-model' \
         WHERE entity_id = '{second}'"
    ));
    c.trips("passages_from_mixed_models");
}
