//! Grouping what changed by the session that changed it.
//!
//! The one thing worth testing hardest is the union. A note leaves no row in
//! `events` (TQ-29), so a per-session account built from the event log alone
//! misses every note — and a note is where a session records what it found,
//! which is the part most worth reading. A test that only ever wrote fields
//! would pass against exactly the version of this that was not worth building.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, ChangeKind, ChangeQuery, Close, CloseReason, EntityId, EntityStore, NewNote, Project,
    Provenance, Store, Task, changes::by_session,
};

struct Fixture {
    store: Store,
    project: EntityId,
    _dir: tempfile::TempDir,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let project = store
        .create(
            Project::new("demo", "Demo").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    Fixture {
        store,
        project,
        _dir: dir,
    }
}

fn session(id: &str) -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session(id)
}

impl Fixture {
    fn task(&mut self, title: &str, prov: &Provenance) -> EntityId {
        self.store
            .create(
                Task::new(self.project.clone(), title, "A row this test needs.").into(),
                prov,
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn note(&mut self, id: &EntityId, body: &str, prov: &Provenance) {
        let mut note = NewNote::new(id.clone(), body, prov.actor);
        if let Some(s) = prov.session_id.clone() {
            note = note.in_session(s);
        }
        self.store.add_note(note, prov).unwrap();
    }

    fn close(&mut self, id: &EntityId, prov: &Provenance) {
        specline_core::work::close(
            &mut self.store,
            id,
            &Close {
                reason: CloseReason::Done,
                message: "Finished it.".to_owned(),
                evidence: vec!["commit:abc1234".to_owned()],
                other: None,
            },
            prov,
        )
        .unwrap();
    }

    fn log(&self, query: ChangeQuery) -> specline_core::ChangeLog {
        by_session(&self.store, &query).unwrap()
    }

    fn scoped(&self) -> ChangeQuery {
        ChangeQuery {
            project_id: Some(self.project.clone()),
            limit: 300,
            ..Default::default()
        }
    }
}

// The point of the whole module.
#[test]
fn a_session_that_wrote_a_note_shows_the_note() {
    let mut f = setup();
    let prov = session("ses_alpha");
    let task = f.task("Something to annotate", &prov);
    f.note(
        &task,
        "Turns out the limiter was fine and the test was not.",
        &prov,
    );

    let log = f.log(f.scoped());
    let group = log
        .sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("ses_alpha"))
        .expect("the session appears");

    assert!(
        group.changes.iter().any(|c| c.kind == ChangeKind::Note),
        "a note writes no event, so the union is the only way it can appear here"
    );
    assert!(
        group.headline.contains("1 note"),
        "the headline names the note, because that is the part a person came for. Got: {}",
        group.headline
    );
}

#[test]
fn two_sessions_are_two_groups_and_the_most_recent_comes_first() {
    let mut f = setup();
    let early = session("ses_early");
    let late = session("ses_late");
    f.task("Done first", &early);
    f.task("Done second", &late);

    let log = f.log(f.scoped());
    let ids: Vec<Option<&str>> = log
        .sessions
        .iter()
        .map(|s| s.session_id.as_deref())
        .collect();
    assert_eq!(
        ids.first().copied().flatten(),
        Some("ses_late"),
        "\"what happened while I was away\" wants the most recently active session at the \
         top. Got: {ids:?}"
    );
    assert!(ids.contains(&Some("ses_early")));
}

#[test]
fn changes_inside_a_session_read_oldest_first() {
    let mut f = setup();
    let prov = session("ses_alpha");
    let first = f.task("First", &prov);
    f.note(&first, "And then this was learned.", &prov);

    let log = f.log(f.scoped());
    let group = &log.sessions[0];
    assert!(group.changes.len() >= 2);
    let times: Vec<_> = group.changes.iter().map(|c| c.at).collect();
    let mut sorted = times.clone();
    sorted.sort();
    assert_eq!(
        times, sorted,
        "the sessions read newest first, but what one session did reads as a sequence"
    );
}

// Writes with no session are a real answer rather than a gap: a bootstrap, a
// migration or a direct call has no conversation behind it.
#[test]
fn writes_with_no_session_are_their_own_group() {
    let mut f = setup();
    f.task("From a migration", &Provenance::anonymous(Actor::System));

    let log = f.log(f.scoped());
    assert!(
        log.sessions.iter().any(|s| s.session_id.is_none()),
        "an untracked write has to appear somewhere, or the feed silently omits it"
    );
}

#[test]
fn a_task_change_carries_its_readable_identifier_so_a_row_can_link() {
    let mut f = setup();
    let prov = session("ses_alpha");
    f.task("Has an identifier", &prov);

    let log = f.log(f.scoped());
    let change = log.sessions[0]
        .changes
        .iter()
        .find(|c| !c.reference.is_empty())
        .expect("a task change carries its reference");
    assert!(
        change.reference.starts_with("DEMO-"),
        "the row links by reference rather than by ULID. Got: {}",
        change.reference
    );
}

#[test]
fn the_actor_filter_narrows_to_one_writer() {
    let mut f = setup();
    f.task("By Claude", &session("ses_claude"));
    f.task("By a person", &Provenance::anonymous(Actor::Human));

    let log = f.log(ChangeQuery {
        actor: Some(Actor::Human),
        ..f.scoped()
    });
    assert!(
        log.sessions
            .iter()
            .all(|s| s.actor == Actor::Human || s.changes.is_empty()),
        "the filter is on who wrote, not on which session"
    );
    assert!(log.changes > 0);
}

// Failure case: a range that excludes everything must come back empty rather
// than falling back to the whole log, which would make "today" silently mean
// "everything" on a quiet day.
#[test]
fn a_range_in_the_future_returns_nothing() {
    let mut f = setup();
    f.task("Written now", &session("ses_alpha"));

    let log = f.log(ChangeQuery {
        since: Some(chrono::Utc::now() + chrono::TimeDelta::days(1)),
        ..f.scoped()
    });
    assert!(log.sessions.is_empty());
    assert_eq!(log.changes, 0);
}

// Hard constraint 4. The limit counts *changes*, because that is what grows
// without bound — one session can be four hundred of them.
#[test]
fn a_cut_log_says_so() {
    let mut f = setup();
    let prov = session("ses_alpha");
    for n in 0..6 {
        f.task(&format!("Row {n}"), &prov);
    }

    let cut = f.log(ChangeQuery {
        limit: 3,
        ..f.scoped()
    });
    assert_eq!(cut.changes, 3);
    assert!(cut.truncated);

    let whole = f.log(ChangeQuery {
        limit: 500,
        ..f.scoped()
    });
    assert!(!whole.truncated);
}

#[test]
fn a_retracted_note_is_not_something_a_session_did() {
    let mut f = setup();
    let prov = session("ses_alpha");
    let task = f.task("Annotated then corrected", &prov);
    f.note(&task, "A finding that turned out to be wrong.", &prov);

    let notes = f.store.notes_for(&task, false).unwrap();
    let id = notes[0].id.clone();
    f.store.retract_note(&id, &prov).unwrap();

    let log = f.log(f.scoped());
    assert!(
        !log.sessions
            .iter()
            .flat_map(|s| &s.changes)
            .any(|c| c.kind == ChangeKind::Note),
        "a withdrawn note stays readable on its row as a record of what was believed, but \
         it is not something to catch up on"
    );
}

/// The headline exists to say what a session did. A close is the thing a person
/// coming back to the machine looks for first, so it leads and it is named.
#[test]
fn a_closed_task_is_named_in_the_headline() {
    let mut f = setup();
    let prov = session("ses_closer");
    let task = f.task("Something to finish", &prov);
    f.close(&task, &prov);

    let log = f.log(f.scoped());
    let group = log
        .sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("ses_closer"))
        .expect("the session appears");

