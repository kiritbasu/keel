//! Claiming a task and closing one.
//!
//! Both were instructions in `product/CLAUDE.md` before they were mechanisms.
//! "Move a task to `in_progress` before starting it" produced zero transitions
//! into that state across sixty-six tasks; "a task is done only when it meets
//! the definition of done" is a seven-item checklist an agent is asked to
//! honour. These tests are about the difference between asking and enforcing.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{
    Actor, Close, CloseReason, Direction, Entity, EntityId, EntityStore, GraphStore, Project,
    Provenance, Relation, Store, Task, claim, close,
};

struct Fixture {
    store: Store,
    project: EntityId,
    _dir: tempfile::TempDir,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("keel.sqlite")).unwrap();
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

/// Provenance for a named conversation, which is what a claim records.
fn session(id: &str) -> Provenance {
    Provenance::anonymous(Actor::Claude).with_session(id)
}

impl Fixture {
    fn task(&mut self, title: &str) -> EntityId {
        self.store
            .create(
                Task::new(
                    self.project.clone(),
                    title,
                    "A row this test needs in the store.",
                )
                .into(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn read(&self, id: &EntityId) -> Task {
        match self.store.get(id).unwrap() {
            Some(Entity::Task(t)) => t,
            _ => panic!("{id} is a task"),
        }
    }

    /// Set fields directly, as `keel_update` does. The point of several tests
    /// below is that this path is held to the same rule as `close`.
    fn update(&mut self, id: &EntityId, changes: serde_json::Value) -> keel_core::Result<Entity> {
        let version = self.read(id).audit.version;
        let map = changes.as_object().unwrap().clone();
        self.store
            .update(id, version, &map, &Provenance::anonymous(Actor::Human))
    }

    /// Backdate a claim so it reads as abandoned, without waiting three days.
    fn backdate_claim(&mut self, id: &EntityId, days: i64) {
        let when = chrono::Utc::now() - chrono::TimeDelta::days(days);
        self.update(id, serde_json::json!({"claimed_at": when}))
            .unwrap();
    }
}

// --- Claiming ------------------------------------------------------------

#[test]
fn claiming_sets_the_status_and_records_who_and_when() {
    let mut f = setup();
    let t = f.task("Write the ready command");

    let claimed = claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();
    assert_eq!(claimed.task.status, keel_core::TaskStatus::InProgress);
    assert_eq!(claimed.task.claimed_by.as_deref(), Some("ses_alpha"));
    assert!(
        claimed.task.claimed_at.is_some(),
        "the time is what makes a claim releasable — without it a dead session holds work \
         for ever"
    );
    assert!(claimed.took_over_from.is_none());
}

// Failure case, and the one this task exists for.
#[test]
fn a_second_session_cannot_take_a_live_claim_and_is_told_who_has_it() {
    let mut f = setup();
    let t = f.task("Only one of us can do this");
    claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();

    let err = claim(&mut f.store, &t, false, &session("ses_beta")).unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("ses_alpha"),
        "the refusal names the session holding it, so the caller knows who to ask. Got: \
         {message}"
    );
    assert_eq!(
        f.read(&t).claimed_by.as_deref(),
        Some("ses_alpha"),
        "a refused claim changes nothing"
    );
}

#[test]
fn re_claiming_your_own_task_is_a_no_op_rather_than_an_error() {
    let mut f = setup();
    let t = f.task("Mine already");
    let first = claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();
    let again = claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();

    assert_eq!(
        first.task.audit.version, again.task.audit.version,
        "a session that retries should not be punished for it, and should not write a \
         second event either"
    );
}

#[test]
fn a_stale_claim_can_be_taken_and_the_takeover_is_reported() {
    let mut f = setup();
    let t = f.task("Abandoned three days ago");
    claim(&mut f.store, &t, false, &session("ses_gone")).unwrap();
    f.backdate_claim(&t, 4);

    let taken = claim(&mut f.store, &t, false, &session("ses_new")).unwrap();
    assert_eq!(taken.task.claimed_by.as_deref(), Some("ses_new"));
    assert_eq!(
        taken.took_over_from.as_deref(),
        Some("ses_gone"),
        "reported rather than silently overwritten — the other session may be slow rather \
         than dead"
    );
}

#[test]
fn force_takes_a_live_claim_and_says_so() {
    let mut f = setup();
    let t = f.task("Taken over deliberately");
    claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();

    let taken = claim(&mut f.store, &t, true, &session("ses_beta")).unwrap();
    assert_eq!(taken.task.claimed_by.as_deref(), Some("ses_beta"));
    assert_eq!(taken.took_over_from.as_deref(), Some("ses_alpha"));
}

// Failure case: everywhere else an anonymous write is merely less traceable.
// Here the session is the content.
#[test]
fn a_claim_with_no_session_is_refused() {
    let mut f = setup();
    let t = f.task("Claimed by nobody");

    let err = claim(
        &mut f.store,
        &t,
        false,
        &Provenance::anonymous(Actor::Claude),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("session"),
        "a claim naming nobody would hide the task from `ready --unclaimed` while telling \
         no one who to ask. Got: {err}"
    );
}

#[test]
fn a_closed_task_cannot_be_claimed() {
    let mut f = setup();
    let t = f.task("Already finished");
    close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::Done,
            message: "Shipped in the first pass.".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap();

    let err = claim(&mut f.store, &t, false, &session("ses_beta")).unwrap_err();
    assert!(err.to_string().contains("closed"), "got: {err}");
}

// --- Closing -------------------------------------------------------------

#[test]
fn closing_as_done_records_the_reason_the_message_and_the_evidence() {
    let mut f = setup();
    let t = f.task("Gets finished properly");

    let closed = close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::Done,
            message: "The ready command now reads the same ranking as the digest.".to_owned(),
            evidence: vec![
                "commit:0f1e2d3".to_owned(),
                "test:cargo test -p keel-core".to_owned(),
            ],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap();

    assert_eq!(closed.task.status, keel_core::TaskStatus::Done);
    assert_eq!(closed.task.close_reason, Some(CloseReason::Done));
    assert_eq!(closed.task.evidence.len(), 2);
    assert!(closed.task.closed_at.is_some());
    assert!(closed.linked.is_none());
}

#[test]
fn closing_releases_the_claim() {
    let mut f = setup();
    let t = f.task("Claimed, then finished");
    claim(&mut f.store, &t, false, &session("ses_alpha")).unwrap();

    close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::Done,
            message: "Done, and the claim should go with it.".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap();

    let after = f.read(&t);
    assert!(
        after.claimed_by.is_none() && after.claimed_at.is_none(),
        "a finished task is not being worked on, and `ready --unclaimed` would keep \
         skipping it"
    );
}

// Failure case: the whole point of the task. A status is a colour; a reason,
// a message and a piece of evidence are information.
#[test]
fn a_bare_status_change_to_done_is_refused_on_every_path() {
    let mut f = setup();
    let t = f.task("Cannot be quietly finished");

    let err = f
        .update(&t, serde_json::json!({"status": "done"}))
        .unwrap_err();
    assert!(
        err.to_string().contains("why"),
        "the storage layer refuses it, so the CLI and MCP cannot diverge on this. Got: \
         {err}"
    );
    assert_eq!(
        f.read(&t).status,
        keel_core::TaskStatus::Todo,
        "a refused close leaves the row alone"
    );

    // `wont_do` is the same rule. It was worth asserting separately: it is the
    // status a session reaches for when abandoning something, which is exactly
    // when the reason matters most.
    assert!(
        f.update(&t, serde_json::json!({"status": "wont_do"}))
            .is_err()
    );
}

#[test]
fn done_needs_evidence_and_wont_do_does_not() {
    let mut f = setup();
    let a = f.task("Finished with nothing to show");
    let err = close(
        &mut f.store,
        &a,
        &Close {
            reason: CloseReason::Done,
            message: "Trust me, it is done.".to_owned(),
            evidence: vec![],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("evidence"), "got: {err}");

    // A task nobody did has nothing to show, and demanding a commit for "we
    // decided not to" would teach people to invent one.
    let b = f.task("Deliberately not doing this");
    close(
        &mut f.store,
        &b,
        &Close {
            reason: CloseReason::WontDo,
            message: "KB's call: app filing is declined, so the holding pen has nothing to \
                      hold."
                .to_owned(),
            evidence: vec![],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap();
    assert_eq!(f.read(&b).status, keel_core::TaskStatus::WontDo);
}

#[test]
fn a_close_with_no_message_is_refused() {
    let mut f = setup();
    let t = f.task("Closed without a word");
    let err = close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::Done,
            message: "   ".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("message"), "got: {err}");
}

#[test]
fn evidence_has_to_say_what_kind_it_is() {
    let mut f = setup();
    let t = f.task("Closed with a bare sha");

    for bad in ["0f1e2d3", "sha:0f1e2d3", "commit:"] {
        let err = close(
            &mut f.store,
            &t,
            &Close {
                reason: CloseReason::Done,
                message: "Finished, with evidence of the wrong shape.".to_owned(),
                evidence: vec![bad.to_owned()],
                other: None,
            },
            &session("ses_alpha"),
        )
        .unwrap_err();
        assert!(
            err.to_string().contains("evidence"),
            "`{bad}` should be refused: the prefix is what makes \"what shipped this week, \
             with the commits\" a query rather than prose. Got: {err}"
        );
    }
}

#[test]
fn closing_as_a_duplicate_draws_the_edge_in_the_right_direction() {
    let mut f = setup();
    let keeper = f.task("The one that keeps the history");
    let dupe = f.task("Filed twice");

    let closed = close(
        &mut f.store,
        &dupe,
        &Close {
            reason: CloseReason::Duplicate,
            message: "Same work as the earlier row, which has the discussion on it.".to_owned(),
            evidence: vec![],
            other: Some(keeper.clone()),
        },
        &session("ses_alpha"),
    )
    .unwrap();

    assert_eq!(closed.task.status, keel_core::TaskStatus::WontDo);
    assert_eq!(closed.task.close_reason, Some(CloseReason::Duplicate));
    assert_eq!(closed.linked, Some((Relation::Duplicates, keeper.clone())));

    // Direction matters and reads left to right: the duplicate duplicates the
    // keeper, so the edge leaves the duplicate. An inverted traversal returns
    // an empty set indistinguishable from "nothing is linked here".
    let outbound = f
        .store
        .neighbours(&dupe, Direction::Outbound, &[Relation::Duplicates], 1)
        .unwrap();
    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].id, keeper);

    let inbound = f
        .store
        .neighbours(&keeper, Direction::Inbound, &[Relation::Duplicates], 1)
        .unwrap();
    assert_eq!(inbound.len(), 1);
    assert_eq!(inbound[0].id, dupe);
}

#[test]
fn superseded_draws_supersedes_and_needs_the_other_task() {
    let mut f = setup();
    let replacement = f.task("Says it better");
    let old = f.task("Replaced by a better-scoped row");

    let err = close(
        &mut f.store,
        &old,
        &Close {
            reason: CloseReason::Superseded,
            message: "Split into something narrower.".to_owned(),
            evidence: vec![],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap_err();
    assert!(
        err.to_string().contains("other"),
        "\"closed as superseded\" with nothing named is the same dead end as no reason at \
         all. Got: {err}"
    );

    let closed = close(
        &mut f.store,
        &old,
        &Close {
            reason: CloseReason::Superseded,
            message: "Split into something narrower.".to_owned(),
            evidence: vec![],
            other: Some(replacement.clone()),
        },
        &session("ses_alpha"),
    )
    .unwrap();
    assert_eq!(
        closed.linked,
        Some((Relation::Supersedes, replacement.clone()))
    );
}

#[test]
fn naming_another_task_for_a_reason_that_does_not_use_one_is_refused() {
    let mut f = setup();
    let other = f.task("Unrelated");
    let t = f.task("Finished, and pointing somewhere for no reason");

    let err = close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::Done,
            message: "Finished.".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: Some(other),
        },
        &session("ses_alpha"),
    )
    .unwrap_err();
    assert!(err.to_string().contains("other"), "got: {err}");
}

#[test]
fn no_change_closes_with_a_message_and_no_evidence() {
    let mut f = setup();
    let t = f.task("Looked at it, nothing to do");

    let closed = close(
        &mut f.store,
        &t,
        &Close {
            reason: CloseReason::NoChange,
            message: "The behaviour was already right; the report described an older build."
                .to_owned(),
            evidence: vec![],
            other: None,
        },
        &session("ses_alpha"),
    )
    .unwrap();
    assert_eq!(closed.task.status, keel_core::TaskStatus::WontDo);
    assert_eq!(closed.task.close_reason, Some(CloseReason::NoChange));
}

// The rows that predate all of this stay reachable, which is the reason the
// check sits on the transition rather than on the table.
#[test]
fn a_task_closed_before_the_rule_existed_can_still_be_edited() {
    let mut f = setup();
    let mut old = Task::new(
        f.project.clone(),
        "Closed in the old world",
        "A row that reached done before a reason was required.",
    );
    old.status = keel_core::TaskStatus::Done;
    let id = f
        .store
        .create(old.into(), &Provenance::anonymous(Actor::Human))
        .unwrap()
        .entity
        .id()
        .clone();

    f.update(&id, serde_json::json!({"priority": "p0"}))
        .expect("a hundred and seven rows are in this state and none of them may be frozen");
    assert!(f.read(&id).close_reason.is_none());
}
