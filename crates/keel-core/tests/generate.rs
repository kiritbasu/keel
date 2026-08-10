//! Generating repository files from Keel.
//!
//! The direction that matters: Keel is the source of truth and the markdown in
//! the repository is an output. These tests cover the three ways that can go
//! quietly wrong — a document written somewhere other than the file it came
//! from, a document written to *two* files with no answer to which wins, and a
//! hand edit silently overwritten instead of reported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{
    Actor, Document, DocumentStore, DuckStore, EntityId, EntityStore, EntityType, Mode, Project,
    Provenance, Spec, generate,
};

/// A store with one project whose checkout is `repo`, plus a spec that has
/// adopted `product/SPEC.md`.
fn fixture(repo: &std::path::Path, body: &str) -> (DuckStore, EntityId, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    // Leaked so the store outlives the helper; the process is a test binary.
    let dir = Box::leak(Box::new(dir));
    let mut store = DuckStore::open(dir.path()).unwrap();
    let prov = Provenance::anonymous(Actor::Human);

    let mut project = Project::new("demo", "Demo");
    project.root_path = Some(repo.display().to_string());
    project.status_path = Some("docs/STATUS.md".to_owned());
    let project_id = store
        .create(project.into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();

    let mut spec = Spec::new(project_id.clone(), "Demo — Technical Specification");
    spec.mirror_path = Some("product/SPEC.md".to_owned());
    let spec_id = store
        .create(spec.into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();

    let doc = Document::first(
        EntityType::Spec,
        spec_id.clone(),
        Some(project_id.clone()),
        "Demo — Technical Specification",
        body,
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    (store, project_id, spec_id)
}

/// Point a project's decision log at `path`.
fn set_decisions_path(store: &mut DuckStore, project_id: &EntityId, path: &str, prov: &Provenance) {
    let version = store.get(project_id).unwrap().unwrap().audit().version;
    let mut changes = serde_json::Map::new();
    changes.insert("decisions_path".to_owned(), serde_json::json!(path));
    store.update(project_id, version, &changes, prov).unwrap();
}

const BODY: &str = "# Demo — Technical Specification\n\n\
                    > Status: Draft\n\n\
                    ## 1. Storage\n\n\
                    DuckDB and Lance, attached as one namespace.\n";

#[test]
fn an_adopted_document_is_written_to_the_file_it_claims() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    assert!(
        report.written.contains(&"product/SPEC.md".to_owned()),
        "the spec must land at its recorded path, got {:?}",
        report.written
    );

    let written = std::fs::read_to_string(repo.path().join("product/SPEC.md")).unwrap();
    assert!(
        written.contains(BODY.trim()),
        "the body must be written verbatim, not summarised or re-headed"
    );
    assert!(
        written.starts_with("<!-- keel:generated"),
        "a generated file must say so, or someone edits it by hand"
    );
    // The banner is an HTML comment specifically so it does not become a
    // visible line in every renderer — and so `product/CLAUDE.md`, which
    // Claude Code loads verbatim, is not led by a stray heading.
    assert!(!written.starts_with("# keel:generated"));
    // No revision number: it is excluded from the change comparison, so any
    // number written here would go stale and quietly lie.
    assert!(
        !written.lines().next().unwrap_or_default().contains(" v1"),
        "the banner must not carry a version it cannot keep accurate"
    );
}

#[test]
fn an_adopted_document_does_not_also_appear_in_the_mirror() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let slugged = repo
        .path()
        .join(".keel/specs/demo-technical-specification.md");
    assert!(
        !slugged.exists(),
        "a document with a home of its own must not also be written into the \
         mirror — two files and no answer to which is authoritative is exactly \
         the reconciliation failure D-3 exists to prevent"
    );
}

#[test]
fn the_tracker_is_generated_at_the_projects_status_path() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let status = std::fs::read_to_string(repo.path().join("docs/STATUS.md"))
        .expect("the tracker goes where the project says, creating the directory");
    assert!(status.contains("Demo"), "the tracker names its project");
}

#[test]
fn a_second_run_that_changes_nothing_writes_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    let again = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert!(
        again.is_current(),
        "regenerating an unchanged store must not touch a file — otherwise every \
         run dirties the working tree and the commits are all empty. Wrote: {:?}",
        again.written
    );
}

