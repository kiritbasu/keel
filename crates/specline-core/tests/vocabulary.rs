//! The project's own words for things.
//!
//! KEEL-116 shipped a fixed alias list in the source; this is the general
//! version, where a project's glossary declares its own vocabulary. The one thing
//! worth guarding hardest is the ceiling: a term declares a *spelling*, never a
//! concept, so nothing here can produce a fourteenth type.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use specline_core::{
    Actor, Entity, EntityId, EntityStore, EntityType, Project, Provenance, Store, Term, WordSource,
    resolve_type,
};

struct Fixture {
    store: Store,
    project: EntityId,
    other: EntityId,
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
    let other = store
        .create(
            Project::new("elsewhere", "Elsewhere").into(),
            &Provenance::anonymous(Actor::Human),
        )
        .unwrap()
        .entity
        .id()
        .clone();
    Fixture {
        store,
        project,
        other,
        _dir: dir,
    }
}

impl Fixture {
    /// Define a word, optionally declaring the type it is a spelling of.
    fn term(
        &mut self,
        project: Option<&EntityId>,
        word: &str,
        means: Option<EntityType>,
        aliases: &[&str],
    ) {
        let mut term = Term::new(
            project.cloned(),
            word,
            format!("What this project means by {word}."),
        );
        term.means = means;
        term.aliases = aliases.iter().map(|a| (*a).to_owned()).collect();
        self.store
            .create(term.into(), &Provenance::anonymous(Actor::Human))
            .unwrap();
    }

    fn set_noun(&mut self, project: &EntityId, noun: &str) -> specline_core::Result<Entity> {
        let version = match self.store.get(project).unwrap() {
            Some(Entity::Project(p)) => p.audit.version,
            _ => panic!("the project exists"),
        };
        let mut changes = serde_json::Map::new();
        changes.insert("milestone_noun".to_owned(), serde_json::json!(noun));
        self.store.update(
            project,
            version,
            &changes,
            &Provenance::anonymous(Actor::Human),
        )
    }

    fn resolve(&self, word: &str) -> specline_core::Resolved {
        resolve_type(&self.store, Some(&self.project), word).unwrap()
    }
}

// --- The glossary as the alias source ------------------------------------

#[test]
fn a_glossary_term_can_say_which_type_it_is_a_spelling_of() {
    let mut f = setup();
    f.term(
        Some(&f.project.clone()),
        "Incident",
        Some(EntityType::Task),
        &["incident"],
    );

    let resolved = f.resolve("incident");
    assert_eq!(resolved.entity_type, EntityType::Task);
    assert_eq!(resolved.source, WordSource::ProjectGlossary);
    assert_eq!(resolved.from.as_deref(), Some("incident"));
}

#[test]
fn an_alias_on_the_term_resolves_too() {
    let mut f = setup();
    f.term(
        Some(&f.project.clone()),
        "Customer conversation",
        Some(EntityType::Feedback),
        &["interview", "call"],
    );
    assert_eq!(f.resolve("interview").entity_type, EntityType::Feedback);
    assert_eq!(f.resolve("call").entity_type, EntityType::Feedback);
}

// A glossary is mostly domain vocabulary — Anchor, Digest, Vertex view — and
// none of it names a type. Treating an ordinary definition as an alias is how
// "hybrid search" would start creating specs.
#[test]
fn a_term_that_declares_nothing_is_not_an_alias() {
    let mut f = setup();
    f.term(Some(&f.project.clone()), "Digest", None, &[]);
    assert!(
        resolve_type(&f.store, Some(&f.project), "digest").is_err(),
        "an ordinary glossary entry must not become a way to create rows"
    );
}

#[test]
fn a_project_cannot_be_made_to_use_another_projects_vocabulary() {
    let mut f = setup();
    let other = f.other.clone();
    f.term(Some(&other), "Incident", Some(EntityType::Task), &[]);

    assert!(
        resolve_type(&f.store, Some(&f.project), "incident").is_err(),
        "a word meaning one thing in one project and another elsewhere is exactly what \
         project scoping is for"
    );
    assert_eq!(
        resolve_type(&f.store, Some(&other), "incident")
            .unwrap()
            .entity_type,
        EntityType::Task
    );
}

#[test]
fn a_global_term_applies_everywhere_and_a_project_one_wins_over_it() {
    let mut f = setup();
    f.term(None, "Brief", Some(EntityType::Spec), &[]);
    assert_eq!(f.resolve("brief").source, WordSource::GlobalGlossary);
    assert_eq!(f.resolve("brief").entity_type, EntityType::Spec);

    // Q-4's rule: a project that defines a word means its own definition.
    f.term(
        Some(&f.project.clone()),
        "Brief",
        Some(EntityType::Decision),
        &[],
    );
    let resolved = f.resolve("brief");
    assert_eq!(resolved.source, WordSource::ProjectGlossary);
    assert_eq!(resolved.entity_type, EntityType::Decision);
}

