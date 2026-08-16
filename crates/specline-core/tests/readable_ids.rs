//! `KEEL-42`, and what has to be true for it to mean anything.
//!
//! A readable identifier is only worth having if it is an *identifier*: one
//! reference, one row, forever. Three things can break that and each has a test
//! here — two projects reducing to the same key, two tasks taking the same
//! number, and a number being handed on after the task that held it was
//! archived. The third is the nastiest, because the reference keeps resolving
//! and quietly starts meaning something else.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Entity, EntityId, EntityStore, Project, Provenance, Store, Task,
    types::{MAX_PROJECT_KEY, derive_project_key, parse_readable_ref},
};

fn store() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::open(dir.path().join("keel.sqlite")).unwrap();
    (dir, store)
}

fn prov() -> Provenance {
    Provenance::anonymous(Actor::Claude)
}

fn project(store: &mut Store, slug: &str, name: &str) -> Project {
    match store
        .create(Project::new(slug, name).into(), &prov())
        .unwrap()
        .entity
    {
        Entity::Project(p) => p,
        other => panic!("expected a project, got {}", other.entity_type()),
    }
}

fn task(store: &mut Store, project_id: &EntityId, title: &str) -> Task {
    match store
        .create(
            Task::new(
                project_id.clone(),
                title,
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap()
        .entity
    {
        Entity::Task(t) => t,
        other => panic!("expected a task, got {}", other.entity_type()),
    }
}

// --- Keys ----------------------------------------------------------------

#[test]
fn a_key_is_derived_from_the_slug() {
    assert_eq!(derive_project_key("specline"), "SPEC");
    assert_eq!(derive_project_key("harbour"), "HARB");
    assert_eq!(derive_project_key("keel-web"), "KEEL");
    assert_eq!(derive_project_key("a-b-c-d-e-f"), "ABCD");
}

// Failure case: a slug with nothing usable must not produce an empty key. An
// empty one would render every task as `-42` and would collide with the next
// such project rather than being visibly odd.
#[test]
fn a_slug_with_no_letters_still_yields_a_key() {
    assert_eq!(derive_project_key("---"), "P");
    assert_eq!(derive_project_key(""), "P");
}

#[test]
fn two_projects_that_reduce_to_the_same_letters_get_different_keys() {
    let (_d, mut store) = store();
    let first = project(&mut store, "specline", "Specline");
    let second = project(&mut store, "sp.ec", "Sp Ec");
    assert_eq!(first.key, "SPEC");
    assert_eq!(
        second.key, "SPEC2",
        "a second project reducing to the same letters must not shadow the first"
    );
    assert_ne!(first.key, second.key);
}

#[test]
fn a_supplied_key_is_uppercased_and_still_de_duplicated() {
    let (_d, mut store) = store();
    project(&mut store, "specline", "Specline");

    let mut wanted = Project::new("other", "Other");
    wanted.key = "spec".to_owned();
    let stored = match store.create(wanted.into(), &prov()).unwrap().entity {
        Entity::Project(p) => p,
        other => panic!("expected a project, got {}", other.entity_type()),
    };
    assert_eq!(
        stored.key, "SPEC2",
        "case must not be a way to smuggle in a second project under one key"
    );
}

// The rule used to live in two places — this constant and a `substr` in the
// migration that backfilled keys onto existing rows — and this test existed to
// keep the two in step. The second copy went with the move to one SQLite file:
// that schema is a single initial migration with nothing to backfill, so there
// is no SQL that can drift from the constant.
//
// What is left is worth asserting anyway. The cases above are written as
// literals, so a change to `MAX_PROJECT_KEY` alone would make them fail without
// saying why; this ties the length back to the constant so the failure names
// the cause.
#[test]
fn a_derived_key_is_never_longer_than_the_constant_allows() {
    for slug in ["specline", "harbour", "keel-web", "a-b-c-d-e-f", "---", ""] {
        let key = derive_project_key(slug);
        assert!(
            !key.is_empty() && key.len() <= MAX_PROJECT_KEY,
            "`{slug}` derived `{key}`, which is not 1..={MAX_PROJECT_KEY} characters"
        );
    }
    assert_eq!(
        derive_project_key("harbour").len(),
        MAX_PROJECT_KEY,
        "a slug with room to spare should use the whole allowance"
    );
}

// --- Numbers -------------------------------------------------------------

#[test]
fn numbers_start_at_one_and_ascend_within_a_project() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    assert_eq!(task(&mut store, &p, "First").number, 1);
    assert_eq!(task(&mut store, &p, "Second").number, 2);
    assert_eq!(task(&mut store, &p, "Third").number, 3);
}

#[test]
fn each_project_counts_for_itself() {
    let (_d, mut store) = store();
    let a = project(&mut store, "specline", "Specline").id;
    let b = project(&mut store, "harbour", "Harbour").id;
    assert_eq!(task(&mut store, &a, "First").number, 1);
    assert_eq!(task(&mut store, &b, "First there too").number, 1);
    assert_eq!(task(&mut store, &a, "Second").number, 2);
}

// The nastiest failure this can have. If an archived task's number is handed
// on, `KEEL-1` keeps resolving and quietly starts meaning a different task —
// every note, commit message and conversation that used it is now wrong, and
// nothing anywhere says so.
#[test]
fn an_archived_tasks_number_is_never_handed_on() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    let first = task(&mut store, &p, "Archived later");
    assert_eq!(first.number, 1);

    store
        .archive(&first.id, first.audit.version, &prov())
        .unwrap();

    assert_eq!(
        task(&mut store, &p, "Comes after").number,
        2,
        "the number of an archived task must not be reused"
    );
}

