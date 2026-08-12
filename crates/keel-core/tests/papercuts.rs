//! The small correctness fixes from the Phase 11 review.
//!
//! None of these was urgent alone. What they share is a shape: each produces a
//! plausible wrong answer rather than an error, which is the failure this
//! codebase treats as the serious one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::store::EventScope;
use keel_core::*;

fn fixture() -> (Store, EntityId, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("papercuts", "Papercuts").into(),
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    (store, project, dir)
}

/// An id arriving as JSON goes through the same validation as one parsed by
/// hand.
///
/// A derived `Deserialize` on a transparent newtype is a `From<String>` in
/// everything but name, and the type's doc comment says deliberately that no
/// such conversion exists. The symptom was worse than a bad row: `entity_type`
/// falls back to `Artifact` for an unrecognised prefix, so nonsense
/// deserialised happily and then reported itself as a real type.
#[test]
fn an_id_that_is_not_an_id_is_refused_when_deserialised() {
    for rubbish in ["banana", "", "tsk_short", "xyz_01H8XK4RPVBQ2N7DZM9C3FGTWY"] {
        let json = serde_json::to_string(rubbish).unwrap();
        assert!(
            serde_json::from_str::<EntityId>(&json).is_err(),
            "`{rubbish}` should not deserialise into an EntityId"
        );
    }

    let real = EntityId::generate(EntityType::Task);
    let round_tripped: EntityId = serde_json::from_str(&serde_json::to_string(&real).unwrap())
        .expect("a real id still round-trips");
    assert_eq!(round_tripped, real);
    assert_eq!(round_tripped.entity_type(), EntityType::Task);
}

/// The connective ids too — they have the same shape and the same hole.
#[test]
fn a_note_id_that_is_not_a_note_id_is_refused_when_deserialised() {
    assert!(serde_json::from_str::<NoteId>("\"tsk_01H8XK4RPVBQ2N7DZM9C3FGTWY\"").is_err());
    assert!(serde_json::from_str::<NoteId>("\"nonsense\"").is_err());
}

/// Paging across types offsets the merged list, not each table.
///
/// The offset used to go into every per-type query, so page two of a
/// cross-type list skipped that many rows *of each type* — dropping rows nobody
/// had seen and showing rows that belonged several pages later. With one type
/// it was correct, which is why it survived.
#[test]
fn a_cross_type_page_offsets_the_merged_list() {
    let (mut store, project, _dir) = fixture();
    let prov = Provenance::anonymous(Actor::Claude);

    for i in 0..4 {
        store
            .create(
                Task::new(
                    project.clone(),
                    format!("task {i}"),
                    "A row for a paging test.",
                )
                .into(),
                &prov,
            )
            .unwrap();
        store
            .create(
                Spec::new(project.clone(), format!("spec {i}")).into(),
                &prov,
            )
            .unwrap();
    }

    let everything: Vec<String> = store
        .list(
            &EntityQuery::in_project(project.clone())
                .of_types([EntityType::Task, EntityType::Spec])
                .limited(100),
        )
        .unwrap()
        .items
        .iter()
        .map(|e| e.id().to_string())
        .collect();
    assert_eq!(everything.len(), 8);

    // Walk it in pages of three and assert the concatenation is the whole list,
    // in order, with nothing repeated and nothing skipped.
    let mut paged: Vec<String> = Vec::new();
    for page in 0..3 {
        let mut query = EntityQuery::in_project(project.clone())
            .of_types([EntityType::Task, EntityType::Spec])
            .limited(3);
        query.offset = page * 3;
        let items = store.list(&query).unwrap().items;
        paged.extend(items.iter().map(|e| e.id().to_string()));
    }

    assert_eq!(
        paged, everything,
        "paging a cross-type list must visit every row exactly once, in the same order"
    );
}

/// Retracting a note says who did it.
///
/// The provenance argument was accepted and discarded — `let _ = provenance;` —
/// in a store whose entire argument is that every write records who made it.
/// Retraction is the one note operation that takes something out of view, so it
/// is the one most worth being able to attribute.
#[test]
fn retracting_a_note_leaves_an_attributed_trace() {
    let (mut store, project, _dir) = fixture();
    let prov = Provenance::anonymous(Actor::Claude).with_session("ses_papercuts");

    let task = store
        .create(
            Task::new(
                project.clone(),
                "Something to annotate",
                "A row with a note on it.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let note = store
        .add_note(
            NewNote::new(task.clone(), "A finding worth recording.", Actor::Claude),
            &prov,
        )
        .unwrap();

    let before = store
        .recent_events(EventScope::Project(&project), 50)
        .unwrap()
        .items
        .len();
    store.retract_note(&note.id, &prov).unwrap();
    let after = store
        .recent_events(EventScope::Project(&project), 50)
        .unwrap();

    assert_eq!(
        after.items.len(),
        before + 1,
        "a retraction should leave an event behind"
    );
    let event = after
        .items
        .iter()
        .find(|e| {
            e.meta
                .as_ref()
                .and_then(|m| m.get("note_id"))
                .and_then(|v| v.as_str())
                == Some(note.id.as_str())
        })
        .expect("the retraction should be findable by the note it retracted");
    assert_eq!(
        event.entity_id, task,
        "and filed against the annotated row, which is what a reader is looking at"
    );
    assert_eq!(event.session_id.as_deref(), Some("ses_papercuts"));

    // The note itself is retracted, not deleted — soft delete only.
    assert!(
        store
            .notes_for(&task, true)
            .unwrap()
            .iter()
            .any(|n| n.id == note.id && n.archived_at.is_some())
    );
    assert!(store.notes_for(&task, false).unwrap().is_empty());
}
