//! The note stream: what it must do, and what it must refuse.
//!
//! Notes exist so the tracker can stop being prose. That makes two properties
//! load-bearing beyond ordinary CRUD:
//!
//! - **Order is time.** A renderer prints a stream in the order it was written
//!   and never sorts it. If ULID ordering did not hold, the tracker would read
//!   as a shuffled set of findings and nobody would trust it.
//! - **Attribution survives.** The reason a note beats a `body` string is that
//!   it remembers which session learned the thing. A note that loses its
//!   `session_id` is a paragraph with extra steps.

#![allow(clippy::unwrap_used, clippy::expect_used)]
use specline_core::{
    Actor, EntityStore, EntityType, NewNote, Project, Provenance, Store, Surface, Task,
};

fn store() -> (tempfile::TempDir, Store, specline_core::EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let prov = Provenance::anonymous(Actor::Human);
    let project = store
        .create(Project::new("specline", "Specline").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

#[test]
fn a_note_round_trips_with_its_attribution_intact() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project.clone(),
                "Hybrid search",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let written = store
        .add_note(
            NewNote::new(
                task.clone(),
                "Found: the DuckDB FTS index is a snapshot and silently misses rows \
                 created after it was built.",
                Actor::Claude,
            )
            .in_session("ses_abc")
            .from_surface(Surface::Code),
            &prov,
        )
        .unwrap();

    let read = store.notes_for(&task, false).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].id, written.id);
    assert_eq!(read[0].session_id.as_deref(), Some("ses_abc"));
    assert_eq!(read[0].surface, Some(Surface::Code));
    assert_eq!(read[0].author, Actor::Claude);
    assert_eq!(read[0].entity_type, EntityType::Task);
    // Denormalised from the subject, so a reader never has to resolve it.
    assert_eq!(read[0].project_id.as_ref(), Some(&project));
    assert!(read[0].body.contains("silently misses rows"));
}

#[test]
fn the_stream_reads_back_in_the_order_it_was_written() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(project, "Restore", "A row this test needs in the store.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    for body in ["first finding", "second finding", "third finding"] {
        store
            .add_note(NewNote::new(task.clone(), body, Actor::Claude), &prov)
            .unwrap();
    }

    let bodies: Vec<String> = store
        .notes_for(&task, false)
        .unwrap()
        .into_iter()
        .map(|n| n.body)
        .collect();
    assert_eq!(bodies, ["first finding", "second finding", "third finding"]);
}

#[test]
fn the_session_falls_back_to_provenance_when_the_note_does_not_carry_one() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude).with_session("ses_from_provenance");
    let task = store
        .create(
            Task::new(project, "Anything", "A row this test needs in the store.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    store
        .add_note(
            NewNote::new(task.clone(), "no session on the note", Actor::Claude),
            &prov,
        )
        .unwrap();

    assert_eq!(
        store.notes_for(&task, false).unwrap()[0]
            .session_id
            .as_deref(),
        Some("ses_from_provenance"),
    );
}

#[test]
fn a_note_on_a_nonexistent_row_is_refused() {
    let (_d, mut store, _project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let ghost = specline_core::EntityId::generate(EntityType::Task);

    let err = store
        .add_note(NewNote::new(ghost, "into the void", Actor::Claude), &prov)
        .expect_err("a note must not outlive the row it annotates");
    // Nothing links to a note, so there is no traversal that would ever
    // surface an orphaned one again. Failing loudly is the only option.
    let text = format!("{err}");
    assert!(text.contains("no row with id"), "{text}");
    // The id must not have prose appended inside it: a model reading
    // "`tsk_… — cannot annotate…`" has been handed a malformed identifier.
    assert!(
        !text.contains("— cannot annotate a row"),
        "the explanation must not live inside the id: {text}"
    );
}

#[test]
fn a_note_on_an_archived_row_is_refused_with_a_usable_message() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project,
                "Dropped work",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    store.archive(&task, 1, &prov).unwrap();

    let err = store
        .add_note(
            NewNote::new(task, "still thinking about this", Actor::Claude),
            &prov,
        )
        .expect_err("an archived row must not accept commentary");
    let text = format!("{err}");
    assert!(text.contains("archived"), "{text}");
    assert!(
        text.contains("Restore it"),
        "message must say what to do: {text}"
    );
}

#[test]
fn an_empty_note_never_reaches_the_store() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(project, "Anything", "A row this test needs in the store.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    store
        .add_note(NewNote::new(task.clone(), "  \n ", Actor::Claude), &prov)
        .expect_err("a blank note would render as a blank bullet nobody can explain");
    assert!(store.notes_for(&task, false).unwrap().is_empty());
}

#[test]
fn retraction_hides_a_note_without_destroying_it() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project,
                "Wrong diagnosis",
                "A row this test needs in the store.",
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
            NewNote::new(task.clone(), "Blamed the FTS index. Wrong.", Actor::Claude),
            &prov,
        )
        .unwrap();

    let retracted = store.retract_note(&note.id, &prov).unwrap();
    assert!(!retracted.is_live());

    // Soft delete only: gone from the default read, still there when asked for.
    assert!(store.notes_for(&task, false).unwrap().is_empty());
    let with_retracted = store.notes_for(&task, true).unwrap();
    assert_eq!(with_retracted.len(), 1);
    assert_eq!(with_retracted[0].body, "Blamed the FTS index. Wrong.");
}

