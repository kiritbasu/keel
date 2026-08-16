//! Generating repository files from Specline.
//!
//! The direction that matters: Specline is the source of truth and the markdown in
//! the repository is an output. These tests cover the three ways that can go
//! quietly wrong — a document written somewhere other than the file it came
//! from, a document written to *two* files with no answer to which wins, and a
//! hand edit silently overwritten instead of reported.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Document, EntityId, EntityStore, EntityType, Mode, Project, Provenance, Spec, Store,
    generate,
};

/// A store with one project whose checkout is `repo`, plus a spec that has
/// adopted `product/SPEC.md`.
///
/// The `TempDir` comes back to the caller rather than staying here, and it is
/// the first element so that binding it is hard to forget. It used to be
/// `Box::leak`ed — "the process is a test binary", which is true and still cost
/// something: nineteen tests call this, so a passing run of this one file left
/// nineteen stores in `TMPDIR` for the sweeper to find later (KEEL-189).
///
/// Bind it as `_home`, not `_`. A bare `_` drops immediately, which would take
/// the directory out from under the store the rest of the test is using.
fn fixture(repo: &std::path::Path, body: &str) -> (tempfile::TempDir, Store, EntityId, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
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

    (dir, store, project_id, spec_id)
}

/// Point a project's decision log at `path`.
fn set_decisions_path(store: &mut Store, project_id: &EntityId, path: &str, prov: &Provenance) {
    let version = store.get(project_id).unwrap().unwrap().audit().version;
    let mut changes = serde_json::Map::new();
    changes.insert("decisions_path".to_owned(), serde_json::json!(path));
    store.update(project_id, version, &changes, prov).unwrap();
}

const BODY: &str = "# Demo — Technical Specification\n\n\
                    > Status: Draft\n\n\
                    ## 1. Storage\n\n\
                    DuckDB and Lance, attached as one namespace.\n";

/// The fixture has to take its store with it when the caller lets go.
///
/// The guard on KEEL-189. `fixture` used to `Box::leak` its `TempDir`, so each
/// of the nineteen tests below left a store behind in `TMPDIR` — and nothing
/// failed, which is exactly why it lasted: a leak has no assertion to trip. The
/// only symptom was a directory count going up, noticed while measuring
/// something else.
#[test]
fn the_fixture_takes_its_store_with_it_when_dropped() {
    let repo = tempfile::tempdir().unwrap();

    let home = {
        let (home, _store, _project_id, _spec_id) = fixture(repo.path(), BODY);
        let path = home.path().to_path_buf();
        assert!(
            path.join("specline.sqlite").exists(),
            "the fixture should have built a store to begin with"
        );
        path
    };

    assert!(
        !home.exists(),
        "the fixture's directory must be gone once the caller drops it, not left \
         for the sweeper to find later"
    );
}

#[test]
fn an_adopted_document_is_written_to_the_file_it_claims() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);

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
        written.starts_with("<!-- specline:generated"),
        "a generated file must say so, or someone edits it by hand"
    );
    // The banner is an HTML comment specifically so it does not become a
    // visible line in every renderer — and so `product/CLAUDE.md`, which
    // Claude Code loads verbatim, is not led by a stray heading.
    assert!(!written.starts_with("# specline:generated"));
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
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);

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
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let status = std::fs::read_to_string(repo.path().join("docs/STATUS.md"))
        .expect("the tracker goes where the project says, creating the directory");
    assert!(status.contains("Demo"), "the tracker names its project");
}

#[test]
fn a_second_run_that_changes_nothing_writes_nothing() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);

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
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);
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
    let (_home, mut store, project_id, spec_id) = fixture(repo.path(), BODY);
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
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);

    // Something the repo already has, that Specline has not been given yet.
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
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);

    // Point the tracker at a file a prose document already owns — the exact
    // situation Specline's own project is in, because `product/STATUS.md` was
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
    use specline_core::{Question, QuestionStatus};

    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
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
    use specline_core::{Decision, DecisionStatus};

    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
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
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
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
/// carrying a `specline:generated` banner and a real id — plausible, greppable, and
/// permanently wrong, with nothing to say so. Correcting seven truncated
/// decision titles produced seven such orphans on 2026-08-10 (TQ-28).
#[test]
fn renaming_an_artifact_removes_the_file_its_old_name_produced() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let created = store
        .create(
            specline_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
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
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let created = store
        .create(
            specline_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
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
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    store
        .create(
            specline_core::Decision::new(project_id.clone(), "Use DuckDB").into(),
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
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);
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

// --- Torn writes ---------------------------------------------------------

/// A generated file is the old content or the new content, never a prefix.
///
/// `product/CLAUDE.md` is one of these files, and Claude Code loads it at the
/// start of every session — so a write that stops halfway silently removes the
/// second half of the standing contract, and every session afterwards follows
/// whatever survived.
#[test]
fn a_generated_file_is_never_left_half_written() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("product").join("CLAUDE.md");
    let old = "# The contract\n\nEvery rule, all the way to the end.\n";
    let new = "# The contract\n\nA replacement that is longer than what it replaces, so a \
               prefix of it would still look like a whole file.\n";

    specline_core::atomic::write(&path, old).unwrap();

    // Make the temp file impossible to create, which is the cheapest stand-in
    // for a full disk: the write fails after the old file is already there.
    let temp = std::fs::read_dir(dir.path().join("product"))
        .unwrap()
        .count();
    assert_eq!(temp, 1, "only the target should exist between writes");

    let blocker = dir
        .path()
        .join("product")
        .join(format!(".CLAUDE.md.keel-{}.tmp", std::process::id()));
    std::fs::create_dir(&blocker).unwrap();

    let failed = specline_core::atomic::write(&path, new);
    assert!(failed.is_err(), "the write should have failed");

    let actual = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        actual, old,
        "the file holds neither the old content nor the new — it is a torn write"
    );
    assert!(
        !new.starts_with(&actual) || actual == old,
        "the file is a prefix of the new content"
    );
}