#[test]
fn check_mode_reports_a_hand_edit_and_does_not_overwrite_it() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let path = repo.path().join("product/SPEC.md");
    let edited = std::fs::read_to_string(&path).unwrap() + "\n## 2. Smuggled in by hand\n";
    std::fs::write(&path, &edited).unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Check).unwrap();
    assert!(
        report.written.contains(&"product/SPEC.md".to_owned()),
        "an edit to a generated file must be reported, or it is lost silently"
    );
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        edited,
        "check mode must not write — its whole job is to report before anyone loses work"
    );
}

#[test]
fn a_new_revision_in_keel_reaches_the_file() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, spec_id) = fixture(repo.path(), BODY);
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let revised = format!("{BODY}\n## 2. Search\n\nBM25 in DuckDB, vectors in Lance.\n");
    let doc = Document::first(
        EntityType::Spec,
        spec_id,
        Some(project_id.clone()),
        "Demo — Technical Specification",
        &revised,
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    assert!(report.written.contains(&"product/SPEC.md".to_owned()));
    assert!(
        std::fs::read_to_string(repo.path().join("product/SPEC.md"))
            .unwrap()
            .contains("BM25 in DuckDB"),
        "the file must carry the newest revision — that is the entire point"
    );
}

#[test]
fn an_artifact_that_claims_a_path_but_has_no_prose_yet_is_skipped_not_emptied() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);

    // Something the repo already has, that Keel has not been given yet.
    std::fs::create_dir_all(repo.path().join("product")).unwrap();
    std::fs::write(repo.path().join("product/PRD.md"), "# Real content\n").unwrap();

    let mut spec = Spec::new(project_id.clone(), "Demo — Product Requirements");
    spec.mirror_path = Some("product/PRD.md".to_owned());
    store
        .create(spec.into(), &Provenance::anonymous(Actor::Human))
        .unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert_eq!(
        std::fs::read_to_string(repo.path().join("product/PRD.md")).unwrap(),
        "# Real content\n",
        "an artifact with no revision must not blank the file it claims"
    );
    assert!(
        report
            .unrepresented
            .iter()
            .any(|s| s.contains("product/PRD.md")),
        "and it must say so rather than passing silently, got {:?}",
        report.unrepresented
    );
}

#[test]
fn the_tracker_never_overwrites_a_document_that_claimed_the_same_file() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);

    // Point the tracker at a file a prose document already owns — the exact
    // situation Keel's own project is in, because `product/STATUS.md` was
    // hand-written long before there were task rows to render it from.
    let mut spec = Spec::new(project_id.clone(), "Demo — Status");
    spec.mirror_path = Some("docs/STATUS.md".to_owned());
    let spec_id = store
        .create(spec.into(), &Provenance::anonymous(Actor::Human))
        .unwrap()
        .entity
        .id()
        .clone();
    let doc = Document::first(
        EntityType::Spec,
        spec_id,
        Some(project_id.clone()),
        "Demo — Status",
        "# Demo — Status\n\nCarefully written prose nobody wants to lose.\n",
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let written = std::fs::read_to_string(repo.path().join("docs/STATUS.md")).unwrap();
    assert!(
        written.contains("Carefully written prose nobody wants to lose"),
        "the prose must survive: it cannot be regenerated, and the tracker can"
    );
    assert!(
        report
            .unrepresented
            .iter()
            .any(|s| s.contains("docs/STATUS.md")),
        "and the collision must be reported, not resolved silently by whichever \
         writer happened to run last. Got {:?}",
        report.unrepresented
    );
}

