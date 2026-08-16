//! What a project calls things.
//!
//! KEEL-116 made `specline_create(type: "phase")` work by adding a fixed list of
//! aliases in the source. That closed §8F's exit criterion and left the general
//! problem open: every project's vocabulary had to be anticipated by whoever
//! wrote that list, and a project saying "incident" for a task or "customer
//! conversation" for feedback was out of luck until someone shipped a binary.
//!
//! This is the general answer, and it needed no new machinery — only wiring up
//! the type that was already there. `term` is one of the thirteen, glossary terms
//! are the one thing `specline_context` never truncates, and the reason they are
//! never truncated is *precisely* so a session uses the project's words. A term
//! can now say which type it is a spelling of, and that declaration is consulted
//! before the built-in list.
//!
//! # Why a column and not the definition
//!
//! A term already has prose saying what it means, and reading the type out of
//! that prose was the obvious shortcut. It does not survive contact with real
//! definitions: "a phase is a milestone with a demo at the end" and "a phase is
//! not a milestone" mention the same word and mean opposite things. A declaration
//! cannot be misread, and the type system makes it one of thirteen.
//!
//! # The rule this lives under
//!
//! **A term declares a spelling, never a concept.** `Term::means` is an
//! `EntityType`, so it is impossible for the glossary to invent a fourteenth
//! type — which is the failure mode KEEL-116's own tests were written to prevent
//! and the one this feature would otherwise reopen at the widest possible point.

use crate::{Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result};

/// Where a word's meaning came from.
///
/// Carried back to the caller because resolution is narrated rather than silent:
/// KEEL-116 established that a session told "you said 'sprint' — in Specline that is
/// a milestone" learns the vocabulary in one round trip, where a silent success
/// teaches it nothing and it guesses the same way next time. Saying *where* the
/// word came from is the same argument one step further: "because this project's
/// glossary says so" is actionable in a way "because Specline accepts it" is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The word is one of the thirteen canonical names.
    Canonical,
    /// A glossary term in this project declared it.
    ProjectGlossary,
    /// A global glossary term declared it.
    GlobalGlossary,
    /// It is the project's own word for a milestone.
    ProjectNoun,
    /// Specline's built-in list of words projects commonly use.
    BuiltIn,
}

impl Source {
    /// How to say it in a sentence a model reads.
    pub const fn because(self) -> &'static str {
        match self {
            Source::Canonical => "that is its name",
            Source::ProjectGlossary => "this project's glossary says so",
            Source::GlobalGlossary => "the glossary says so",
            Source::ProjectNoun => "that is this project's word for it",
            Source::BuiltIn => "Specline recognises that word",
        }
    }
}

/// What a word resolved to, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The type. Always one of the thirteen.
    pub entity_type: EntityType,
    /// The word the caller used, when it was not the canonical name.
    pub from: Option<String>,
    /// Where the resolution came from.
    pub source: Source,
}

/// Turn whatever a caller called something into one of the thirteen types.
///
/// The order is deliberate and each step earns its place:
///
/// 1. **The canonical name**, so nothing can shadow `milestone` with a term
///    called `milestone` that means something else.
/// 2. **This project's glossary**, then the global one. Project-first is Q-4's
///    rule for terms generally, and it applies here for the same reason: a
///    project that defines a word means its own definition.
/// 3. **The project's `milestone_noun`.** Not redundant with the glossary even
///    though creating a project seeds one as a term: the noun is what the
///    *interface* says, and a project whose board reads "Phase 8" should accept
///    "phase" on input whether or not anybody kept the term in step.
/// 4. **The built-in list**, which is where KEEL-116 stopped.
///
/// A word that matches nothing gets the same error it always did, listing the
/// thirteen — because at that point the caller has invented a type, and the
/// thirteen are the answer.
pub fn resolve_type(
    store: &impl EntityStore,
    project: Option<&EntityId>,
    word: &str,
) -> Result<Resolved> {
    let lower = word.trim().to_lowercase();

    if let Ok(canonical) = EntityType::parse(&lower) {
        return Ok(Resolved {
            entity_type: canonical,
            from: None,
            source: Source::Canonical,
        });
    }

    // The glossary, project-first, from one read rather than two.
    if let Some((found, source)) = glossary_lookup(store, project, &lower)? {
        return Ok(Resolved {
            entity_type: found,
            from: Some(lower),
            source,
        });
    }

    // The project's own word for a milestone.
    if let Some(project) = project
        && let Some(Entity::Project(p)) = store.get(project)?
        && p.milestone_noun
            .as_deref()
            .is_some_and(|noun| noun.trim().eq_ignore_ascii_case(&lower))
    {
        return Ok(Resolved {
            entity_type: EntityType::Milestone,
            from: Some(lower),
            source: Source::ProjectNoun,
        });
    }

    // Specline's built-in list, and then the error it already raised.
    let (entity_type, alias) = EntityType::parse_with_alias(word)?;
    Ok(Resolved {
        entity_type,
        from: alias.map(str::to_owned),
        source: if alias.is_some() {
            Source::BuiltIn
        } else {
            Source::Canonical
        },
    })
}

