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
    /// each of `specline_activity`, `specline_context` and the desktop feed.
    pub const fn is_notable(&self) -> bool {
        true
    }

    /// The one-line description, safe to write into a file that gets committed.
    ///
    /// Use this rather than [`Event::summary`] anywhere the result is
    /// **written to disk and committed** — `product/STATUS.md`'s changelog is
    /// the case this exists for.
    ///
    /// It rebuilds the line from the structured `field`, `before` and `after`
    /// rather than trusting the stored text, and that is the point. The stored
    /// `summary` is whatever was formatted at write time, so an event appended
    /// before [`render_field_value`] existed still holds a full prose body in
    /// it — and events are immutable, so it always will. Recomputing here is
    /// what makes the guarantee retrospective instead of only applying to
    /// whatever happens next.
    ///
    /// Falls back to the stored summary for events that carry no field change,
    /// which is every create, archive and link. Those summaries are built from
    /// a label, not from a field value, so there is nothing to rebuild.
    pub fn publishable_summary(&self) -> String {
        match (&self.field, &self.before, &self.after) {
            (Some(field), Some(before), Some(after)) => format!(
                "{field} {} → {}",
                render_field_value(field, before),
                render_field_value(field, after)
            ),
            _ => self.summary.clone(),
        }
    }
}

/// The longest field value an event summary will quote.
///
/// Eighty characters covers every status, priority, label list, id and date
/// this store holds, and stops short of prose. It is a bound, not a
/// classifier — see [`render_field_value`].
pub const SUMMARY_VALUE_LIMIT: usize = 80;

/// Fields whose value is prose and is never quoted in a summary, whatever its
/// length.
///
/// Named rather than inferred, because a short body is still a body: the reason
/// these are elided is what they are, not how big they happen to be today.
pub const PROSE_FIELDS: [&str; 3] = ["body", "summary", "definition"];