/// The register has to carry the settled half too.
///
/// An unresolved question stops someone deciding what nobody has decided. A
/// *settled* one stops them re-deciding what was already settled — the more
/// expensive mistake, and the one an agent joining a project makes by default.
/// Emitting only the open half is what left `product/QUESTIONS.md` maintaining
/// the other half by hand, and two registers that must agree is no register.
#[test]
fn the_questions_file_carries_settled_questions_as_well_as_open_ones() {
    use keel_core::{Question, QuestionStatus};

    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let mut still_open = Question::new(project_id.clone(), "Where does the store live?");
    still_open.status = QuestionStatus::Open;
    store.create(still_open.into(), &prov).unwrap();

    let mut settled = Question::new(project_id.clone(), "Local or hosted embeddings?");
    settled.status = QuestionStatus::Answered;
    let settled_id = store
        .create(settled.into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();

    let doc = Document::first(
        EntityType::Question,
        settled_id,
        Some(project_id.clone()),
        "Local or hosted embeddings?",
        "Local. A local-first store that phones an API to index a private spec is not local-first.",
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    let written = std::fs::read_to_string(repo.path().join(".keel/questions.md")).unwrap();

    assert!(written.contains("Where does the store live?"), "{written}");
    assert!(written.contains("Local or hosted embeddings?"), "{written}");
    assert!(written.contains("## Open"), "{written}");
    assert!(written.contains("## Settled"), "{written}");

    // The answer itself, not just the title — a settled question whose
    // reasoning is missing invites exactly the re-litigation it exists to stop.
    assert!(written.contains("is not local-first"), "{written}");

    // Order matters: open first. The file is read top-down by someone
    // orienting, and what is undecided is what they can still affect.
    assert!(
        written.find("## Open").unwrap() < written.find("## Settled").unwrap(),
        "{written}"
    );
}

/// The decision log is rendered from rows, at the project's `decisions_path`.
///
/// The register used to be two things that had to agree by hand: a numbered
/// prose table and one generated file per decision. This is the half that makes
/// the table stop existing.
#[test]
fn the_decision_log_is_generated_at_the_projects_decisions_path() {
    use keel_core::{Decision, DecisionStatus};

    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    set_decisions_path(&mut store, &project_id, "docs/DECISIONS.md", &prov);

    let live = store
        .create(
            Decision::new(project_id.clone(), "chrono, not jiff").into(),
            &prov,
        )
        .unwrap()
        .entity;
    let doc = Document::first(
        EntityType::Decision,
        live.id().clone(),
        Some(project_id.clone()),
        "chrono, not jiff",
        "## Decision\n\nchrono.\n\n## Reasoning\n\nduckdb-rs has a chrono feature and no jiff one.",
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    let mut reversed = Decision::new(project_id.clone(), "blocked is a status");
    reversed.status = DecisionStatus::Superseded;
    let reversed = store.create(reversed.into(), &prov).unwrap().entity;
    let doc = Document::first(
        EntityType::Decision,
        reversed.id().clone(),
        Some(project_id.clone()),
        "blocked is a status",
        "## Decision\n\nA status.\n\n## Superseded\n\nDerived from the edges instead.",
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    let written = std::fs::read_to_string(repo.path().join("docs/DECISIONS.md")).unwrap();

    // Numbered, and the number is the heading — that is what prose cites.
    assert!(written.contains("### B-1 — chrono, not jiff"), "{written}");
    assert!(
        written.contains("duckdb-rs has a chrono feature"),
        "{written}"
    );

    // A reversal is surfaced above the decisions, with the reason, because a
    // reversal met after acting on the original is one met too late.
    assert!(written.contains("## Reversals"), "{written}");
    assert!(
        written.contains("Derived from the edges instead."),
        "{written}"
    );
    assert!(
        written.find("## Reversals").unwrap() < written.find("### B-1").unwrap(),
        "{written}"
    );

    // The body's own headings are demoted, or every decision's `## Decision`
    // reads as a top-level section of the log.
    assert!(written.contains("#### Decision"), "{written}");
    assert!(!written.contains("\n## Decision\n"), "{written}");
}

/// A document that has adopted the path wins, and the conflict is reported.
///
/// Same rule as the tracker: the prose cannot be regenerated and the log can, so
/// neither is written rather than letting whichever runs last silently take the
/// file.
#[test]
fn the_decision_log_never_overwrites_a_document_that_claimed_the_same_file() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    set_decisions_path(&mut store, &project_id, "product/SPEC.md", &prov);

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert!(
        report
            .unrepresented
            .iter()
            .any(|u| u.contains("decisions_path")),
        "the collision must be reported: {:?}",
        report.unrepresented
    );
    let spec = std::fs::read_to_string(repo.path().join("product/SPEC.md")).unwrap();
    assert!(
        spec.contains("DuckDB and Lance"),
        "the prose must survive: {spec}"
    );
}

/// Renaming an artifact removes the file its old name produced.
///
/// `generate` used to only ever write, so a rename left the old slug on disk
/// carrying a `keel:generated` banner and a real id — plausible, greppable, and
/// permanently wrong, with nothing to say so. Correcting seven truncated
/// decision titles produced seven such orphans on 2026-08-10 (TQ-28).
#[test]
fn renaming_an_artifact_removes_the_file_its_old_name_produced() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let created = store
        .create(
            keel_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
            &prov,
        )
        .unwrap()
        .entity;
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let old = repo.path().join(".keel/decisions/use-duckdb.md");
    assert!(old.is_file(), "the first run should have written it");

    let mut changes = serde_json::Map::new();
    changes.insert("title".to_owned(), serde_json::json!("Use SQLite"));
    store
        .update(created.id(), created.audit().version, &changes, &prov)
        .unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert!(
        !old.exists(),
        "the file the old title produced must not survive the rename"
    );
    assert!(
        repo.path().join(".keel/decisions/use-sqlite.md").is_file(),
        "the new one must be written"
    );
    assert_eq!(
        report.orphans,
        vec![".keel/decisions/use-duckdb.md".to_owned()],
        "and the removal must be reported, never silent"
    );
}

/// `--check` reports an orphan and deletes nothing.
///
/// The mode exists so a hook can refuse a commit; a checking run that mutated
/// the tree would be the same betrayal as the hook that silently did nothing.
#[test]
fn check_mode_reports_an_orphan_without_removing_it() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let created = store
        .create(
            keel_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
            &prov,
        )
        .unwrap()
        .entity;
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let mut changes = serde_json::Map::new();
    changes.insert("title".to_owned(), serde_json::json!("Use SQLite"));
    store
        .update(created.id(), created.audit().version, &changes, &prov)
        .unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Check).unwrap();

    assert!(
        repo.path().join(".keel/decisions/use-duckdb.md").is_file(),
        "check mode must not delete"
    );
    assert_eq!(
        report.orphans,
        vec![".keel/decisions/use-duckdb.md".to_owned()]
    );
    assert!(
        !report.is_current(),
        "a tree carrying an orphan does not match the store, so --check must fail"
    );
}

