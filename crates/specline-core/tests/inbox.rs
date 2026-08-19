//! The Inbox: untriaged signals, and what must never count them as work.
//!
//! A signal is something somebody wants. A task is work somebody committed to.
//! Every count in the product answers one question or the other, and the whole
//! reason B-90 separates them is that a number mixing the two cannot answer
//! either — "37 open" is not usable for "how much is left" if four of the
//! thirty-seven are ideas nobody has looked at.
//!
//! So these tests are mostly negative: they assert what the Inbox is *not* in.
//! That is the half that cannot be seen by looking at a screen, and the half
//! that regresses silently, because a signal leaking into the task count looks
//! exactly like a project with one more task.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Direction, EntityId, EntityStore, Feedback, FeedbackKind, GraphStore, NewLink, Project,
    Provenance, Relation, Spec, SpecKind, Store, Task,
    digest::{self, Depth},
};

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session("ses_inbox")
}

fn fixture() -> (tempfile::TempDir, Store, EntityId) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();
    let project = store
        .create(Project::new("specline", "Specline").into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone();
    (dir, store, project)
}

fn file_signal(store: &mut Store, project: &EntityId, said: &str) -> EntityId {
    let mut signal = Feedback::new(project.clone(), said);
    signal.kind = FeedbackKind::Idea;
    store
        .create(signal.into(), &prov())
        .unwrap()
        .entity
        .id()
        .clone()
}

#[test]
fn an_untriaged_signal_is_counted_in_the_inbox() {
    let (_d, mut store, project) = fixture();

    let (none, oldest) = store.untriaged_signals(&project).unwrap();
    assert_eq!(none, 0);
    assert!(
        oldest.is_none(),
        "with nothing waiting there is no oldest thing waiting"
    );

    file_signal(&mut store, &project, "this should work with codex");
    file_signal(&mut store, &project, "search should look inside documents");

    let (count, oldest) = store.untriaged_signals(&project).unwrap();
    assert_eq!(count, 2);
    assert!(
        oldest.is_some(),
        "a non-zero count always has an oldest, or the age can never be reported"
    );
}

/// Triaging is what empties the Inbox, and it is the only thing that does.
/// Nothing here archives or deletes the signal — it stays, with its outcome,
/// because B-91 turns on being able to find out what was decided about it.
#[test]
fn a_triaged_signal_leaves_the_inbox_without_leaving_the_store() {
    let (_d, mut store, project) = fixture();
    let signal = file_signal(&mut store, &project, "this should work with codex");

    let current = store.get(&signal).unwrap().unwrap();
    let mut changes = serde_json::Map::new();
    changes.insert("triaged".to_owned(), serde_json::Value::Bool(true));
    store
        .update(&signal, current.audit().version, &changes, &prov())
        .unwrap();

    let (count, _) = store.untriaged_signals(&project).unwrap();
    assert_eq!(count, 0, "triaging is what takes it out of the Inbox");
    assert!(
        store.get(&signal).unwrap().is_some(),
        "and the signal itself is still there, because the outcome has to stay findable"
    );
}

/// An archived signal is out of the Inbox too — soft delete is still delete as
/// far as a count of outstanding work is concerned, and a signal somebody
/// archived is not waiting on anybody.
#[test]
fn an_archived_signal_is_not_waiting_for_anyone() {
    let (_d, mut store, project) = fixture();
    let signal = file_signal(&mut store, &project, "a thing said once and withdrawn");
    let version = store.get(&signal).unwrap().unwrap().audit().version;

    store.archive(&signal, version, &prov()).unwrap();

    assert_eq!(store.untriaged_signals(&project).unwrap().0, 0);
}

