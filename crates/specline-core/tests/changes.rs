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

// ---------------------------------------------------------------------------
// One row per action (KEEL-300).
//
// The feed showed a field write per row, so one close was four rows and one
// claim three — and two of the close's four could only say how many characters
// had moved. These assert the rows now count acts, and that collapsing them did
// not become an excuse to print what the renderer refuses to.
// ---------------------------------------------------------------------------

/// The four events of a close are one thing a person did.
#[test]
fn a_close_is_one_row_rather_than_four() {
    let mut f = setup();
    let prov = session("ses_close");
    let task = f.task("A task to finish", &prov);
    f.close(&task, &prov);

    let log = f.log(f.scoped());
    let group = log
        .sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("ses_close"))
        .expect("the closing session");
    let fields: Vec<&specline_core::Change> = group
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Field)
        .collect();

    assert_eq!(
        fields.len(),
        1,
        "a close writes status, close_reason, close_message and evidence, and it is one act: {:?}",
        fields.iter().map(|c| &c.summary).collect::<Vec<_>>()
    );
    assert_eq!(fields[0].summary, "closed as done");
    assert_eq!(
        fields[0].field.as_deref(),
        Some("close_reason"),
        "the headline counts closes by this field, so a collapsed row has to keep carrying it"
    );
}

/// A claim is one row, and says so in words rather than in a session id.
#[test]
fn a_claim_is_one_row_and_never_prints_the_session_id() {
    let mut f = setup();
    let prov = session("ses_claim");
    let task = f.task("A task to pick up", &prov);
    specline_core::work::claim(&mut f.store, &task, false, &prov).unwrap();

    let log = f.log(f.scoped());
    let group = log
        .sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("ses_claim"))
        .expect("the claiming session");
    let fields: Vec<&specline_core::Change> = group
        .changes
        .iter()
        .filter(|c| c.kind == ChangeKind::Field)
        .collect();

    assert_eq!(
        fields.len(),
        1,
        "a claim writes three fields and is one act"
    );
    assert_eq!(fields[0].summary, "claimed");
    assert!(
        !fields[0].summary.contains("ses_"),
        "a session id is not something a person reads: {}",
        fields[0].summary
    );
}

/// The failure this whole rendering exists to prevent, through the new path.
///
/// A body was once redacted and then republished into the committed changelog
/// by the very edit that removed it (KEEL-215). Collapsing rows must drop a
/// value the renderer refuses to quote, never take the collapse as licence to
/// quote it — so the close message goes in with a string that must not come
/// back out.
#[test]
fn a_collapsed_close_never_quotes_the_message_it_was_written_with() {
    let mut f = setup();
    let prov = session("ses_secret");
    let task = f.task("A task with a careless close", &prov);
    let secret = "/Users/someone/private/path";
    specline_core::work::close(
        &mut f.store,
        &task,
        &Close {
            reason: CloseReason::Done,
            message: format!("Fixed it, the file was at {secret} which nobody should read."),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &prov,
    )
    .unwrap();

    let log = f.log(f.scoped());
    for group in &log.sessions {
        for change in &group.changes {
            assert!(
                !change.summary.contains(secret),
                "a collapsed row published the close message: {}",
                change.summary
            );
        }
    }
}

/// A lone prose edit names the field rather than measuring it.
#[test]
fn a_body_rewrite_says_which_field_moved_not_how_big_it_was() {
    let mut f = setup();
    let prov = session("ses_body");
    let task = f.task("A task whose body grows", &prov);
    let mut changes = serde_json::Map::new();
    changes.insert(
        "body".to_owned(),
        serde_json::Value::String("x".repeat(1_852)),
    );
    let version = f.store.get(&task).unwrap().unwrap().audit().version;
    f.store.update(&task, version, &changes, &prov).unwrap();

    let log = f.log(f.scoped());
    let summaries: Vec<String> = log
        .sessions
        .iter()
        .flat_map(|s| s.changes.iter())
        .filter(|c| c.kind == ChangeKind::Field)
        .map(|c| c.summary.clone())
        .collect();

    assert!(
        summaries.iter().any(|s| s == "body changed"),
        "expected the field named, got {summaries:?}"
    );
    assert!(
        !summaries.iter().any(|s| s.contains("characters")),
        "a size is not what changed: {summaries:?}"
    );
}

/// Picking up somebody else's task is not the same event as claiming a free
/// one, and the row should not say it is.
#[test]
fn a_takeover_reads_differently_from_a_first_claim() {
    let mut f = setup();
    let first = session("ses_first");
    let second = session("ses_second");
    let task = f.task("A contested task", &first);
    specline_core::work::claim(&mut f.store, &task, false, &first).unwrap();
    specline_core::work::claim(&mut f.store, &task, true, &second).unwrap();

    let log = f.log(f.scoped());
    let taken = log
        .sessions
        .iter()
        .find(|s| s.session_id.as_deref() == Some("ses_second"))
        .expect("the taking session");
    let summaries: Vec<String> = taken.changes.iter().map(|c| c.summary.clone()).collect();

    assert!(
        summaries.iter().any(|s| s == "taken over"),
        "expected a takeover to say so, got {summaries:?}"
    );
}

/// An id is as unreadable as a size, and passes every test the redaction rule
/// applies — it is short, and it is not prose.
#[test]
fn a_row_never_shows_a_bare_identifier() {
    let mut f = setup();
    let prov = session("ses_move");
    let task = f.task("A task that joins a phase", &prov);
    let milestone = f
        .store
        .create(
            specline_core::Milestone::new(f.project.clone(), "Phase 11", "Hardening").into(),
            &prov,
        )
        .unwrap()
        .entity
        .id()
        .clone();

    let mut changes = serde_json::Map::new();
    changes.insert(
        "milestone_id".to_owned(),
        serde_json::Value::String(milestone.to_string()),
    );
    let version = f.store.get(&task).unwrap().unwrap().audit().version;
    f.store.update(&task, version, &changes, &prov).unwrap();

    let summaries: Vec<String> = f
        .log(f.scoped())
        .sessions
        .iter()
        .flat_map(|s| s.changes.iter())
        .map(|c| c.summary.clone())
        .collect();

    assert!(
        !summaries.iter().any(|s| s.contains(&milestone.to_string())),
        "a row published a raw id: {summaries:?}"
    );
    assert!(
        summaries.iter().any(|s| s == "milestone id changed"),
        "expected the field named, got {summaries:?}"
    );
}