/// Deciding and writing are two phases, and the second needs nothing from the
/// store.
///
/// This is the property the daemon depends on: it holds the whole store behind
/// one mutex, so a generate that wrote files with the lock held blocked every
/// other request for the length of a few dozen filesystem writes — including
/// the health probe the CLI uses to decide whether a daemon is there at all.
///
/// Asserted by closing the store between the halves. If `apply` ever reaches
/// back for a row, this stops compiling rather than failing at runtime, which
/// is the strongest form the assertion can take.
#[test]
fn a_plan_can_be_applied_after_the_store_is_gone() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, store, project_id, _spec) = fixture(repo.path(), "# Spec\n\nBody.\n");

    let plan = generate::plan(&store, &project_id, repo.path()).unwrap();

    // Nothing has been written yet. A plan is a decision, and a decision that
    // had already touched the disk would not be one.
    assert!(
        !repo.path().join("product/SPEC.md").exists(),
        "planning must not write anything"
    );
    assert!(!repo.path().join(".keel").exists());

    drop(store);

    let report = plan.apply(Mode::Write).unwrap();
    assert!(repo.path().join("product/SPEC.md").is_file());
    assert!(repo.path().join(".keel/manifest.json").is_file());
    assert!(
        report.written.iter().any(|p| p == "product/SPEC.md"),
        "the report should name what it wrote: {:?}",
        report.written
    );
}

/// The split must not have changed what a generate produces.
///
/// Two runs, one through each door, against identical stores: same files, same
/// report. A refactor of this shape is exactly the kind that silently drops one
/// file, and the mirror's own orphan pass would then delete it on the next run.
#[test]
fn planning_then_applying_matches_generating_directly() {
    let one = tempfile::tempdir().unwrap();
    let (_home_a, store_a, project_a, _) = fixture(one.path(), "# Spec\n\nBody.\n");
    let direct = generate::all(&store_a, &project_a, one.path(), Mode::Write).unwrap();

    let two = tempfile::tempdir().unwrap();
    let (_home_b, store_b, project_b, _) = fixture(two.path(), "# Spec\n\nBody.\n");
    let split = generate::plan(&store_b, &project_b, two.path())
        .unwrap()
        .apply(Mode::Write)
        .unwrap();

    assert_eq!(direct.written, split.written);
    assert_eq!(direct.unchanged, split.unchanged);
    assert_eq!(direct.orphans, split.orphans);
    assert_eq!(direct.unrepresented, split.unrepresented);

    for relative in &direct.written {
        let a = std::fs::read_to_string(one.path().join(relative)).unwrap();
        let b = std::fs::read_to_string(two.path().join(relative)).unwrap();
        assert_eq!(
            generate::strip_banner_public(&a),
            generate::strip_banner_public(&b),
            "{relative} differs between the two doors"
        );
    }
}

/// A check run still reads the files it is comparing against, and still writes
/// none of them.
#[test]
fn a_checked_plan_reports_without_writing() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, store, project_id, _) = fixture(repo.path(), "# Spec\n\nBody.\n");

    let report = generate::plan(&store, &project_id, repo.path())
        .unwrap()
        .apply(Mode::Check)
        .unwrap();

    assert!(
        report.written.iter().any(|p| p == "product/SPEC.md"),
        "check should say the file would be written"
    );
    assert!(
        !repo.path().join("product/SPEC.md").exists(),
        "and must not have written it"
    );
}

// --- The tracker/changelog split ------------------------------------------
//
// Closed work used to be rendered into the tracker and grew without bound
// there: 488 KB on Specline's own project, 87% of it finished tasks, at which point
// the file exceeded what a reader would open and the standing contract's first
// instruction — read the tracker — could not be carried out. The split is by
// what the reader is asking, so these tests are about which file a row lands in
// and about the tracker saying what it left out.

