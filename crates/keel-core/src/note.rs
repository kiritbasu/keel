//! Notes — the running commentary attached to a row.
//!
//! # Why this exists
//!
//! The tracker was prose for one reason: a task row could hold a title and a
//! status, but not the paragraph that says *what was found while doing it*.
//! `product/STATUS.md` carried fifty of those paragraphs — "the DuckDB FTS
//! index is a snapshot and silently misses rows created after it was built" —
//! and they were the most valuable text in the repository. Rendering the
//! tracker from rows meant losing them, so the tracker stayed prose, so the
//! rows stayed thin. That loop is what this breaks.
//!
//! # Why not a column on `tasks`
//!
//! `Task::body` already exists and is the wrong shape. A body is one value that
//! the next writer overwrites; this is a *stream* — many entries, each from a
//! different session on a different day, accumulating. Squashing them into one
//! string loses who found what and when, which is exactly the provenance the
//! rest of the store is careful about. It would also make two sessions
//! appending on the same day a lost update rather than two notes.
//!
//! # Why not a fourteenth artifact type
//!
//! Because it is not an artifact. `links` and `events` are both tables that no
//! one would call an artifact type, and a note is the same kind of thing: it
//! has no independent existence, no lifecycle of its own, and nothing ever
//! links to one. It hangs off a row and dies with it. The ceiling of thirteen
//! is about how many *kinds of thing a project has* — the number a model has to
//! choose between when deciding where something goes — and this adds nothing to
//! that choice.
//!
//! # Append-only
//!
//! A note is never updated, only added or retracted. The alternative — editable
//! notes — turns the stream back into a body with extra steps, because the
//! natural way to "update" a running log is to rewrite the last entry, and then
//! the history is gone again. Retraction is a soft delete, per the hard
//! constraint that nothing is ever removed.

use crate::{Actor, EntityId, EntityType, Error, Result, Surface};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// One entry in a row's running commentary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// `nte_…`. A ULID, so ordering by id is ordering by time and a note
    /// stream needs no separate sort key.
    pub id: crate::NoteId,
    /// The project the annotated row belongs to.
    pub project_id: Option<EntityId>,
    /// The type of row annotated. Stored rather than derived so that reading a
    /// note never requires resolving its subject first.
    pub entity_type: EntityType,
    /// The row annotated.
    pub entity_id: EntityId,
    /// The note itself. Markdown, because it lands in generated markdown and
    /// in a digest a model reads.
    pub body: String,
    /// Who wrote it.
    pub author: Actor,
    /// The conversation responsible, if supplied. This is what makes a note
    /// traceable back to the session that learned the thing.
    pub session_id: Option<String>,
    /// Where the write came from.
    pub surface: Option<Surface>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
    /// Set when retracted. Retracted notes stay readable — a wrong note is
    /// itself part of the history of what was believed.
    pub archived_at: Option<DateTime<Utc>>,
}

impl Note {
    /// Whether this note still counts as current commentary.
    pub const fn is_live(&self) -> bool {
        self.archived_at.is_none()
    }

    /// The first line, for a one-line summary in a digest or a list.
    ///
    /// Notes are frequently a paragraph; a digest has a token budget and a
    /// tracker line has a width. Both want the opening sentence, and neither
    /// wants to invent its own truncation rule.
    pub fn headline(&self) -> &str {
        self.body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty())
            .unwrap_or("")
    }
}

/// A request to append a note, before the id and timestamp are minted.
///
/// Separate from [`Note`] for the same reason [`crate::NewEvent`] is separate
/// from [`crate::Event`]: nothing outside the store may choose a note's id,
/// because ULID ordering is what makes the stream readable in order.
#[derive(Debug, Clone, PartialEq)]
pub struct NewNote {
    /// The row to annotate.
    pub entity_id: EntityId,
    /// The note body.
    pub body: String,
    /// Who is writing.
    pub author: Actor,
    /// The conversation responsible.
    pub session_id: Option<String>,
    /// Where the write came from.
    pub surface: Option<Surface>,
}

impl NewNote {
    /// A note from an actor, with no session attribution.
    pub fn new(entity_id: EntityId, body: impl Into<String>, author: Actor) -> Self {
        Self {
            entity_id,
            body: body.into(),
            author,
            session_id: None,
            surface: None,
        }
    }

    /// Attribute the note to a conversation.
    #[must_use]
    pub fn in_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Record where the write came from.
    #[must_use]
    pub const fn from_surface(mut self, surface: Surface) -> Self {
        self.surface = Some(surface);
        self
    }

    /// Reject a note that says nothing.
    ///
    /// An empty note is never a legitimate write, and letting one through puts
    /// a blank bullet in a generated tracker that no one can explain the origin
    /// of. The message names the fix because a model reads it.
    pub fn validate(&self) -> Result<()> {
        if self.body.trim().is_empty() {
            return Err(Error::Invalid {
                entity_type: EntityType::Task,
                field: "body".to_owned(),
                problem: "the note body is empty or only whitespace".to_owned(),
                expected: "at least one sentence recording the finding, decision or \
                           observation — a note that records nothing is never a valid write"
                    .to_owned(),
            });
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn note(body: &str) -> Note {
        Note {
            id: crate::NoteId::generate(),
            project_id: None,
            entity_type: EntityType::Task,
            entity_id: EntityId::generate(EntityType::Task),
            body: body.to_owned(),
            author: Actor::Claude,
            session_id: None,
            surface: None,
            created_at: Utc::now(),
            archived_at: None,
        }
    }

    #[test]
    fn headline_is_the_first_non_empty_line() {
        let n = note("\n\n  Found and fixed: the index is a snapshot.\n\nMore detail here.");
        assert_eq!(n.headline(), "Found and fixed: the index is a snapshot.");
    }

    #[test]
    fn headline_of_a_blank_body_is_empty_rather_than_panicking() {
        // Can't arrive through `NewNote::validate`, but a row read back from a
        // store written by an older version must not bring the renderer down.
        assert_eq!(note("   \n  ").headline(), "");
    }

    #[test]
    fn an_empty_note_is_rejected_with_an_actionable_message() {
        let err = NewNote::new(
            EntityId::generate(EntityType::Task),
            "   \n\t ",
            Actor::Claude,
        )
        .validate()
        .expect_err("an all-whitespace body must not be accepted");
        assert!(format!("{err}").contains("records nothing"), "{err}");
    }

    #[test]
    fn a_note_with_content_passes() {
        NewNote::new(
            EntityId::generate(EntityType::Task),
            "The FTS index is a snapshot.",
            Actor::Claude,
        )
        .validate()
        .expect("a one-sentence note is valid");
    }

    #[test]
    fn retraction_is_visible_without_hiding_the_note() {
        let mut n = note("This turned out to be wrong.");
        assert!(n.is_live());
        n.archived_at = Some(Utc::now());
        assert!(!n.is_live());
        assert_eq!(n.headline(), "This turned out to be wrong.");
    }
}
