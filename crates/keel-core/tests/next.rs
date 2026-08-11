//! Ranking what to do next.
//!
//! TQ-16. The failure this replaces was not a crash — it was a digest that
//! answered "what should I do next" with a count. So these tests are mostly
//! about the answer being *specific*: a named task, in a defensible order,
//! with the reason attached.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{
    Actor, Entity, EntityId, EntityStore, NewLink, Project, Provenance, Question, Relation, Store,
    Task, TaskPriority, TaskStatus, next,
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
    fn task(&mut self, title: &str, priority: TaskPriority, status: TaskStatus) -> EntityId {
        let mut t = Task::new(
            self.project.clone(),
            title,
            "A row this test needs in the store.",
        );
        t.priority = priority;
        t.status = status;
        self.store
            .create(t.into(), &Provenance::anonymous(Actor::Human))
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn decision_task(&mut self, title: &str, priority: TaskPriority) -> EntityId {
        let mut t = Task::new(
            self.project.clone(),
            title,
            "A row this test needs in the store.",
        );
        t.priority = priority;
        t.labels = vec![next::DECISION_LABEL.to_owned()];
        self.store
            .create(t.into(), &Provenance::anonymous(Actor::Human))
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn question(&mut self, title: &str) -> EntityId {
        self.store
            .create(
                Question::new(self.project.clone(), title).into(),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap()
            .entity
            .id()
            .clone()
    }

    fn blocks(&mut self, from: &EntityId, to: &EntityId) {
        self.store
            .link(
                NewLink::new(from.clone(), Relation::Blocks, to.clone()),
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap();
    }

    fn rank(&self) -> keel_core::NextUp {
        next::rank(&self.store, &self.project).unwrap()
    }

    fn ready(&self, filter: &keel_core::ReadyFilter) -> keel_core::Ready {
        keel_core::ready(&self.store, &self.project, filter).unwrap()
    }

    /// Finish a task through the close path, which is the only way to reach a
    /// terminal status now that a reason is required.
    fn close(&mut self, id: &EntityId) {
        let Some(Entity::Task(t)) = self.store.get(id).unwrap() else {
            panic!("{id} is a task")
        };
        keel_core::close(
            &mut self.store,
            &t.id,
            &keel_core::Close {
                reason: keel_core::CloseReason::Done,
                message: "Finished, so that what it was blocking becomes ready.".to_owned(),
                evidence: vec!["test:cargo test -p keel-core --test next".to_owned()],
                other: None,
            },
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();
    }

    fn claim_as(&mut self, id: &EntityId, session: &str) {
        keel_core::claim(
            &mut self.store,
            id,
            false,
            &Provenance::anonymous(Actor::Claude).with_session(session),
        )
        .unwrap();
    }

    fn set_parent(&mut self, child: &EntityId, parent: &EntityId) {
        let Some(Entity::Task(t)) = self.store.get(child).unwrap() else {
            panic!("{child} is a task")
        };
        let mut changes = serde_json::Map::new();
        changes.insert("parent_id".to_owned(), serde_json::json!(parent.as_str()));
        self.store
            .update(
                child,
                t.audit.version,
                &changes,
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap();
    }

    fn label(&mut self, id: &EntityId, labels: &[&str]) {
        let Some(Entity::Task(t)) = self.store.get(id).unwrap() else {
            panic!("{id} is a task")
        };
        let mut changes = serde_json::Map::new();
        changes.insert("labels".to_owned(), serde_json::json!(labels));
        self.store
            .update(
                id,
                t.audit.version,
                &changes,
                &Provenance::anonymous(Actor::Human),
            )
            .unwrap();
    }
}

#[test]
fn the_answer_names_a_task_rather_than_counting_them() {
    let mut f = setup();
    f.task("Write the parser", TaskPriority::P1, TaskStatus::Todo);

    let up = f.rank();
    assert_eq!(up.ready.len(), 1);
    assert_eq!(up.ready[0].title, "Write the parser");
    assert!(
        !up.ready[0].why.is_empty(),
        "every candidate carries its reason, so the digest and the app cannot word it differently"
    );
}

#[test]
fn work_that_releases_more_is_ranked_first_even_at_lower_priority() {
    let mut f = setup();
    // A p0 that releases nothing, against a p1 that releases two.
    let lone = f.task("Tidy the logging", TaskPriority::P0, TaskStatus::Todo);
    let gate = f.task("Run the gate", TaskPriority::P1, TaskStatus::Todo);
    let a = f.task("Downstream A", TaskPriority::P2, TaskStatus::Todo);
    let b = f.task("Downstream B", TaskPriority::P2, TaskStatus::Todo);
    f.blocks(&gate, &a);
    f.blocks(&gate, &b);

    let up = f.rank();
    assert_eq!(
        up.ready[0].id, gate,
        "a task that releases two others goes before a p0 that releases none — that is the \
         whole point of ranking on the graph rather than on the label"
    );
    assert_eq!(up.ready[0].unblocks, 2);
    assert!(up.ready[0].why.contains("unblocks 2"));
    assert!(up.ready.iter().any(|c| c.id == lone));
}

#[test]
fn priority_breaks_the_tie_when_nothing_is_blocked() {
    let mut f = setup();
    f.task("Later", TaskPriority::P3, TaskStatus::Todo);
    let urgent = f.task("Sooner", TaskPriority::P0, TaskStatus::Todo);

    let up = f.rank();
    assert_eq!(up.ready[0].id, urgent);
}

#[test]
fn a_blocked_task_is_never_offered_as_ready_and_names_its_blocker() {
    let mut f = setup();
    let gate = f.task("Run the gate", TaskPriority::P1, TaskStatus::Todo);
    let downstream = f.task("Build the integration", TaskPriority::P0, TaskStatus::Todo);
    f.blocks(&gate, &downstream);

    let up = f.rank();
    assert!(
        !up.ready.iter().any(|c| c.id == downstream),
        "a p0 with something in its way is not ready, however urgent it is"
    );
    let stuck = up.blocked.iter().find(|c| c.id == downstream).unwrap();
    assert!(
        stuck.why.contains("Run the gate"),
        "the blocker is named — the old digest told you to go and look, and the query \
         returned nothing. Got: {}",
        stuck.why
    );
}

#[test]
fn finishing_the_blocker_releases_the_work() {
    let mut f = setup();
    let gate = f.task("Run the gate", TaskPriority::P1, TaskStatus::Todo);
    let downstream = f.task("Build the integration", TaskPriority::P0, TaskStatus::Todo);
    f.blocks(&gate, &downstream);

    f.close(&gate);

    let up = f.rank();
    assert_eq!(
        up.ready.first().map(|c| &c.id),
        Some(&downstream),
        "a finished blocker stops blocking. Leaving the edge in force would freeze work \
         permanently behind something already done"
    );
    assert!(up.blocked.is_empty());
}

#[test]
fn an_unresolved_question_blocks_but_an_answered_one_does_not() {
    let mut f = setup();
    let q = f.question("How do images get in from a chat session?");
    let work = f.task(
        "Design artifacts with stored images",
        TaskPriority::P2,
        TaskStatus::Todo,
    );
    f.blocks(&q, &work);

    assert!(
        f.rank().ready.is_empty(),
        "an open question is a real blocker"
    );

    let Some(Entity::Question(entity)) = f.store.get(&q).unwrap() else {
        panic!("the question exists")
    };
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("answered"));
    f.store
        .update(
            &q,
            entity.audit.version,
            &changes,
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();

    assert_eq!(f.rank().ready.first().map(|c| &c.id), Some(&work));
}

#[test]
fn decisions_waiting_on_a_human_do_not_compete_with_work() {
    let mut f = setup();
    let decide = f.decision_task("Decide whether BM25 lives in DuckDB", TaskPriority::P0);
    let build = f.task("Write the parser", TaskPriority::P2, TaskStatus::Todo);

    let up = f.rank();
    assert_eq!(
        up.ready.iter().map(|c| &c.id).collect::<Vec<_>>(),
        vec![&build],
        "a p0 decision must not outrank real work in the ready list — nobody can start it"
    );
    assert_eq!(up.waiting_on_you.len(), 1);
    assert_eq!(up.waiting_on_you[0].id, decide);
}

#[test]
fn a_task_with_nothing_blocking_it_is_ready() {
    // This used to be the test for a contradiction: a task *marked* blocked
    // with no `blocks` edge, which the ranking reported as its own data
    // problem. Keel's own store was in exactly that state — three such tasks.
    //
    // The status is gone (TQ-25), so the contradiction cannot be written down
    // and there is nothing left to report. What remains to assert is the thing
    // that replaced it: blocked means an edge, and only an edge.
    let mut f = setup();
    let alone = f.task(
        "Deployable daemon with auth",
        TaskPriority::P3,
        TaskStatus::Todo,
    );

    let up = f.rank();
    assert!(
        up.blocked.is_empty(),
        "nothing links to it, so nothing is blocking it"
    );
    assert!(
        up.ready.iter().any(|c| c.id == alone),
        "and it is therefore work someone can pick up"
    );
}

#[test]
fn done_work_is_not_ranked_and_an_empty_project_says_so() {
    let mut f = setup();
    assert!(
        f.rank().is_empty(),
        "a project with no open work has no next action"
    );

    f.task("Already shipped", TaskPriority::P1, TaskStatus::Done);
    f.task("Never doing this", TaskPriority::P1, TaskStatus::WontDo);
    assert!(
        f.rank().is_empty(),
        "closed work must not reappear as something to pick up"
    );
}

// --- keel ready ----------------------------------------------------------
//
// The ranking was good and had no front door: the only way to reach it was
// inside the ~3,500-token digest, so "what should I do next" cost a full
// digest. These tests are about the door and its filters. The order they
// return is `rank`'s, tested above.

#[test]
fn ready_offers_the_children_and_not_the_parent() {
    let mut f = setup();
    let parent = f.task("Make the app legible", TaskPriority::P0, TaskStatus::Todo);
    let child = f.task("Fix the date format", TaskPriority::P2, TaskStatus::Todo);
    f.set_parent(&child, &parent);

    let ready = f.ready(&keel_core::ReadyFilter::default());
    let ids: Vec<_> = ready.items.iter().map(|c| c.id.clone()).collect();
    assert!(
        ids.contains(&child),
        "the child is the work, so it is what gets offered"
    );
    assert!(
        !ids.contains(&parent),
        "a parent is a container for its children — offering both puts one job in the \
         list twice, with the vaguer of the two ranked higher"
    );
}

#[test]
fn unclaimed_hides_what_somebody_is_already_doing() {
    let mut f = setup();
    let mine = f.task("Being worked on", TaskPriority::P0, TaskStatus::Todo);
    let free = f.task("Nobody on it", TaskPriority::P1, TaskStatus::Todo);
    f.claim_as(&mine, "ses_someone_else");

    let all = f.ready(&keel_core::ReadyFilter::default());
    assert_eq!(all.items.len(), 2, "a claim does not stop work being ready");

    let unclaimed = f.ready(&keel_core::ReadyFilter {
        unclaimed: true,
        ..Default::default()
    });
    assert_eq!(unclaimed.items.len(), 1);
    assert_eq!(unclaimed.items[0].id, free);
}

#[test]
fn labels_narrow_the_list_in_both_directions() {
    let mut f = setup();
    let app = f.task("Restyle the rail", TaskPriority::P1, TaskStatus::Todo);
    let store_work = f.task("Add a column", TaskPriority::P1, TaskStatus::Todo);
    f.label(&app, &["desktop", "phase8"]);
    f.label(&store_work, &["storage", "phase8"]);

    let only_desktop = f.ready(&keel_core::ReadyFilter {
        labels: vec!["desktop".to_owned()],
        ..Default::default()
    });
    assert_eq!(only_desktop.items.len(), 1);
    assert_eq!(only_desktop.items[0].id, app);

    let not_desktop = f.ready(&keel_core::ReadyFilter {
        without_labels: vec!["desktop".to_owned()],
        ..Default::default()
    });
    assert_eq!(not_desktop.items.len(), 1);
    assert_eq!(not_desktop.items[0].id, store_work);

    let both_wanted = f.ready(&keel_core::ReadyFilter {
        labels: vec!["desktop".to_owned(), "storage".to_owned()],
        ..Default::default()
    });
    assert!(
        both_wanted.items.is_empty(),
        "labels are required together, not alternatives — otherwise `--label` on two \
         labels would widen the list, which is the opposite of filtering"
    );
}

// Failure case, and hard constraint 4: a list that was cut says so. Ten of ten
// is indistinguishable from ten of ninety otherwise, and a session that reads
// the first is entitled to believe it has seen everything.
#[test]
fn a_limited_list_reports_that_it_was_cut_and_how_much_there_was() {
    let mut f = setup();
    for n in 0..5 {
        f.task(
            &format!("Task number {n}"),
            TaskPriority::P2,
            TaskStatus::Todo,
        );
    }

    let cut = f.ready(&keel_core::ReadyFilter {
        limit: Some(2),
        ..Default::default()
    });
    assert_eq!(cut.items.len(), 2);
    assert_eq!(
        cut.total, 5,
        "the total is what was ready, not what was returned"
    );
    assert!(cut.truncated);

    let whole = f.ready(&keel_core::ReadyFilter {
        limit: Some(5),
        ..Default::default()
    });
    assert!(
        !whole.truncated,
        "a limit that happens to equal the count did not cut anything"
    );
}

#[test]
fn ready_skips_what_is_blocked_and_what_is_waiting_on_a_person() {
    let mut f = setup();
    let gate = f.task("Must happen first", TaskPriority::P2, TaskStatus::Todo);
    let waiting = f.task("Downstream", TaskPriority::P0, TaskStatus::Todo);
    f.blocks(&gate, &waiting);
    let decide = f.decision_task("Decide TQ-40", TaskPriority::P0);

    let ready = f.ready(&keel_core::ReadyFilter::default());
    let ids: Vec<_> = ready.items.iter().map(|c| c.id.clone()).collect();
    assert!(ids.contains(&gate));
    assert!(!ids.contains(&waiting), "something live is in its way");
    assert!(
        !ids.contains(&decide),
        "a decision is not work — nothing can start on it until a person answers"
    );
}