/// Add one open and one closed task to a project, and return the closed one's
/// title.
fn one_of_each(store: &mut Store, project_id: &EntityId) -> String {
    use specline_core::{Close, CloseReason, Task, work};
    let prov = Provenance::anonymous(Actor::Human);

    store
        .create(
            Task::new(project_id.clone(), "Still to do", "An open row.").into(),
            &prov,
        )
        .unwrap();

    let closed = store
        .create(
            Task::new(project_id.clone(), "Already finished", "A closed row.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    work::close(
        store,
        &closed,
        &Close {
            reason: CloseReason::Done,
            message: "It was done, and here is why that is known.".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &prov,
    )
    .unwrap();

    "Already finished".to_owned()
}

#[test]
fn closed_work_goes_to_the_changelog_and_not_the_tracker() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    let closed_title = one_of_each(&mut store, &project_id);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let tracker = std::fs::read_to_string(repo.path().join("docs/STATUS.md")).unwrap();
    let changelog = std::fs::read_to_string(repo.path().join("docs/CHANGELOG.md"))
        .expect("the changelog is written beside the tracker");

    // The Tasks section only. A closed row is still allowed to appear in the
    // recent-changes tail — that is what the tail is for, and it is bounded.
    // What must not be there is the row itself, with its body and its notes,
    // which is the part that grew without bound.
    let tasks_section = {
        let start = tracker.find("## Tasks").expect("the tracker lists tasks");
        let rest = &tracker[start..];
        let end = rest.find("\n---").unwrap_or(rest.len());
        &rest[..end]
    };

    assert!(
        tasks_section.contains("Still to do"),
        "open work belongs in the tracker"
    );
    assert!(
        !tasks_section.contains(&closed_title),
        "closed work must not be listed in the tracker's task section — it is the part \
         that grows without bound, and it is what made the file too large to read: \
         {tasks_section}"
    );
    assert!(
        !tracker.contains("### done"),
        "and the tracker should not have a done group at all"
    );
    assert!(
        changelog.contains(&closed_title),
        "and it must be in the changelog, or the split has lost it"
    );
    assert!(
        changelog.contains("It was done, and here is why that is known."),
        "with the close message, which is the part worth reading"
    );
    assert!(
        changelog.contains("commit:abc1234"),
        "and the evidence, so a claim in it can be checked"
    );
}

/// Hard constraint 4: every list that can be cut says that it was, with a
/// total. A tracker that quietly stopped showing closed rows would read as a
/// project that had never finished anything.
#[test]
fn the_tracker_says_how_much_it_left_out() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    one_of_each(&mut store, &project_id);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();
    let tracker = std::fs::read_to_string(repo.path().join("docs/STATUS.md")).unwrap();

    assert!(
        tracker.contains("1 closed task(s) are not listed here"),
        "the tracker must state the count it omitted and where it went: {tracker}"
    );
}

/// A project with nothing closed yet gets a changelog that says so, rather than
/// no file at all — an absent file reads as a feature that is not working.
#[test]
fn a_project_with_nothing_closed_still_gets_a_changelog() {
    let repo = tempfile::tempdir().unwrap();
    let (_home, store, project_id, _) = fixture(repo.path(), BODY);

    generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let changelog = std::fs::read_to_string(repo.path().join("docs/CHANGELOG.md")).unwrap();
    assert!(
        changelog.contains("Nothing has closed yet"),
        "an empty changelog should say it is empty: {changelog}"
    );
}

/// The collision rule, same as the tracker's and the decision log's: a document
/// that has adopted the path owns it, and the derived file is skipped with a
/// reason rather than clobbering prose somebody wrote.
#[test]
fn a_document_that_adopted_the_changelog_path_keeps_it() {
    use specline_core::{Document, Spec};

    let repo = tempfile::tempdir().unwrap();
    let (_home, mut store, project_id, _) = fixture(repo.path(), BODY);
    let prov = Provenance::anonymous(Actor::Human);

    let mut spec = Spec::new(project_id.clone(), "A hand-written changelog");
    spec.mirror_path = Some("docs/CHANGELOG.md".to_owned());
    let spec_id = store
        .create(spec.into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    let doc = Document::first(
        EntityType::Spec,
        spec_id,
        Some(project_id.clone()),
        "A hand-written changelog",
        "# Written by a person\n",
        Actor::Human,
        chrono::Utc::now(),
    )
    .unwrap();
    store.write_revision(doc).unwrap();

    let report = generate::all(&store, &project_id, repo.path(), Mode::Write).unwrap();

    let written = std::fs::read_to_string(repo.path().join("docs/CHANGELOG.md")).unwrap();
    assert!(
        written.contains("Written by a person"),
        "the document owns the path and its prose must survive: {written}"
    );
    assert!(
        report
            .unrepresented
            .iter()
            .any(|u| u.contains("docs/CHANGELOG.md")),
        "and the skipped changelog must be reported, not dropped in silence: {:?}",
        report.unrepresented
    );
}
