//! The fixture corpus, `fsck`, and the backup round trip.
//!
//! These three are Phase 0's remaining exit criteria: a 200-entity fixture
//! loads, integrity checks pass, and a backup restores and diffs clean.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::backup;
use keel_core::*;
use std::sync::Arc;

fn loaded_store() -> (Store, tempfile::TempDir, fixture::FixtureSummary) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite"))
        .unwrap()
        .with_embedder(Arc::new(HashEmbedder::new()));
    let summary = fixture::load(&mut store).expect("load the fixture");
    (store, dir, summary)
}

// --- The fixture ---------------------------------------------------------

#[test]
fn the_fixture_loads_at_least_two_hundred_entities() {
    let (_s, _d, summary) = loaded_store();
    assert!(
        summary.total_entities() >= 200,
        "Phase 0's exit criterion is a 200-entity fixture; got {}",
        summary.total_entities()
    );
}

#[test]
fn the_fixture_covers_every_entity_type() {
    let (_s, _d, summary) = loaded_store();
    for t in fixture::EXPECTED_TYPES {
        let n = summary.entities.get(t.as_str()).copied().unwrap_or(0);
        assert!(n > 0, "the fixture creates no {t}");
    }
    assert_eq!(summary.entities.len(), 13);
}

#[test]
fn the_fixture_covers_every_relation() {
    let (_s, _d, summary) = loaded_store();
    for r in Relation::ALL {
        let n = summary.links.get(r.as_str()).copied().unwrap_or(0);
        assert!(
            n > 0,
            "the fixture creates no `{r}` link, so nothing exercises that direction"
        );
    }
}

#[test]
fn the_fixture_spans_more_than_one_project() {
    // G7 and UC-6: the multi-project roll-up cannot be exercised with one.
    let (store, _d, _) = loaded_store();
    let projects = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .unwrap();
    assert!(projects.total >= 3, "got {} project(s)", projects.total);
}

#[test]
fn the_fixture_writes_prose_that_search_can_actually_rank() {
    // R-3: retrieval quality must be evaluable on real queries. A corpus of
    // `task 1`, `task 2` makes every document equidistant from every query.
    let (store, _d, summary) = loaded_store();
    assert!(
        summary.revisions >= 30,
        "got {} revisions",
        summary.revisions
    );

    let hits = store
        .search(&SearchQuery::new("double billing on retries"))
        .unwrap();
    assert!(
        !hits.items.is_empty(),
        "a real question must return something"
    );

    let top = &hits.items[0];
    assert!(
        !top.excerpt.trim().is_empty(),
        "a hit with no excerpt is not useful to a human or a model"
    );
}

#[test]
fn the_fixture_produces_a_traversable_graph() {
    let (store, _d, _) = loaded_store();

    // Find a spec that something implements, and walk inbound to it — UC-7.
    let specs = store
        .list(&EntityQuery::default().of_type(EntityType::Spec))
        .unwrap();
    let mut found_any = false;
    for spec in &specs.items {
        let implementers = store
            .neighbours(
                spec.id(),
                Direction::Inbound,
                &[Relation::Implements],
                DEFAULT_DEPTH,
            )
            .unwrap();
        if !implementers.is_empty() {
            found_any = true;
            for n in &implementers {
                assert_eq!(
                    n.entity_type,
                    EntityType::Task,
                    "only tasks implement specs in this fixture"
                );
                assert!(!n.path.is_empty(), "a neighbour must carry its path");
            }
        }
    }
    assert!(found_any, "no spec in the fixture has an implementer");
}

