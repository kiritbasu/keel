//! Near-identical titles must not become two rows.
//!
//! The idempotency key is a hash of the normalised title, so it treats
//! "Validate constituent phases to 0–360 degrees" and "Validate constituent
//! phases to 0–360" as unrelated. Two gate runs produced exactly that pair.
//! Per-session stores hid it; in a shared store they are two rows for one
//! task — UC-8's failure arriving one level below projects.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{
    Actor, DuckStore, EntityId, EntityStore, Project, Provenance, Question, Task,
    types::{SAME_THING_THRESHOLD, same_thing, title_similarity},
};

fn store() -> (DuckStore, EntityId, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DuckStore::open(dir.path()).unwrap();
    let id = store
        .create(
            Project::new("demo", "Demo").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    (store, id, dir)
}

#[test]
fn the_exact_pair_two_gate_runs_produced_becomes_one_row() {
    let (mut store, project, _dir) = store();
    let prov = Provenance::anonymous(Actor::Human);

    let first = store
        .create(
            Task::new(
                project.clone(),
                "Validate constituent phases to 0–360 degrees",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    assert!(first.created);

    let second = store
        .create(
            Task::new(
                project.clone(),
                "Validate constituent phases to 0–360",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    assert!(
        !second.created,
        "the second is the same task said slightly differently"
    );
    assert_eq!(second.entity.id(), first.entity.id());
    assert_eq!(
        second.entity.label(),
        "Validate constituent phases to 0–360 degrees",
        "the first title wins; the caller is told nothing was created"
    );
}

#[test]
fn genuinely_different_work_that_shares_words_stays_separate() {
    // The failure that would be worse than the duplicate: a false merge hides
    // work that was new, and a hidden row is neither visible nor mergeable.
    let (mut store, project, _dir) = store();
    let prov = Provenance::anonymous(Actor::Human);

    let a = store
        .create(
            Task::new(
                project.clone(),
                "Validate constituent phases to 0–360",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    let b = store
        .create(
            Task::new(
                project.clone(),
                "Validate constituent amplitude and speed",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    let c = store
        .create(
            Task::new(
                project.clone(),
                "Guard high_waters against a step of zero",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();

    assert!(b.created && c.created, "three distinct tasks, three rows");
    assert_ne!(a.entity.id(), b.entity.id());
    assert_ne!(b.entity.id(), c.entity.id());
}

#[test]
fn a_near_match_of_a_different_type_is_not_the_same_thing() {
    // "Should we switch to blake3?" as a question and as a task are different
    // artifacts: one records an open unknown, the other records work agreed.
    let (mut store, project, _dir) = store();
    let prov = Provenance::anonymous(Actor::Human);

    store
        .create(
            Question::new(project.clone(), "Switch the content-address hash to blake3").into(),
            &prov,
        )
        .unwrap();
    let task = store
        .create(
            Task::new(
                project.clone(),
                "Switch the content-address hash to blake3",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    assert!(task.created, "matching is scoped to one type");
}

#[test]
fn a_near_match_in_another_project_is_not_the_same_thing() {
    let (mut store, first, _dir) = store();
    let prov = Provenance::anonymous(Actor::Human);
    let second = store
        .create(Project::new("other", "Other").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();

    store
        .create(
            Task::new(
                first,
                "Add a size cap with LRU eviction",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    let elsewhere = store
        .create(
            Task::new(
                second,
                "Add a size cap with LRU eviction",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    assert!(
        elsewhere.created,
        "two projects can legitimately have the same task"
    );
}

#[test]
fn a_substituted_token_is_a_different_thing_however_much_else_matches() {
    // The flaw a test caught in the first version of this. Sixty questions
    // differing by one digit scored 0.875 similarity — fourteen shared tokens
    // out of sixteen — and collapsed into two rows. High overlap says "mostly
    // the same words"; only containment says "one adds nothing the other
    // lacks".
    let a = "Open question number 4 that is deliberately quite long so that sixty of them exceed the budget";
    let b = "Open question number 5 that is deliberately quite long so that sixty of them exceed the budget";
    assert!(
        title_similarity(a, b) > SAME_THING_THRESHOLD,
        "they do overlap heavily — which is exactly why overlap alone is not enough"
    );
    assert!(
        !same_thing(a, b),
        "but a substituted token makes them distinct"
    );

    // And the real pair still merges, because its difference is an addition.
    assert!(same_thing(
        "Validate constituent phases to 0–360",
        "Validate constituent phases to 0–360 degrees"
    ));

    // A short title must not swallow everything it is a prefix of.
    assert!(!same_thing("Fix", "Fix the login page"));
}

#[test]
fn sixty_near_identical_questions_stay_sixty_rows() {
    let (mut store, project, _dir) = store();
    let prov = Provenance::anonymous(Actor::Human);
    for i in 0..60 {
        store
            .create(
                Question::new(
                    project.clone(),
                    format!("Open question number {i} that is deliberately quite long"),
                )
                .into(),
                &prov,
            )
            .unwrap();
    }
    let page = store
        .list(
            &keel_core::EntityQuery::in_project(project)
                .of_type(keel_core::EntityType::Question)
                .limited(200),
        )
        .unwrap();
    assert_eq!(page.items.len(), 60, "one row each, not two");
}

#[test]
fn the_threshold_separates_the_cases_it_has_to() {
    // Pinned so a future tweak to the threshold has to face these explicitly.
    let same = title_similarity(
        "Validate constituent phases to 0–360 degrees",
        "Validate constituent phases to 0–360",
    );
    assert!(
        same >= SAME_THING_THRESHOLD,
        "the real pair, similarity {same}"
    );

    let different = title_similarity(
        "Validate constituent phases to 0–360",
        "Validate constituent amplitude and speed",
    );
    assert!(
        different < SAME_THING_THRESHOLD,
        "sibling validation tasks must stay apart, similarity {different}"
    );

    // Order-insensitive by construction: the same words shuffled score 1.0,
    // because two people describing one task rarely choose the same order.
    assert_eq!(
        title_similarity("peak detector fix", "fix peak detector"),
        1.0
    );
    assert_eq!(title_similarity("", "anything"), 0.0);
}