    assert!(
        group.headline.starts_with("closed "),
        "a close leads the headline. Got: {}",
        group.headline
    );
    assert!(
        group.headline.contains("KEEL-") || group.headline.contains("1 task"),
        "the close is named, not counted as an anonymous write. Got: {}",
        group.headline
    );
}

/// The defect this replaced: one close writes four events and one claim writes
/// three, so counting rows made every session read the same. The headline must
/// not report a close as four of anything.
#[test]
fn one_close_is_one_act_not_the_four_events_it_writes() {
    let mut f = setup();
    let prov = session("ses_once");
    let task = f.task("Closed exactly once", &prov);
    f.close(&task, &prov);

    let group = f
        .log(f.scoped())
        .sessions
        .into_iter()
        .find(|s| s.session_id.as_deref() == Some("ses_once"))
        .expect("the session appears");

    assert!(
        !group.headline.contains("closed 4") && !group.headline.contains("4 tasks"),
        "a close is one act however many fields it writes. Got: {}",
        group.headline
    );
    assert!(
        group.headline.contains("change"),
        "the raw count survives as a suffix, so volume is still visible. Got: {}",
        group.headline
    );
}

/// Creations say what they were. "created 6 things" was the original complaint:
/// thirteen artifact types collapsed into one word.
#[test]
fn creations_are_named_by_type_rather_than_called_things() {
    let mut f = setup();
    let prov = session("ses_maker");
    f.task("One", &prov);
    f.task("Two", &prov);

    let group = f
        .log(f.scoped())
        .sessions
        .into_iter()
        .find(|s| s.session_id.as_deref() == Some("ses_maker"))
        .expect("the session appears");

    assert!(
        group.headline.contains("2 tasks"),
        "the type is named and pluralised. Got: {}",
        group.headline
    );
    assert!(
        !group.headline.contains("thing"),
        "\"things\" is what this replaced. Got: {}",
        group.headline
    );
}

/// The regression this nearly shipped with. A session that only edits rows
/// closes nothing, creates nothing and writes no note — and the headline names
/// acts, so it had nothing to say and said "nothing". A claim is exactly that
/// shape, and calling a session that claimed work idle is worse than the count
/// it replaced.
#[test]
fn a_session_that_only_edited_rows_is_not_called_nothing() {
    let mut f = setup();
    let author = session("ses_author");
    let task = f.task("Created by one session", &author);

    let claimer = session("ses_claimer");
    specline_core::work::claim(&mut f.store, &task, false, &claimer).unwrap();

    let group = f
        .log(f.scoped())
        .sessions
        .into_iter()
        .find(|s| s.session_id.as_deref() == Some("ses_claimer"))
        .expect("the claiming session appears");

    assert!(
        !group.changes.is_empty(),
        "the claim wrote fields, so the session did something"
    );
    assert_ne!(
        group.headline, "nothing",
        "a session that edited rows did not do nothing"
    );
    assert!(
        group.headline.contains("change"),
        "with no act to name, the count is what is left. Got: {}",
        group.headline
    );
}