/// No manifest means "nothing known", never "everything is an orphan".
///
/// The dangerous reading. A first run, a manifest deleted by hand, or one
/// written by a version that could not be parsed must all leave the tree alone
/// — the alternative is a delete-everything bug on the one code path that
/// deletes.
#[test]
fn a_missing_or_unreadable_manifest_removes_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let (mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    store
        .create(
            keel_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
            &prov,
        )
        .unwrap();
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let decision = repo.path().join(".keel/decisions/use-duckdb.md");
    let spec = repo.path().join("product/SPEC.md");
    assert!(decision.is_file() && spec.is_file());

    std::fs::write(repo.path().join(".keel/manifest.json"), "{ not json").unwrap();
    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert!(report.orphans.is_empty(), "{:?}", report.orphans);
    assert!(decision.is_file(), "an unreadable manifest must not delete");
    assert!(spec.is_file(), "and must certainly not reach product/");
}

/// Pruning never reaches outside the mirror root.
///
/// An adopted document lives in `product/` and is worth more than any mirror
/// file — 53 KB of prose in this repository's case. The manifest lists mirror
/// paths only, and the write path refuses anything that is not one, so a
/// corrupt or hostile manifest cannot make generation delete a spec.
#[test]
fn pruning_refuses_paths_outside_the_mirror_root() {
    let repo = tempfile::tempdir().unwrap();
    let (store, project_id, _) = fixture(repo.path(), BODY);
    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let manifest = repo.path().join(".keel/manifest.json");
    let mut parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
    for path in [
        "product/SPEC.md",
        "../escape.md",
        ".keel/../product/SPEC.md",
    ] {
        parsed["files"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({ "path": path, "contributors": [] }));
    }
    std::fs::write(&manifest, parsed.to_string()).unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    assert!(report.orphans.is_empty(), "{:?}", report.orphans);
    assert!(
        repo.path().join("product/SPEC.md").is_file(),
        "the spec must survive a manifest that names it"
    );
}
