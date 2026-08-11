//! Ranking what to do next.
//!
//! TQ-16. The failure this replaces was not a crash — it was a digest that
//! answered "what should I do next" with a count. So these tests are mostly
//! about the answer being *specific*: a named task, in a defensible order,
//! with the reason attached.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{
    Actor, DuckStore, Entity, EntityId, EntityStore, NewLink, Project, Provenance, Question,
    Relation, Task, TaskPriority, TaskStatus, next,
};

struct Fixture {
    store: DuckStore,
    project: EntityId,
    _dir: tempfile::TempDir,
}

fn setup() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DuckStore::open(dir.path()).unwrap();
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

    let Some(Entity::Task(t)) = f.store.get(&gate).unwrap() else {
        panic!("the gate exists")
    };
    let mut changes = serde_json::Map::new();
    changes.insert("status".to_owned(), serde_json::json!("done"));
    f.store
        .update(
            &gate,
            t.audit.version,
            &changes,
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap();

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
