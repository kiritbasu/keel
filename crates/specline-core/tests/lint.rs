//! What `specline lint` finds in a real store.
//!
//! The heuristic itself is unit-tested next to the code. This is about the scan:
//! that it reads the rows rather than the documents, that it never changes one,
//! and that the counts it reports describe the project rather than the page.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Close, CloseReason, Entity, EntityId, EntityStore, Project, Provenance, Store, Task,
    lint, lint::CLOSED_WITHOUT_REASON, lint::TASK_WITHOUT_SUMMARY, lint::UNEXPANDED_IDENTIFIER,
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

impl Fixture {
    fn task(&mut self, title: &str, summary: &str, body: Option<&str>) -> EntityId {
        let mut t = Task::new(self.project.clone(), title, summary);
        t.body = body.map(str::to_owned);
        self.store
            .create(t.into(), &Provenance::anonymous(Actor::Human))
            .unwrap()
            .entity
            .id()
            .clone()
    }

    /// A row in the state the rules would now refuse: no summary at all.
    ///
    /// Created through the store with the field cleared afterwards, because the
    /// create path refuses it — which is the whole reason this lint exists.
    fn task_without_summary(&mut self, title: &str) -> EntityId {
        let id = self.task(title, "A summary that is about to be removed.", None);
        let mut changes = serde_json::Map::new();
        changes.insert("summary".to_owned(), serde_json::json!(null));
        let version = match self.store.get(&id).unwrap() {
            Some(Entity::Task(t)) => t.audit.version,
            _ => panic!("the task exists"),
        };
        self.store
            .update(&id, version, &changes, &Provenance::anonymous(Actor::Human))
            .unwrap();
        id
    }

    /// Apply a hand-built change, the way `keel_update` does.
    fn set(&mut self, id: &EntityId, changes: serde_json::Value) {
        let map = changes.as_object().unwrap().clone();
        let version = match self.store.get(id).unwrap() {
            Some(Entity::Task(t)) => t.audit.version,
            _ => panic!("the task exists"),
        };
        self.store
            .update(id, version, &map, &Provenance::anonymous(Actor::Human))
            .unwrap();
    }

    /// Close a row the way the rules require, so a test can then take something
    /// away and see what the lint says about what is left.
    fn close_properly(&mut self, id: &EntityId) {
        self.set(
            id,
            serde_json::json!({
                "status": "done",
                "close_reason": "done",
                "close_message": "Finished, with a reason.",
                "evidence": ["commit:abc1234"],
            }),
        );
    }

    fn report(&self, limit: Option<usize>) -> specline_core::LintReport {
        lint(&self.store, &self.project, limit).unwrap()
    }
}

#[test]
fn a_row_with_no_summary_is_reported_by_its_readable_identifier() {
    let mut f = setup();
    f.task_without_summary("Cargo workspace scaffold");

    let report = f.report(None);
    assert_eq!(report.count_of(TASK_WITHOUT_SUMMARY), 1);
    assert!(
        report.findings[0].reference.starts_with("DEMO-"),
        "a finding has to be sayable out loud, so it carries the readable \
         identifier rather than a ULID. Got: {}",
        report.findings[0].reference
    );
}

#[test]
fn a_row_that_leans_on_a_bare_identifier_is_reported() {
    let mut f = setup();
    f.task(
        "Rework the activity screen",
        "The feed shows every mutation and none of the rows link anywhere. Done when a row \
         reaches the thing it changed.",
        Some("Waiting on TQ-35."),
    );

    let report = f.report(None);
    assert_eq!(report.count_of(UNEXPANDED_IDENTIFIER), 1);
    assert!(report.findings[0].detail.contains("TQ-35"));
}

#[test]
fn a_row_that_explains_the_identifier_is_left_alone() {
    let mut f = setup();
    f.task(
        "Rework the activity screen",
        "The feed shows every mutation and none of the rows link anywhere. Done when a row \
         reaches the thing it changed.",
        Some(
            "Waiting on TQ-35, which asks whether the screen is rebuilt as a session feed, \
             fixed cheaply, or deleted outright.",
        ),
    );
    assert_eq!(f.report(None).count_of(UNEXPANDED_IDENTIFIER), 0);
}

#[test]
fn a_close_that_predates_the_reason_rule_is_reported() {
    let mut f = setup();
    let old = f.task(
        "Closed in the old world",
        "A row closed before B-47 landed.",
        None,
    );
    // Straight into a terminal status through a hand-built change, the way the
    // hundred and seven historical rows got there.
    f.close_properly(&old);
    assert_eq!(
        f.report(None).count_of(CLOSED_WITHOUT_REASON),
        0,
        "a close that stated its reason is not a finding"
    );

    // And one that never did, which is what the rule is looking for. Built by
    // closing it and stripping the reason, because since KEEL-217 no door into
    // a terminal status accepts a row without one — the same shape as
    // `task_without_summary` above, and for the same reason.
    let bare = f.task(
        "Closed with nothing said",
        "A row that reached done before a reason was required.",
        None,
    );
    f.close_properly(&bare);
    f.set(
        &bare,
        serde_json::json!({"close_reason": null, "close_message": null, "evidence": []}),
    );
    assert_eq!(f.report(None).count_of(CLOSED_WITHOUT_REASON), 1);
}