/// The one that matters, and the one that would regress silently. A signal
/// leaking into `open_tasks` looks identical to a project with one more task,
/// so nothing about the output would tell you it had happened.
#[test]
fn a_signal_is_not_a_task_and_is_counted_by_nothing_that_counts_tasks() {
    let (_d, mut store, project) = fixture();

    store
        .create(
            Task::new(
                project.clone(),
                "Wire up the exporter",
                "The exporter writes nothing when the window is empty.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    for said in [
        "this should work with codex",
        "search should look inside documents",
        "the board should group by phase",
    ] {
        file_signal(&mut store, &project, said);
    }

    let (open, _urgent) = store.task_counts(&project).unwrap();
    assert_eq!(
        open, 1,
        "three signals and one task is one open task, not four"
    );

    let ranked = specline_core::next::rank(&store, &project).unwrap();
    let named: Vec<&str> = ranked
        .ready
        .iter()
        .map(|c| c.title.as_str())
        .chain(ranked.waiting_on_you.iter().map(|c| c.title.as_str()))
        .collect();
    assert!(
        !named.iter().any(|l| l.contains("codex")),
        "a signal is not something to pick up next: {named:?}"
    );
}

/// Hard constraint 4, at the digest. The Inbox may be long by design, so the
/// digest carries its size rather than its contents — but carrying neither is
/// the silent omission the constraint exists to forbid, and it is how four
/// signals sat next to the release work with nothing saying so.
#[test]
fn the_digest_says_how_many_signals_are_waiting() {
    let (_d, mut store, project) = fixture();

    let quiet = digest::build(&store, Some(&project), Depth::Standard, None).unwrap();
    assert_eq!(quiet.project.as_ref().unwrap().inbox, 0);
    assert!(
        !quiet.to_prose().contains("inbox:"),
        "an empty Inbox says nothing — a line reading 0 is one every session reads forever to \
         learn nothing"
    );

    file_signal(&mut store, &project, "this should work with codex");
    file_signal(&mut store, &project, "search should look inside documents");

    let built = digest::build(&store, Some(&project), Depth::Standard, None).unwrap();
    let line = built.project.as_ref().unwrap();
    assert_eq!(line.inbox, 2);
    assert_eq!(
        line.inbox_oldest_days,
        Some(0),
        "filed just now, so nothing has been waiting a day"
    );

    let prose = built.to_prose();
    assert!(
        prose.contains("inbox: 2 untriaged signal(s)"),
        "the digest has to say the Inbox is there: {prose}"
    );
    // On its own line, not folded into the task counts, because the whole
    // point is that they are different kinds of number.
    assert!(
        prose.contains("0 open task(s)"),
        "and the task count stays a task count: {prose}"
    );
}

// --- Picking a signal up (KEEL-323) ---------------------------------------

/// A signal that survives triage becomes a **feature spec**, not a task.
///
/// That is the structural call B-90 turns on: the thinking outlives the work.
/// The spec exists whether or not the thing is ever built, which is what a
/// session picking up child task nine reads to learn why the other eight
/// matter — and it is what keeps an unbuilt idea off the board entirely, since
/// the epic task is created only at the moment somebody decides to build.
#[test]
fn a_picked_up_signal_becomes_a_feature_spec_that_remembers_where_it_came_from() {
    let (_d, mut store, project) = fixture();
    let signal = file_signal(&mut store, &project, "this should work with codex");

    let mut spec = Spec::new(project.clone(), "Specline works with OpenAI Codex");
    spec.kind = SpecKind::Feature;
    let feature = store
        .create_with_document(
            spec.into(),
            Some(
                "Madhu asked for this. The open question is whether it means the MCP endpoint \
                 or the whole plugin surface."
                    .to_owned(),
            ),
            None,
            &prov(),
        )
        .unwrap()
        .entity
        .id()
        .clone();

    store
        .link(
            NewLink::new(feature.clone(), Relation::DerivedFrom, signal.clone()),
            &prov(),
        )
        .unwrap();

    // Both directions, because an inverted traversal returns an empty set that
    // is indistinguishable from "nothing is linked here" — which here would
    // mean a feature whose origin is silently unrecoverable.
    let out: Vec<EntityId> = store
        .neighbours(&feature, Direction::Outbound, &[Relation::DerivedFrom], 1)
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert!(
        out.contains(&signal),
        "the feature derives from the signal: {out:?}"
    );

    let inbound: Vec<EntityId> = store
        .neighbours(&signal, Direction::Inbound, &[Relation::DerivedFrom], 1)
        .unwrap()
        .into_iter()
        .map(|n| n.id)
        .collect();
    assert!(
        inbound.contains(&feature),
        "and the signal knows what it became, which is how the loop gets closed \
         back to whoever asked: {inbound:?}"
    );
}

/// `feature` is a real value of the enum and the refusal lists it, because a
/// model reading "`feature` is not valid" without being shown that it is would
/// simply guess something else.
#[test]
fn feature_is_one_of_the_spec_kinds_and_the_refusal_says_so() {
    assert_eq!(SpecKind::parse("feature").unwrap(), SpecKind::Feature);
    let err = SpecKind::parse("epic").unwrap_err().to_string();
    assert!(err.contains("feature"), "{err}");
}