#[test]
fn the_fixture_stores_depends_on_as_blocks() {
    let (store, _d, _) = loaded_store();
    let stored: i64 = store
        .connection()
        .query_row(
            "SELECT count(*) FROM links WHERE rel = 'depends_on'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, 0, "D-11: `depends_on` must never reach the table");
}

#[test]
fn the_fixture_creates_no_accidental_cross_project_links() {
    // Links may legitimately span projects — the column is nullable, and REQ-19
    // wants cross-project dependencies eventually. But none of *these* were
    // meant to, and two crept in when appending rows shifted the positional
    // indices the link section used. The links are addressed by name now; this
    // is the assertion that keeps them honest.
    let (store, _d, _) = loaded_store();
    let mut stmt = store
        .connection()
        .prepare(
            "SELECT vf.label, pf.slug, l.rel, vt.label, pt.slug
             FROM links l
             JOIN v_entities vf ON vf.id = l.from_id
             JOIN v_entities vt ON vt.id = l.to_id
             LEFT JOIN projects pf ON pf.id = vf.project_id
             LEFT JOIN projects pt ON pt.id = vt.project_id
             WHERE pf.slug IS DISTINCT FROM pt.slug",
        )
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| {
            Ok(format!(
                "{} ({}) --{}--> {} ({})",
                r.get::<_, String>(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<String>>(4)?.unwrap_or_default(),
            ))
        })
        .unwrap()
        .filter_map(std::result::Result::ok)
        .collect();

    assert!(
        rows.is_empty(),
        "the fixture linked artifacts across projects:\n  {}",
        rows.join("\n  ")
    );
}

#[test]
fn the_fixture_records_more_than_one_actor() {
    // The activity feed and "what did Claude do today" are meaningless
    // otherwise.
    let (store, _d, _) = loaded_store();
    let actors: i64 = store
        .connection()
        .query_row("SELECT count(DISTINCT actor) FROM events", [], |r| r.get(0))
        .unwrap();
    assert!(
        actors >= 2,
        "only {actors} distinct actor(s) in the event log"
    );
}

#[test]
fn loading_the_fixture_twice_creates_nothing_new() {
    // Because it goes through the ordinary write path, idempotency applies.
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    fixture::load(&mut store).unwrap();

    let before: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    fixture::load(&mut store).unwrap();
    let after: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, after, "re-loading must not duplicate the corpus");
}

// --- fsck ----------------------------------------------------------------

#[test]
fn a_freshly_loaded_fixture_passes_fsck() {
    let (store, _d, _) = loaded_store();
    let report = fsck::check(&store).unwrap();
    assert!(
        report.is_clean(),
        "fixture should be consistent, but: {:#?}",
        report.errors().collect::<Vec<_>>()
    );
    assert!(
        report.checks_run >= 20,
        "only {} checks ran",
        report.checks_run
    );
}