#[test]
fn retracting_the_same_note_twice_is_an_error_rather_than_a_silent_success() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(project, "Anything", "A row this test needs in the store.").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    let note = store
        .add_note(NewNote::new(task, "once", Actor::Claude), &prov)
        .unwrap();

    store.retract_note(&note.id, &prov).unwrap();
    store
        .retract_note(&note.id, &prov)
        .expect_err("a second retraction means the caller believes something false");
}

#[test]
fn a_projects_notes_come_back_in_one_call_across_every_row() {
    // The renderer needs fifty streams at once. Asking row by row is fifty
    // round trips to answer one question.
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let mut tasks = Vec::new();
    for title in ["one", "two", "three"] {
        let id = store
            .create(
                Task::new(
                    project.clone(),
                    title,
                    "A row this test needs in the store.",
                )
                .into(),
                &prov,
            )
            .unwrap()
            .entity
            .id()
            .clone();
        store
            .add_note(
                NewNote::new(id.clone(), format!("note on {title}"), Actor::Claude),
                &prov,
            )
            .unwrap();
        tasks.push(id);
    }

    let all = store.notes_in_project(&project).unwrap();
    assert_eq!(all.len(), 3);
    // Still in write order across rows, because the id is a ULID.
    let bodies: Vec<&str> = all.iter().map(|n| n.body.as_str()).collect();
    assert_eq!(bodies, ["note on one", "note on two", "note on three"]);
}

#[test]
fn a_retracted_note_is_excluded_from_the_project_wide_read() {
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project.clone(),
                "Anything",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    let keep = store
        .add_note(NewNote::new(task.clone(), "keep me", Actor::Claude), &prov)
        .unwrap();
    let drop = store
        .add_note(NewNote::new(task, "retract me", Actor::Claude), &prov)
        .unwrap();
    store.retract_note(&drop.id, &prov).unwrap();

    let live = store.notes_in_project(&project).unwrap();
    assert_eq!(live.len(), 1, "the renderer must not print retracted notes");
    assert_eq!(live[0].id, keep.id);
}

#[test]
fn the_rendered_tracker_is_byte_identical_across_runs() {
    // `generate --check` compares the rendered tracker against the file on
    // disk, stripping only the banner comment. Anything else time-varying in
    // the output makes the check fail on a minute boundary — a red CI that
    // means nothing, which trains people to ignore it.
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project.clone(),
                "Something",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    store
        .add_note(NewNote::new(task, "a finding", Actor::Claude), &prov)
        .unwrap();

    let first = specline_core::render_status::render(&store, &project).unwrap();
    let second = specline_core::render_status::render(&store, &project).unwrap();
    assert_eq!(
        specline_core::generate::strip_banner_public(&first),
        specline_core::generate::strip_banner_public(&second),
        "the tracker body must not vary between renders",
    );
}

#[test]
fn the_rendered_tracker_carries_the_notes() {
    // The whole reason the prose tracker outlived the rows: the rows could not
    // hold the findings. If this ever regresses, the prose comes back.
    let (_d, mut store, project) = store();
    let prov = Provenance::anonymous(Actor::Claude);
    let task = store
        .create(
            Task::new(
                project.clone(),
                "Hybrid search",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();
    store
        .add_note(
            NewNote::new(
                task,
                "Found: the FTS index is a snapshot and misses later rows.",
                Actor::Claude,
            ),
            &prov,
        )
        .unwrap();

    let rendered = specline_core::render_status::render(&store, &project).unwrap();
    assert!(rendered.contains("Hybrid search"));
    assert!(
        rendered.contains("the FTS index is a snapshot"),
        "the note must appear under its task:\n{rendered}"
    );
}