// Hard constraint 4, and the bug this test exists for: the per-rule counts used
// to be derived from the truncated list, so a report cut at twelve said "12
// task_without_summary" under a total of 240. The counts describe the project.
#[test]
fn a_cut_report_still_counts_the_whole_project() {
    let mut f = setup();
    for n in 0..5 {
        f.task_without_summary(&format!("Row number {n}"));
    }

    let cut = f.report(Some(2));
    assert_eq!(cut.findings.len(), 2);
    assert_eq!(cut.total, 5);
    assert!(cut.truncated);
    assert_eq!(
        cut.count_of(TASK_WITHOUT_SUMMARY),
        5,
        "the per-rule count is what a person reads to decide what to work on, so it has to \
         describe the project rather than the page"
    );
}

// The property the whole design rests on.
#[test]
fn linting_changes_nothing() {
    let mut f = setup();
    let id = f.task_without_summary("Left exactly as it was");
    let before = f.store.get(&id).unwrap();

    f.report(None);
    f.report(None);

    assert_eq!(
        f.store.get(&id).unwrap(),
        before,
        "a lint that rewrote a row would produce the confident, plausible, wrong prose the \
         rule exists to prevent"
    );
    // And no events, which is the other half: a read that writes history is a
    // write.
    let events = f.store.events_for(&id, 50).unwrap();
    assert_eq!(
        events.items.len(),
        2,
        "created and the summary being cleared, and nothing from the lint"
    );
}

#[test]
fn a_clean_project_reports_nothing() {
    let mut f = setup();
    f.task(
        "Show the milestone on every row",
        "The board never says which phase a task is in, so you have to open each one. Done \
         when every row shows it.",
        Some("Chips in the right-hand gutter, dashed when a task has no milestone."),
    );
    let report = f.report(None);
    assert_eq!(report.total, 0);
    assert!(report.by_check().is_empty());
    assert_eq!(report.scanned, 1);
}

/// Closing properly, through the real path, leaves nothing for the lint.
#[test]
fn a_task_closed_the_new_way_is_not_a_finding() {
    let mut f = setup();
    let id = f.task(
        "Finished properly",
        "A row closed through keel_close, with everything the rule asks for.",
        None,
    );
    specline_core::close(
        &mut f.store,
        &id,
        &Close {
            reason: CloseReason::Done,
            message: "Built, tested and committed.".to_owned(),
            evidence: vec!["commit:abc1234".to_owned()],
            other: None,
        },
        &Provenance::anonymous(Actor::Claude).with_session("ses_test"),
    )
    .unwrap();

    assert_eq!(f.report(None).total, 0);
}

/// KEEL-171 closed the door; ten rows were already inside. Six of them are
/// accepted decisions that say what was chosen and nothing about why, which is
/// the one shape the decision log exists to prevent.
///
/// Reported rather than repaired, and that is the point of it being a lint: the
/// missing prose is somebody's reasoning, and a later session writing it would
/// be inferring an argument from the code and presenting it as what a person
/// thought.
#[test]
fn a_prose_bearing_row_with_no_prose_is_reported() {
    let mut f = setup();

    // Through the store rather than the create path, because the create path
    // refuses this now — which is exactly why the historical rows need a lint
    // rather than a rerun.
    let bare = specline_core::Decision::new(f.project.clone(), "Use one parser");
    f.store
        .create(bare.into(), &Provenance::anonymous(Actor::Claude))
        .unwrap();

    let report = f.report(None);
    assert_eq!(
        report.count_of(specline_core::lint::DOCUMENT_WITHOUT_PROSE),
        1
    );
    let finding = report
        .findings
        .iter()
        .find(|x| x.check == specline_core::lint::DOCUMENT_WITHOUT_PROSE)
        .expect("the finding is in the list, not only in the tally");
    assert!(
        finding.detail.contains("nothing says what"),
        "it has to say what is missing rather than name a field: {}",
        finding.detail
    );
}

/// And one that has prose is not a finding, or the rule reports the whole store
/// and gets ignored.
#[test]
fn a_row_that_carries_its_reasoning_is_left_alone() {
    let mut f = setup();

    f.store
        .create_with_document(
            specline_core::Question::new(f.project.clone(), "Cache the digest?").into(),
            Some("Rebuilding it costs 40ms and nothing has complained yet.".to_owned()),
            None,
            &Provenance::anonymous(Actor::Claude),
        )
        .unwrap();

    assert_eq!(
        f.report(None)
            .count_of(specline_core::lint::DOCUMENT_WITHOUT_PROSE),
        0
    );
}
