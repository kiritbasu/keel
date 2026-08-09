//! The append-only mutation log.
//!
//! Every mutation writes one of these (REQ-6). They power the activity feed,
//! the agent's "what changed since I last looked", and the 409 payload's
//! `events_since` — which is the difference between an agent that can merge a
//! concurrent edit and one that has to give up.
//!
//! Events carry no audit block. SPEC §3.1 makes this the one deliberate
//! exception: append-only and immutable means there is no `updated_at`, no
//! `version` and no `archived_at`, because none of them could ever change.
//! Modelling them anyway would invite code that tries to set them.

use crate::{Actor, EntityId, EntityType, Error, EventId, Result, Surface};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// What kind of change an event records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// A new entity appeared.
    Created,
    /// One or more fields changed.
    Updated,
    /// The `status` field specifically changed.
    ///
    /// Separate from `Updated` because status transitions are what the
    /// activity feed and the roadmap actually care about, and filtering
    /// `Updated` by `field = 'status'` after the fact is both slower and
    /// easier to get wrong.
    StatusChanged,
    /// An edge was created or archived.
    Linked,
    /// A new document revision was appended.
    Revised,
    /// The entity was soft-deleted.
    Archived,
}

impl Action {
    /// Every action.
    pub const ALL: [Action; 6] = [
        Action::Created,
        Action::Updated,
        Action::StatusChanged,
        Action::Linked,
        Action::Revised,
        Action::Archived,
    ];

    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Action::Created => "created",
            Action::Updated => "updated",
            Action::StatusChanged => "status_changed",
            Action::Linked => "linked",
            Action::Revised => "revised",
            Action::Archived => "archived",
        }
    }

    /// Parse a stored string.
    pub fn parse(s: &str) -> Result<Self> {
        Action::ALL
            .into_iter()
            .find(|a| a.as_str() == s)
            .ok_or_else(|| Error::MalformedId {
                supplied: s.to_owned(),
                problem: format!("`{s}` is not a known event action"),
                expected: Action::ALL
                    .into_iter()
                    .map(Action::as_str)
                    .collect::<Vec<_>>()
                    .join(" | "),
            })
    }
}

impl fmt::Display for Action {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One record in the mutation log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// `evt_…`. A ULID, so ordering by id *is* ordering by time and "what
    /// changed since T" is a range scan.
    pub id: EventId,
    /// The project affected, if the entity has one.
    pub project_id: Option<EntityId>,
    /// The type of entity affected.
    pub entity_type: EntityType,
    /// The entity affected.
    pub entity_id: EntityId,
    /// What happened.
    pub action: Action,
    /// The field that changed, for single-field updates.
    pub field: Option<String>,
    /// The prior value.
    pub before: Option<serde_json::Value>,
    /// The new value.
    pub after: Option<serde_json::Value>,
    /// Who did it. Always equals the affected row's `updated_by`.
    pub actor: Actor,
    /// The conversation responsible, if supplied.
    pub session_id: Option<String>,
    /// The surface it came from.
    pub surface: Option<Surface>,
    /// A one-line human-readable description, for the activity feed.
    pub summary: String,
    /// Structured extras — e.g. `{"confirmed_by":"human"}` on project creation
    /// (§6.4).
    pub meta: Option<serde_json::Value>,
    /// When it happened.
    pub created_at: DateTime<Utc>,
}

impl Event {
    /// Whether this event is one an agent should be told about when catching
    /// up on a project.
    ///
    /// All of them are, currently. The method exists so that if a noisy class
    /// of event is ever added, filtering happens in one place rather than in
    /// each of `keel_activity`, `keel_context` and the desktop feed.
    pub const fn is_notable(&self) -> bool {
        true
    }
}

/// A request to append an event, before the id and timestamp are minted.
///
/// Separate from [`Event`] so that nothing can construct an event with a
/// caller-chosen id — ULID ordering is what makes the log queryable, and an
/// out-of-order id would corrupt every "since" query silently.
#[derive(Debug, Clone, PartialEq)]
pub struct NewEvent {
    /// The project affected.
    pub project_id: Option<EntityId>,
    /// The entity affected.
    pub entity_id: EntityId,
    /// What happened.
    pub action: Action,
    /// The field that changed, for single-field updates.
    pub field: Option<String>,
    /// The prior value.
    pub before: Option<serde_json::Value>,
    /// The new value.
    pub after: Option<serde_json::Value>,
    /// A one-line description.
    pub summary: String,
    /// Structured extras.
    pub meta: Option<serde_json::Value>,
}

impl NewEvent {
    /// An event with the minimum: what happened, to what, and a summary.
    pub fn new(entity_id: EntityId, action: Action, summary: impl Into<String>) -> Self {
        NewEvent {
            project_id: None,
            entity_id,
            action,
            field: None,
            before: None,
            after: None,
            summary: summary.into(),
            meta: None,
        }
    }

    /// Attach the owning project.
    pub fn in_project(mut self, project_id: Option<EntityId>) -> Self {
        self.project_id = project_id;
        self
    }

    /// Record a single field's transition.
    pub fn field_change(
        mut self,
        field: impl Into<String>,
        before: impl Into<serde_json::Value>,
        after: impl Into<serde_json::Value>,
    ) -> Self {
        self.field = Some(field.into());
        self.before = Some(before.into());
        self.after = Some(after.into());
        self
    }

    /// Attach structured extras.
    pub fn with_meta(mut self, meta: serde_json::Value) -> Self {
        self.meta = Some(meta);
        self
    }
}

/// A position in the event log.
///
/// Either a ULID cursor or a timestamp. Both are supported because the two
/// callers differ: an agent resuming a session has the last event id it saw,
/// while a human asking "what happened this week" has a date. Collapsing them
/// into one would make one of the two awkward.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cursor {
    /// Everything strictly after this event id.
    After(EventId),
    /// Everything at or after this instant.
    Since(DateTime<Utc>),
    /// Everything, from the beginning.
    Beginning,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn action_names_round_trip() {
        for a in Action::ALL {
            assert_eq!(Action::parse(a.as_str()).unwrap(), a);
        }
        let err = Action::parse("deleted").unwrap_err().to_string();
        assert!(err.contains("deleted"), "{err}");
        assert!(
            err.contains("archived"),
            "should point at the soft-delete action: {err}"
        );
    }

    #[test]
    fn there_is_no_delete_action() {
        // D-9: soft delete only. If a `deleted` action ever appears here,
        // something has learned to hard-delete.
        assert!(!Action::ALL.iter().any(|a| a.as_str().contains("delete")));
    }

    #[test]
    fn event_ids_order_by_time() {
        let ids: Vec<_> = (0..20).map(|_| EventId::generate()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "the `since` range scan depends on this");
    }

    #[test]
    fn field_change_records_both_sides() {
        let e = NewEvent::new(
            EntityId::generate(EntityType::Task),
            Action::StatusChanged,
            "moved to done",
        )
        .field_change("status", "in_progress", "done");
        assert_eq!(e.field.as_deref(), Some("status"));
        assert_eq!(e.before, Some(serde_json::json!("in_progress")));
        assert_eq!(e.after, Some(serde_json::json!("done")));
    }
}