/// How a field's old and new values appear in an event summary.
///
/// **This is the only thing standing between "somebody edited a value out" and
/// "the old value is published anyway."** Summaries are printed into the
/// Changelog table of `product/STATUS.md`, which is committed — so anything
/// quoted here lands in a repository, and the event it came from is immutable
/// by design and cannot be edited later to take it back.
///
/// That was not hypothetical. A machine path was edited out of a task body
/// before publishing the repository, and the next `specline generate` put the old
/// body — path included — straight into the changelog, one section below the
/// text that had just been cleaned. The edit that removed the string added a
/// copy of it (KEEL-215).
///
/// So values are quoted only when they are short *and* not prose. A long value
/// is reported by size, which still answers the question the changelog exists
/// to answer — this field changed, by roughly this much — without reproducing
/// it. Truncating to a prefix was the obvious alternative and is worse: the
/// first eighty characters of a body are as likely to be the sensitive part as
/// any other eighty.
///
/// Two limits worth being honest about. The length bound is a heuristic, so an
/// eighty-character title that should not have been written down is still
/// quoted; and the event *row* keeps the full before and after in its columns,
/// because that is the audit trail and removing it would break what the event
/// log is for. What this guarantees is narrower and is the guarantee that was
/// missing: what gets **committed to a repository** can be redacted.
pub fn render_field_value(field: &str, value: &serde_json::Value) -> String {
    let text = match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => return "none".to_owned(),
        other => other.to_string(),
    };

    // `chars`, not `len`: truncating a UTF-8 string by bytes splits characters,
    // and half the titles in this store contain an em dash.
    let characters = text.chars().count();
    if PROSE_FIELDS.contains(&field) || characters > SUMMARY_VALUE_LIMIT {
        return format!("({characters} characters)");
    }
    text
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
    use serde_json::json;

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

    // --- What an event summary is allowed to quote --------------------------
    //
    // These guard a publication boundary, not a formatting preference. The
    // summary is printed into `product/STATUS.md`'s changelog, which is
    // committed; the event it comes from is immutable. So a value quoted here
    // is in a repository permanently and cannot be edited out afterwards.

    /// The short scalars are the reason the changelog is worth reading. If this
    /// ever starts eliding them the fix has gone too far.
    #[test]
    fn a_short_value_is_quoted_in_full() {
        assert_eq!(
            render_field_value("status", &serde_json::json!("in_progress")),
            "in_progress"
        );
        assert_eq!(
            render_field_value("priority", &serde_json::json!("p2")),
            "p2"
        );
        assert_eq!(
            render_field_value("claimed_by", &serde_json::json!(null)),
            "none"
        );
        assert_eq!(render_field_value("rank", &serde_json::json!(208)), "208");
    }

    /// A body is prose whatever its length. Short is not the same as harmless —
    /// "acquiring Acme" is twelve characters.
    #[test]
    fn a_prose_field_is_never_quoted_however_short() {
        assert_eq!(
            render_field_value("body", &serde_json::json!("tiny")),
            "(4 characters)"
        );
        assert_eq!(
            render_field_value("summary", &serde_json::json!("also tiny")),
            "(9 characters)"
        );
        assert_eq!(
            render_field_value("definition", &serde_json::json!("brief")),
            "(5 characters)"
        );
    }

    /// The regression this exists for. KEEL-215: a machine path was edited out
    /// of a task body, and the next generate published the old body — path and
    /// all — into the changelog.
    #[test]
    fn the_value_being_redacted_does_not_come_back_through_the_summary() {
        let secret = "/Users/somebody/development/specline";
        let before = json!(format!(
            "Hit while closing KEEL-133 from a worktree; the writes landed in {secret} instead."
        ));
        let after =
            json!("Hit while closing KEEL-133 from a worktree; the writes landed elsewhere.");

        let summary = format!(
            "{} {} → {}",
            "body",
            render_field_value("body", &before),
            render_field_value("body", &after)
        );

        assert!(
            !summary.contains(secret),
            "the changelog must not reprint the value someone just removed: {summary}"
        );
        assert!(
            !summary.contains("worktree"),
            "and not a prefix of it either — the first eighty characters are as \
             likely to be the sensitive part as any other: {summary}"
        );
        assert!(
            summary.starts_with("body ("),
            "it should still say which field changed and roughly how much: {summary}"
        );
    }

    /// A long non-prose value is elided too. The named list is the primary
    /// rule; the length bound is the backstop for a field nobody has thought
    /// about yet, and it has to fail closed.
    #[test]
    fn a_long_value_is_elided_even_when_the_field_is_not_named_prose() {
        let long = "x".repeat(SUMMARY_VALUE_LIMIT + 1);
        assert_eq!(
            render_field_value("title", &serde_json::json!(long)),
            format!("({} characters)", SUMMARY_VALUE_LIMIT + 1)
        );

        let at_the_limit = "x".repeat(SUMMARY_VALUE_LIMIT);
        assert_eq!(
            render_field_value("title", &serde_json::json!(at_the_limit)),
            at_the_limit
        );
    }

    /// The retrospective half, and the reason `publishable_summary` rebuilds
    /// instead of trusting what is stored.
    ///
    /// An event written before this rule existed still holds the whole prose
    /// value in its `summary` column, and events are immutable — so fixing the
    /// write path alone would have left every historical row leaking, including
    /// the one that found the bug.
    #[test]
    fn an_event_written_before_the_rule_is_still_rendered_safely() {
        let secret = "/Users/somebody/development/specline";
        let event = Event {
            id: EventId::generate(),
            project_id: None,
            entity_type: EntityType::Task,
            entity_id: EntityId::generate(EntityType::Task),
            action: Action::Updated,
            field: Some("body".to_owned()),
            before: Some(json!(format!("the writes landed in {secret} instead"))),
            after: Some(json!("the writes landed elsewhere")),
            actor: Actor::Claude,
            session_id: None,
            surface: None,
            // Exactly what the old formatter stored, secret and all.
            summary: format!(
                "body the writes landed in {secret} instead → the writes landed elsewhere"
            ),
            meta: None,
            created_at: Utc::now(),
        };

        assert!(
            event.summary.contains(secret),
            "the fixture is only meaningful if the stored summary really does leak"
        );
        assert!(
            !event.publishable_summary().contains(secret),
            "but what gets committed must not: {}",
            event.publishable_summary()
        );
    }

    /// A create or an archive carries no field change, so there is nothing to
    /// rebuild and the stored summary is the answer. Without this the changelog
    /// would lose every line that is not an update.
    #[test]
    fn an_event_with_no_field_change_keeps_its_stored_summary() {
        let event = Event {
            id: EventId::generate(),
            project_id: None,
            entity_type: EntityType::Task,
            entity_id: EntityId::generate(EntityType::Task),
            action: Action::Created,
            field: None,
            before: None,
            after: None,
            actor: Actor::Claude,
            session_id: None,
            surface: None,
            summary: "created task “Ship the plugin”".to_owned(),
            meta: None,
            created_at: Utc::now(),
        };

        assert_eq!(
            event.publishable_summary(),
            "created task “Ship the plugin”"
        );
    }

    /// Counted in characters, not bytes. Half the titles in this store contain
    /// an em dash, and a byte-wise bound would both miscount and, if it ever
    /// became a truncation again, split one down the middle.
    #[test]
    fn the_bound_counts_characters_not_bytes() {
        // 40 em dashes: 40 characters, 120 bytes. Under the limit either way
        // that matters, and over it if you count bytes.
        let dashes = "—".repeat(40);
        assert_eq!(
            dashes.len(),
            120,
            "this fixture is only interesting if it is multi-byte"
        );
        assert_eq!(
            render_field_value("title", &serde_json::json!(dashes.clone())),
            dashes,
            "40 characters is under the limit; counting its 120 bytes would wrongly elide it"
        );
    }
}