/// The type a glossary term declares, matching the term or any of its aliases.
///
/// Only terms that actually declare one are considered. A glossary is mostly
/// domain vocabulary — "Anchor", "Digest", "Vertex view" — and none of that names
/// a type; treating an ordinary definition as an alias is how "hybrid search"
/// would start creating specs.
///
/// One read for both scopes, then project-first in Rust. `EntityQuery` can say
/// "this project" and "every project" but not "global only", and adding a third
/// state to it for one caller is more than this needs at a few thousand terms.
fn glossary_lookup(
    store: &impl EntityStore,
    project: Option<&EntityId>,
    lower: &str,
) -> Result<Option<(EntityType, Source)>> {
    let page = store.list(
        &EntityQuery::default()
            .of_type(EntityType::Term)
            .limited(5_000),
    )?;

    let names = |term: &crate::Term| {
        term.term.trim().eq_ignore_ascii_case(lower)
            || term
                .aliases
                .iter()
                .any(|a| a.trim().eq_ignore_ascii_case(lower))
    };

    let mut global = None;
    for entity in &page.items {
        let Entity::Term(term) = entity else { continue };
        let Some(means) = term.means else { continue };
        if !names(term) {
            continue;
        }
        match &term.project_id {
            // A project's own definition wins, and wins immediately (Q-4).
            Some(owner) if Some(owner) == project => {
                return Ok(Some((means, Source::ProjectGlossary)));
            }
            // Another project's term never applies here. A word meaning one
            // thing in one project and another elsewhere is exactly what
            // project scoping is for.
            Some(_) => {}
            None => global = global.or(Some(means)),
        }
    }
    Ok(global.map(|means| (means, Source::GlobalGlossary)))
}

/// The glossary term that records a project's word for a milestone.
///
/// Seeded when a project declares a noun, so the word appears in the one part of
/// the digest that is never truncated. That is what turns "the interface says
/// Phase" into "a session is told, in its first call, that this project calls
/// milestones phases".
pub fn milestone_noun_term(project_id: &EntityId, noun: &str) -> crate::Term {
    let mut term = crate::Term::new(
        Some(project_id.clone()),
        noun.trim(),
        format!(
            "What this project calls a milestone. `{}` and `milestone` are the same thing; \
             this is the word to use when talking to a person.",
            noun.trim()
        ),
    );
    term.means = Some(EntityType::Milestone);
    term.aliases = vec![noun.trim().to_lowercase()];
    term
}

/// Refuse a noun that would shadow something.
///
/// A project calling milestones "tasks" would make `specline_create(type: "task")`
/// ambiguous on every call, and the resolution order above hides the problem
/// rather than surfacing it — the canonical name wins, so the project's noun
/// would silently do nothing. Better to refuse it at the point somebody sets it.
pub fn validate_milestone_noun(noun: &str) -> Result<()> {
    let lower = noun.trim().to_lowercase();
    if lower.is_empty() {
        return Err(Error::invalid(
            EntityType::Project,
            "milestone_noun",
            "is empty",
            "the word this project uses for a milestone, such as `Phase`",
        ));
    }
    if let Ok(clash) = EntityType::parse(&lower)
        && clash != EntityType::Milestone
    {
        return Err(Error::invalid(
            EntityType::Project,
            "milestone_noun",
            format!("`{noun}` is already the name of another artifact type"),
            "a word that is not one of the thirteen type names — otherwise every \
             `specline_create` naming that type would be ambiguous, and the canonical name \
             would quietly win",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_noun_that_is_another_type_name_is_refused() {
        assert!(validate_milestone_noun("task").is_err());
        assert!(validate_milestone_noun("Decision").is_err());
        assert!(validate_milestone_noun("  ").is_err());
    }

    #[test]
    fn milestone_itself_is_allowed_because_it_shadows_nothing() {
        assert!(validate_milestone_noun("Milestone").is_ok());
        assert!(validate_milestone_noun("Phase").is_ok());
        assert!(validate_milestone_noun("Cycle").is_ok());
    }

    #[test]
    fn the_seeded_term_declares_a_milestone_and_keeps_the_capitalisation() {
        let term = milestone_noun_term(&EntityId::generate(EntityType::Project), "Phase");
        assert_eq!(term.means, Some(EntityType::Milestone));
        assert_eq!(term.term, "Phase");
        assert!(term.aliases.contains(&"phase".to_owned()));
    }

    #[test]
    fn every_source_can_say_why_in_a_sentence() {
        for source in [
            Source::Canonical,
            Source::ProjectGlossary,
            Source::GlobalGlossary,
            Source::ProjectNoun,
            Source::BuiltIn,
        ] {
            assert!(!source.because().is_empty());
        }
    }
}