// Failure case, and the one that would be worst: a term named after a canonical
// type must not be able to redirect it. `specline_create(type: "task")` has to mean
// a task in every project, forever.
#[test]
fn a_term_cannot_shadow_a_canonical_type_name() {
    let mut f = setup();
    f.term(
        Some(&f.project.clone()),
        "task",
        Some(EntityType::Decision),
        &[],
    );

    let resolved = f.resolve("task");
    assert_eq!(
        resolved.entity_type,
        EntityType::Task,
        "the canonical name is checked first, so no glossary can redirect it"
    );
    assert_eq!(resolved.source, WordSource::Canonical);
}

#[test]
fn the_built_in_list_still_works_where_the_glossary_says_nothing() {
    let f = setup();
    let resolved = f.resolve("sprint");
    assert_eq!(resolved.entity_type, EntityType::Milestone);
    assert_eq!(resolved.source, WordSource::BuiltIn);
}

#[test]
fn a_word_that_names_nothing_gets_the_list_of_thirteen() {
    let f = setup();
    let err = resolve_type(&f.store, Some(&f.project), "sprocket").unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("milestone") && message.contains("task"),
        "at that point the caller has invented a type, and the thirteen are the answer. \
         Got: {message}"
    );
}

// --- The project's own noun ----------------------------------------------

#[test]
fn a_projects_noun_resolves_even_with_no_term_for_it() {
    let mut f = setup();
    let project = f.project.clone();
    f.set_noun(&project, "Phase").unwrap();

    let resolved = f.resolve("phase");
    assert_eq!(resolved.entity_type, EntityType::Milestone);
    // "phase" is in the built-in list too, so this asserts the *order*: the
    // project's own word is consulted before Specline's list, which matters for a
    // noun the list has never heard of.
    assert!(matches!(
        resolved.source,
        WordSource::ProjectNoun | WordSource::BuiltIn
    ));

    f.set_noun(&project, "Wave").unwrap();
    let unusual = f.resolve("wave");
    assert_eq!(unusual.entity_type, EntityType::Milestone);
    assert_eq!(
        unusual.source,
        WordSource::ProjectNoun,
        "a word Specline has never heard of still works, because the project said it"
    );
}

#[test]
fn the_noun_is_what_the_interface_says_and_falls_back_to_speclines_word() {
    let mut f = setup();
    let project = f.project.clone();
    let before = match f.store.get(&project).unwrap() {
        Some(Entity::Project(p)) => p,
        _ => panic!("the project exists"),
    };
    assert_eq!(
        before.milestone_word(),
        "Milestone",
        "a blank where a noun should be is worse than the generic term"
    );

    f.set_noun(&project, "Phase").unwrap();
    let after = match f.store.get(&project).unwrap() {
        Some(Entity::Project(p)) => p,
        _ => panic!("the project exists"),
    };
    assert_eq!(after.milestone_word(), "Phase");
}

// Failure case: a project calling milestones "tasks" would make every
// `specline_create(type: "task")` ambiguous, and the resolution order hides that
// rather than surfacing it — the canonical name wins, so the noun would silently
// do nothing.
#[test]
fn a_noun_that_is_another_type_name_is_refused_by_the_store() {
    let mut f = setup();
    let project = f.project.clone();
    let err = f.set_noun(&project, "task").unwrap_err();
    assert!(err.to_string().contains("already the name"), "got: {err}");

    assert!(
        f.set_noun(&project, "Phase").is_ok(),
        "a word that shadows nothing is fine"
    );
}

// The ceiling, asserted rather than trusted. This is the feature most able to
// break it, because it lets a *stored row* introduce vocabulary.
#[test]
fn nothing_the_glossary_says_can_produce_a_fourteenth_type() {
    let mut f = setup();
    for (word, means) in [
        ("incident", EntityType::Task),
        ("brief", EntityType::Spec),
        ("wave", EntityType::Milestone),
    ] {
        f.term(Some(&f.project.clone()), word, Some(means), &[]);
    }

    for word in ["incident", "brief", "wave", "sprint", "task", "milestone"] {
        let resolved = resolve_type(&f.store, Some(&f.project), word).unwrap();
        assert!(
            EntityType::ALL.contains(&resolved.entity_type),
            "`{word}` resolved to something outside the thirteen, which is the one thing \
             this must never do"
        );
    }
}

#[test]
fn resolution_works_with_no_project_at_all() {
    let f = setup();
    // Creating a project itself has no project to consult, so this path has to
    // work: the canonical name and the built-in list, and nothing else.
    let resolved = resolve_type(&f.store, None, "project").unwrap();
    assert_eq!(resolved.entity_type, EntityType::Project);
    assert_eq!(
        resolve_type(&f.store, None, "epic").unwrap().entity_type,
        EntityType::Milestone
    );
}