// Failure case: a retry must not consume a number. Idempotency returns the
// existing row, and if the number were assigned first the sequence would grow
// gaps that read as deleted work.
#[test]
fn a_repeated_create_does_not_burn_a_number() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    let first = task(&mut store, &p, "Only once");
    assert_eq!(first.number, 1);

    let again = store
        .create(
            Task::new(
                p.clone(),
                "Only once",
                "A row this test needs in the store.",
            )
            .into(),
            &prov(),
        )
        .unwrap();
    assert!(!again.created);
    assert_eq!(task(&mut store, &p, "Genuinely new").number, 2);
}

// --- Resolution ----------------------------------------------------------

#[test]
fn a_readable_reference_resolves_to_the_task_it_names() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    task(&mut store, &p, "First");
    let second = task(&mut store, &p, "Second");

    assert_eq!(
        store.resolve_ref("SPEC-2").unwrap(),
        Some(second.id.clone())
    );
    assert_eq!(
        store.resolve_ref("spec-2").unwrap(),
        Some(second.id.clone()),
        "a reference typed in a sentence will not be shouted"
    );
    assert_eq!(store.resolve_ref(" SPEC-2 ").unwrap(), Some(second.id));
}

#[test]
fn a_ulid_resolves_to_itself() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    let t = task(&mut store, &p, "First");
    assert_eq!(store.resolve_ref(t.id.as_str()).unwrap(), Some(t.id));
}

// Failure cases. Each of these must resolve to nothing rather than to
// something: a reference that quietly finds the wrong row is worse than one
// that finds none, because the caller acts on it.
#[test]
fn a_reference_that_names_nothing_resolves_to_nothing() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    task(&mut store, &p, "The only task");

    assert_eq!(
        store.resolve_ref("SPEC-99").unwrap(),
        None,
        "no such number"
    );
    assert_eq!(
        store.resolve_ref("NOPE-1").unwrap(),
        None,
        "no such project"
    );
    assert_eq!(
        store.resolve_ref("SPEC-0").unwrap(),
        None,
        "numbers start at 1"
    );
    assert_eq!(store.resolve_ref("SPEC--1").unwrap(), None, "not a number");
    assert_eq!(store.resolve_ref("not a reference").unwrap(), None);
    assert_eq!(store.resolve_ref("").unwrap(), None);
}

#[test]
fn parsing_a_reference_is_strict_about_the_number() {
    assert_eq!(parse_readable_ref("KEEL-42"), Some(("KEEL".to_owned(), 42)));
    assert_eq!(parse_readable_ref("keel-42"), Some(("KEEL".to_owned(), 42)));
    // A trailing character is a typo, and resolving it to KEEL-42 would hand
    // back a real task for a reference the caller did not write.
    assert_eq!(parse_readable_ref("KEEL-42x"), None);
    assert_eq!(parse_readable_ref("KEEL-"), None);
    assert_eq!(parse_readable_ref("-42"), None);
    assert_eq!(parse_readable_ref("KEEL"), None);
}

// --- The number is not the caller's to set -------------------------------

#[test]
fn a_number_cannot_be_changed_by_an_update() {
    let (_d, mut store) = store();
    let p = project(&mut store, "specline", "Specline").id;
    let t = task(&mut store, &p, "Numbered once");

    let mut changes = serde_json::Map::new();
    changes.insert("number".to_owned(), serde_json::json!(99));
    let err = store
        .update(&t.id, t.audit.version, &changes, &prov())
        .unwrap_err();
    assert!(
        err.to_string().contains("number"),
        "the refusal must name the field: {err}"
    );
}