#[test]
fn fsck_notices_a_dangling_link() {
    let (store, _d, _) = loaded_store();
    // Corrupt the store behind keel-core's back, which is exactly the class of
    // damage fsck exists to find — a crash between two writes, or a restore
    // from a half-good backup.
    store
        .connection()
        .execute_batch(
            "UPDATE links SET to_id = 'tsk_01ZZZZZZZZZZZZZZZZZZZZZZZZ' \
             WHERE id = (SELECT min(id) FROM links);",
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    assert!(!report.is_clean());
    let finding = report
        .errors()
        .find(|f| f.check.starts_with("dangling_link"))
        .expect("fsck should have found the dangling link");
    assert!(
        finding.detail.contains("silently"),
        "the finding must explain the consequence: {}",
        finding.detail
    );
    assert!(!finding.remedy.is_empty());
}

#[test]
fn fsck_notices_a_document_pointer_that_leads_nowhere() {
    let (store, _d, _) = loaded_store();
    store
        .connection()
        .execute_batch("UPDATE specs SET current_doc_version = 99;")
        .unwrap();

    let report = fsck::check(&store).unwrap();
    assert!(
        report
            .errors()
            .any(|f| f.check.starts_with("doc_pointer_dangling")),
        "fsck missed a spec pointing at a revision that does not exist"
    );
}

#[test]
fn fsck_notices_a_depends_on_that_bypassed_normalisation() {
    let (store, _d, _) = loaded_store();
    store
        .connection()
        .execute_batch(
            "UPDATE links SET rel = 'depends_on' WHERE id = (SELECT min(id) FROM links);",
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    let finding = report
        .errors()
        .find(|f| f.check == "depends_on_stored")
        .expect("fsck must catch a stored depends_on");
    assert!(finding.detail.contains("D-11"), "{}", finding.detail);
}

#[test]
fn fsck_reports_an_orphaned_child_as_a_warning_not_an_error() {
    // Archiving a parent never cascades (SPEC §3.1). The orphan is expected,
    // and treating it as an error would make fsck cry wolf.
    let (mut store, _d, _) = loaded_store();
    let projects = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .unwrap();
    let project = &projects.items[0];
    store
        .archive(
            project.id(),
            project.audit().version,
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    assert!(
        report.is_clean(),
        "an orphaned child is untidy, not broken: {:#?}",
        report.errors().collect::<Vec<_>>()
    );
    assert!(
        report.findings.iter().any(|f| f.check == "orphan_task"),
        "but it should still be reported"
    );
}

// --- Backup round trip ---------------------------------------------------

#[test]
fn a_backup_restores_and_diffs_clean() {
    // Phase 0's exit criterion, stated as: back up → wipe → restore → diff.
    // "Assert equality, don't eyeball it."
    let (store, source_dir, summary) = loaded_store();

    let backup_dir = tempfile::tempdir().unwrap();
    let manifest = backup::backup(&store, backup_dir.path()).expect("backup");

    assert!(manifest.total_rows() > 0);
    assert!(
        manifest.counts.contains_key("documents"),
        "the prose is the whole escape hatch (R-5) and must be in the manifest"
    );
    assert!(
        manifest.counts["documents"] >= summary.revisions as i64,
        "every revision must be in the backup"
    );

    // Wipe: drop the original store entirely.
    drop(store);
    drop(source_dir);

    // Restore into a fresh directory.
    let target = tempfile::tempdir().unwrap();
    let restored_root = target.path().join("restored").join("keel.sqlite");
    let restored_manifest = backup::restore(backup_dir.path(), &restored_root).expect("restore");
    assert_eq!(restored_manifest, manifest);

    let restored = Store::open(&restored_root).expect("open the restored store");
    backup::verify_restore(&restored, &manifest).expect("restore diff");

    // And the restored store is not merely row-count-equal — it works.
    let report = fsck::check(&restored).unwrap();
    assert!(
        report.is_clean(),
        "restored store fails fsck: {:#?}",
        report.errors().collect::<Vec<_>>()
    );

    let hits = restored
        .search(&SearchQuery::new("invoice reconciliation"))
        .unwrap();
    assert!(!hits.items.is_empty(), "search must work after a restore");
}

#[test]
fn a_restored_document_keeps_its_embedding() {
    // The DuckDB version of this test asserted that the restore re-applied a
    // `FLOAT[384]` cast, because Parquet lost the fixed-size list type on the
    // way out. Nothing is converted now — the vectors are bytes the store owns
    // and `VACUUM INTO` copies them — so what is left to assert is the part
    // that was always the point: the vectors are there, and they are the right
    // width.
    let (store, _d, _) = loaded_store();
    let backup_dir = tempfile::tempdir().unwrap();
    backup::backup(&store, backup_dir.path()).unwrap();

    let target = tempfile::tempdir().unwrap();
    let root = target.path().join("restored").join("keel.sqlite");
    backup::restore(backup_dir.path(), &root).unwrap();

    let restored = Store::open(&root).unwrap();
    let with_vectors: i64 = restored
        .connection()
        .query_row(
            "SELECT count(*) FROM documents WHERE embedding IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        with_vectors > 0,
        "embeddings did not survive the round trip"
    );

    let bytes: i64 = restored
        .connection()
        .query_row(
            "SELECT length(embedding) FROM documents WHERE embedding IS NOT NULL LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        bytes,
        EMBEDDING_DIM as i64 * 4,
        "the vector width changed: {bytes} bytes is not {EMBEDDING_DIM} little-endian f32s"
    );
}

#[test]
fn restoring_over_an_existing_store_is_refused() {
    // A restore is run in a panic. "Restore over the top of the thing I was
    // trying to recover" is the mistake that makes an incident permanent.
    let (store, _d, _) = loaded_store();
    let backup_dir = tempfile::tempdir().unwrap();
    backup::backup(&store, backup_dir.path()).unwrap();

    let occupied = tempfile::tempdir().unwrap();
    let occupied_store = occupied.path().join("keel.sqlite");
    Store::open(&occupied_store).unwrap();

    let err = backup::restore(backup_dir.path(), &occupied_store).unwrap_err();
    assert!(
        err.to_string().contains("already exists"),
        "should refuse rather than overwrite: {err}"
    );
}

#[test]
fn a_backup_with_no_snapshot_in_it_is_refused_at_restore() {
    // What the DuckDB version of this test called "missing its Lance half".
    // With two engines a backup could be half-written and still look complete;
    // with one there is a single file, so the equivalent damage is that the
    // file is not there — and the refusal has to say so rather than opening an
    // empty store and reporting a successful restore of nothing.
    let (store, _d, _) = loaded_store();
    let backup_dir = tempfile::tempdir().unwrap();
    backup::backup(&store, backup_dir.path()).unwrap();

    std::fs::remove_file(backup_dir.path().join("keel.sqlite")).unwrap();

    let target = tempfile::tempdir().unwrap();
    let err = backup::restore(backup_dir.path(), target.path().join("restored")).unwrap_err();
    assert!(
        err.to_string().contains("no `keel.sqlite`"),
        "the error must say what is missing: {err}"
    );
}

#[test]
fn verify_restore_notices_a_missing_table() {
    let (store, _d, _) = loaded_store();
    let mut manifest = backup::backup(&store, tempfile::tempdir().unwrap().path()).unwrap();
    manifest.counts.insert("tasks".to_owned(), 99_999);

    let err = backup::verify_restore(&store, &manifest).unwrap_err();
    assert!(
        err.to_string().contains("tasks"),
        "a row-count mismatch must be reported: {err}"
    );
}

// --- The checks added in Phase 11 ----------------------------------------
//
// Three orphans the non-atomic write path could produce, and the file-level
// check nothing ever ran. Each corrupts the store behind the API, which is the
// only way to reach a state keel-core now refuses to create.

#[test]
fn fsck_notices_a_row_with_no_creation_event() {
    let (store, _d, _) = loaded_store();
    store
        .connection()
        .execute_batch(
            "DELETE FROM events WHERE op = 'created' AND entity_id = \
             (SELECT min(id) FROM tasks);",
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.check == "row_without_creation_event")
        .expect("fsck should have found the row with no history");
    assert_eq!(
        finding.severity,
        fsck::Severity::Warning,
        "the row still works; what is missing is its provenance"
    );
    assert!(finding.count >= 1);
}

#[test]
fn fsck_notices_a_live_link_into_an_archived_row() {
    let (store, _d, _) = loaded_store();
    // Archive a row without archiving what points at it — the half-archive the
    // transaction in KEEL-141 now prevents.
    store
        .connection()
        .execute_batch(
            "UPDATE tasks SET archived_at = '2026-08-12T00:00:00.000000Z' \
             WHERE id = (SELECT to_id FROM links WHERE archived_at IS NULL \
                         AND to_id LIKE 'tsk_%' LIMIT 1);",
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    let finding = report
        .errors()
        .find(|f| f.check == "live_link_to_archived")
        .expect("fsck should have found the live edge into an archived row");
    assert!(
        finding.detail.contains("disagree"),
        "the finding must say what breaks: {}",
        finding.detail
    );
}

#[test]
fn fsck_notices_a_blob_nothing_points_at() {
    let (store, _d, _) = loaded_store();
    store
        .connection()
        .execute_batch(
            "INSERT INTO blobs (blob_id, entity_id, project_id, media_type, byte_length, \
             sha256, bytes, created_at) \
             VALUES ('blb_01ZZZZZZZZZZZZZZZZZZZZZZZZ', NULL, NULL, 'image/png', 3, 'abc', \
             x'010203', '2026-08-12T00:00:00.000000Z');",
        )
        .unwrap();

    let report = fsck::check(&store).unwrap();
    let finding = report
        .findings
        .iter()
        .find(|f| f.check == "orphan_blob")
        .expect("fsck should have found the orphaned blob");
    assert!(
        finding.remedy.contains("DELETE"),
        "an orphaned blob is the one thing that can only be reclaimed by deletion: {}",
        finding.remedy
    );
}

/// A healthy store passes SQLite's own check, and the helper reports that as
/// `None` rather than as an empty list somebody has to interpret.
#[test]
fn a_healthy_store_passes_the_page_integrity_check() {
    let (store, _d, _) = loaded_store();
    assert_eq!(
        fsck::page_integrity(&store, "integrity_check").unwrap(),
        None
    );
    assert_eq!(fsck::page_integrity(&store, "quick_check").unwrap(), None);
}

/// The manifest describes the snapshot, not the store as it was a moment
/// before the snapshot.
///
/// Counting the live store and *then* taking the snapshot left a seam: a write
/// landing between the two put a row in the backup that the manifest did not
/// know about, and `verify_restore` then rejected a perfectly good backup — at
/// restore time, which is the worst moment available to refuse one.
#[test]
fn a_write_at_the_seam_cannot_make_the_manifest_disagree_with_the_snapshot() {
    use keel_core::{Actor, EntityQuery, EntityStore, EntityType, Provenance, Task};

    let (mut store, _d, _) = loaded_store();
    let project = store
        .list(
            &EntityQuery::default()
                .of_type(EntityType::Project)
                .limited(1),
        )
        .unwrap()
        .items
        .first()
        .expect("the fixture has projects")
        .id()
        .clone();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("backup");

    let manifest = backup::backup(&store, &dest).unwrap();

    // A write immediately after. Under the old ordering the equivalent write
    // landed *inside* the snapshot while the counts were already taken; the
    // assertion that matters either way is that the manifest matches the file
    // it describes.
    store
        .create(
            Task::new(
                project.clone(),
                "Written right after the backup",
                "A summary.",
            )
            .into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();

    let restored_dir = tempfile::tempdir().unwrap();
    let target = restored_dir.path().join("restored").join("keel.sqlite");
    let restored_manifest = backup::restore(&dest, &target).unwrap();
    let restored = Store::open(&target).unwrap();

    backup::verify_restore(&restored, &restored_manifest)
        .expect("a healthy backup must not fail its own verification");
    assert_eq!(
        manifest.counts, restored_manifest.counts,
        "the manifest written and the manifest read back describe different stores"
    );
}

/// A backup that SQLite wrote without erroring is not the same as a backup
/// that is sound.
#[test]
fn a_backup_verifies_the_snapshot_it_just_wrote() {
    let (store, _d, _) = loaded_store();
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("backup");

    let manifest = backup::backup(&store, &dest).unwrap();
    assert!(manifest.total_rows() > 0);

    // The counts came from the snapshot, so they have to match what is in it.
    let snapshot = rusqlite::Connection::open(dest.join("keel.sqlite")).unwrap();
    let tasks: i64 = snapshot
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        manifest.counts.get("tasks").copied().unwrap_or(-1),
        tasks,
        "the manifest's task count does not match the snapshot it describes"
    );
}
