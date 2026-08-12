//! An entity, its first revision and its image are one write.
//!
//! This used to be four store calls orchestrated in `keel-mcp` over untyped
//! JSON. A crash partway left an entity with no body, or a blob no entity
//! points at — and because `fsck` had no blob check, that orphan was invisible
//! and therefore unreclaimable forever.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "support/faults.rs"]
mod faults;

use keel_core::{Actor, Design, EntityId, EntityStore, Project, Provenance, Spec, Store, Task};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_composite")
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

/// A one-pixel PNG. Small enough to inline, real enough to store.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
];

#[test]
fn a_spec_and_its_body_arrive_together() {
    let (_d, mut store, project) = fixture();

    let out = store
        .create_with_document(
            Spec::new(project.clone(), "Storage specification").into(),
            Some("The store is one SQLite file.".to_owned()),
            None,
            &prov(),
        )
        .unwrap();

    assert!(out.created);
    let doc = out
        .document
        .expect("a body was supplied, so it was written");
    assert_eq!(doc.version, 1);
    assert_eq!(doc.body, "The store is one SQLite file.");
    assert_eq!(
        doc.session_id.as_deref(),
        Some("ses_composite"),
        "the revision carries the caller's provenance"
    );

    // And the returned entity already reflects the header the transaction
    // advanced, so a caller does not have to re-read to learn its own result.
    let current = store.revision(out.entity.id(), None).unwrap().unwrap();
    assert_eq!(current.version, 1);
}

#[test]
fn a_design_carries_its_image_without_a_second_write() {
    let (_d, mut store, project) = fixture();

    let out = store
        .create_with_document(
            Design::new(project.clone(), "The board, empty").into(),
            Some("What the board looks like with nothing in it.".to_owned()),
            Some((PNG.to_vec(), "image/png".to_owned())),
            &prov(),
        )
        .unwrap();

    let blob_id = out.blob_id.clone().expect("an image was supplied");
    let blob = store
        .get_blob(&blob_id)
        .unwrap()
        .expect("the blob is stored");
    assert_eq!(blob.bytes, PNG);
    assert_eq!(blob.entity_id.as_ref(), Some(out.entity.id()));

    match &out.entity {
        keel_core::Entity::Design(d) => assert_eq!(
            d.blob_id.as_ref(),
            Some(&blob_id),
            "the row should name the blob it owns"
        ),
        other => panic!("expected a design, got {other:?}"),
    }

    assert_eq!(
        out.entity.audit().version,
        1,
        "the blob pointer used to cost a second update() and a version bump; \
         minting the id before the insert removes both"
    );
}

/// The whole point. Denying the revision must take the entity with it.
#[test]
fn an_entity_whose_body_cannot_be_written_is_not_created() {
    let (_d, mut store, project) = fixture();

    let fault = faults::deny_insert_after(store.connection(), "documents", 1);
    let attempt = store.create_with_document(
        Spec::new(project.clone(), "A spec with no body").into(),
        Some("Prose that will not land.".to_owned()),
        None,
        &prov(),
    );
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);

    let specs: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM specs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        specs, 0,
        "a spec landed with no body — the exact half-write this method exists to prevent"
    );
}

#[test]
fn an_entity_whose_image_cannot_be_written_is_not_created() {
    let (_d, mut store, project) = fixture();

    let fault = faults::deny_insert_after(store.connection(), "blobs", 1);
    let attempt = store.create_with_document(
        Design::new(project.clone(), "A design with no image").into(),
        None,
        Some((PNG.to_vec(), "image/png".to_owned())),
        &prov(),
    );
    faults::clear(store.connection());

    assert!(attempt.is_err());
    fault.assert_fired(1);

    let designs: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM design_artifacts", [], |r| r.get(0))
        .unwrap();
    assert_eq!(designs, 0, "the design survived without its image");
}

/// A retry must not overwrite prose that is already there.
#[test]
fn a_repeat_create_returns_the_existing_row_and_writes_nothing() {
    let (_d, mut store, project) = fixture();

    let first = store
        .create_with_document(
            Spec::new(project.clone(), "Storage specification").into(),
            Some("The original wording.".to_owned()),
            None,
            &prov(),
        )
        .unwrap();

    let again = store
        .create_with_document(
            Spec::new(project.clone(), "Storage specification").into(),
            Some("A wording that must not replace the first.".to_owned()),
            None,
            &prov(),
        )
        .unwrap();

    assert!(!again.created);
    assert_eq!(again.entity.id(), first.entity.id());
    assert!(
        again.document.is_none(),
        "a repeat create reports no revision, because it wrote none"
    );
    let current = store.revision(first.entity.id(), None).unwrap().unwrap();
    assert_eq!(
        current.body, "The original wording.",
        "a retry silently overwrote the body it found"
    );
}

/// The eight types with no prose accept a body without erroring — the caller
/// has usually folded it into a column by then — but nothing is written.
#[test]
fn a_task_with_a_body_gets_a_row_and_no_revision() {
    let (_d, mut store, project) = fixture();

    let out = store
        .create_with_document(
            Task::new(
                project.clone(),
                "Wire the daemon",
                "A summary that is required.",
            )
            .into(),
            Some("Detail that lives in the row.".to_owned()),
            None,
            &prov(),
        )
        .unwrap();

    assert!(out.created);
    assert!(out.document.is_none());
    assert!(store.revision(out.entity.id(), None).unwrap().is_none());
}

/// An image on a type with nowhere to put it is refused, not dropped.
#[test]
fn an_image_on_a_type_that_cannot_hold_one_is_refused() {
    let (_d, mut store, project) = fixture();

    let err = store
        .create_with_document(
            Task::new(project.clone(), "A task", "A summary.").into(),
            None,
            Some((PNG.to_vec(), "image/png".to_owned())),
            &prov(),
        )
        .unwrap_err();

    assert!(
        err.to_string().contains("does not hold an image"),
        "expected a message naming the problem, got: {err}"
    );
    let tasks: i64 = store
        .connection()
        .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        tasks, 0,
        "the refusal must happen before anything is written"
    );
}
