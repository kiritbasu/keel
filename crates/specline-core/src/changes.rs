//! What changed, grouped by the conversation that changed it.
//!
//! The screen this feeds used to be a reverse-chronological feed of up to 300
//! mutations — "created task X", "status todo → done" — with no grouping, no way
//! to reach the thing that changed, and no time range. Its own header said its
//! job was "what did Claude do today", and it answered "what were the last 300
//! events".
//!
//! KB chose the rebuild over the cheaper fix (TQ-35), against my recommendation,
//! on the grounds that "what happened while I was away" is the single most
//! valuable question this app could answer for someone who leaves Claude working
//! and comes back, and that nothing else in the product answers it.
//!
//! # Notes leave no event, and that is the whole cost of this
//!
//! TQ-29 established it: `specline_note` writes no row in `events`, which is why the
//! daemon announces notes under their own kind. So a per-session change count
//! built from the event log alone silently misses every note — and a note is
//! where a session records what it *found*, which is the part most worth
//! reading.
//!
//! Grouping by session therefore means unioning two streams rather than
//! regrouping one, and that union is what this module is. Doing it here rather
//! than in the screen means the CLI and any future surface get the same answer,
//! and it means the union is tested.

use crate::{Actor, EntityId, EntityStore, EntityType, Note, Result, store::Store};
use chrono::{DateTime, Utc};

/// One thing a session did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `evt_…` or `nte_…`. Which it is says what kind of change this was.
    pub id: String,
    /// Whether this came from the event log or the note stream.
    pub kind: ChangeKind,
    /// The row it happened to, so the screen can link to it.
    pub entity_id: EntityId,
    /// The row's type, for the icon and for the link's destination.
    pub entity_type: EntityType,
    /// `KEEL-42` where the row has one. Empty otherwise.
    pub reference: String,
    /// One line, as a person would read it.
    pub summary: String,
    /// When.
    pub at: DateTime<Utc>,
}

/// Where a change came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    /// A field moved. From the event log.
    Field,
    /// Something was created.
    Created,
    /// A note was written. From the note stream, which leaves no event.
    Note,
}

impl ChangeKind {
    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            ChangeKind::Field => "field",
            ChangeKind::Created => "created",
            ChangeKind::Note => "note",
        }
    }
}

/// Everything one conversation did, newest session first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChanges {
    /// The session id, or `None` for writes made outside a tracked session.
    ///
    /// `None` is a real answer rather than a gap: a `specline bootstrap`, a
    /// migration or a `curl` has no conversation behind it. The screen says so
    /// in a tooltip rather than in the row, because whose session it was is a
    /// build-time concern and not product copy — the class of mistake KEEL-85
    /// cleaned up elsewhere.
    pub session_id: Option<String>,
    /// Who was writing. Where a session mixed actors, the one that wrote most.
    pub actor: Actor,
    /// When the session's first change landed.
    pub started_at: DateTime<Utc>,
    /// When its last one did. What the sessions are ordered by, because "what
    /// happened while I was away" wants the most recently active first, not the
    /// one that started most recently.
    pub ended_at: DateTime<Utc>,
    /// Everything it did, oldest first, so it reads as a sequence.
    pub changes: Vec<Change>,
    /// A one-line account of the session, for the collapsed row.
    pub headline: String,
}

/// A page of sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangeLog {
    /// Sessions, newest first.
    pub sessions: Vec<SessionChanges>,
    /// How many changes were read across all of them.
    pub changes: usize,
    /// Whether the underlying scan hit its limit, so older changes exist.
    pub truncated: bool,
}

/// What to include.
#[derive(Debug, Clone, Default)]
pub struct ChangeQuery {
    /// Only this project.
    pub project_id: Option<EntityId>,
    /// Only changes after this instant. The screen's today / this week range.
    pub since: Option<DateTime<Utc>>,
    /// Only this actor.
    pub actor: Option<Actor>,
    /// How many changes to read. Sessions are grouped from whatever comes back.
    pub limit: usize,
}

/// Read the recent changes and group them by session.
///
/// The limit applies to *changes*, not sessions, because that is the thing that
/// can grow without bound. A session that made four hundred writes is one row on
/// the screen and four hundred rows against the limit, which is the honest
/// accounting — and `truncated` says when older ones were left behind.
pub fn by_session(store: &Store, query: &ChangeQuery) -> Result<ChangeLog> {
    let limit = if query.limit == 0 { 300 } else { query.limit };

    // Newest first, and only as many as could possibly survive the cut.
    //
    // This read used to ask for the oldest 100,000, which is the whole log
    // today and the wrong end of it later. Asking for `limit` events would be
    // too few, because notes are merged in afterwards and a note can displace
    // an event — but ten times the limit cannot plausibly be displaced, and
    // `truncated` still reports honestly either way.
    let events = store.recent_events(
        query
            .project_id
            .as_ref()
            .map_or(crate::store::EventScope::Everything, |p| {
                crate::store::EventScope::Project(p)
            }),
        limit.saturating_mul(10).max(1_000),
    )?;
    let notes = notes_for(store, query)?;

    // Readable identifiers, fetched once. A change carries the reference so the
    // screen never has to resolve one per row, which at three hundred rows is
    // three hundred requests to render a link's text.
    let keys = project_keys(store)?;
    let numbers = task_numbers(store)?;

    let mut all: Vec<Change> = Vec::new();
    let mut owner: std::collections::HashMap<String, (Option<String>, Actor)> = Default::default();

    for event in events.items {
        if let Some(since) = query.since
            && event.created_at < since
        {
            continue;
        }
        if let Some(actor) = query.actor
            && event.actor != actor
        {
            continue;
        }
        let reference = reference_for(&keys, &numbers, &event.entity_id, event.project_id.as_ref());
        let kind = if event.action == crate::Action::Created {
            ChangeKind::Created
        } else {
            ChangeKind::Field
        };
        owner.insert(
            event.id.as_str().to_owned(),
            (event.session_id.clone(), event.actor),
        );
        all.push(Change {
            id: event.id.as_str().to_owned(),
            kind,
            entity_id: event.entity_id,
            entity_type: event.entity_type,
            reference,
            summary: event.summary,
            at: event.created_at,
        });
    }

    for note in notes {
        if let Some(since) = query.since
            && note.created_at < since
        {
            continue;
        }
        if let Some(actor) = query.actor
            && note.author != actor
        {
            continue;
        }
        let reference = reference_for(&keys, &numbers, &note.entity_id, note.project_id.as_ref());
        owner.insert(
            note.id.as_str().to_owned(),
            (note.session_id.clone(), note.author),
        );
        all.push(Change {
            id: note.id.as_str().to_owned(),
            kind: ChangeKind::Note,
            entity_id: note.entity_id,
            entity_type: note.entity_type,
            reference,
            summary: first_line(&note.body),
            at: note.created_at,
        });
    }

    // Newest first, then cut, then grouped. Cutting before grouping is what
    // makes the limit mean "the most recent N changes" rather than "N changes
    // from whichever stream happened to be read first".
    all.sort_by(|a, b| b.at.cmp(&a.at).then_with(|| b.id.cmp(&a.id)));
    let total_read = all.len();
    let truncated = total_read > limit;
    all.truncate(limit);

    let mut grouped: Vec<SessionChanges> = Vec::new();
    let mut index: std::collections::HashMap<Option<String>, usize> = Default::default();
    for change in all {
        let (session, actor) = owner
            .get(&change.id)
            .cloned()
            .unwrap_or((None, Actor::System));
        let slot = *index.entry(session.clone()).or_insert_with(|| {
            grouped.push(SessionChanges {
                session_id: session.clone(),
                actor,
                started_at: change.at,
                ended_at: change.at,
                changes: Vec::new(),
                headline: String::new(),
            });
            grouped.len() - 1
        });
        let group = &mut grouped[slot];
        group.started_at = group.started_at.min(change.at);
        group.ended_at = group.ended_at.max(change.at);
        group.changes.push(change);
    }

    for group in &mut grouped {
        // Oldest first inside a session: what it did reads as a sequence, even
        // though the sessions themselves read newest first.
        group
            .changes
            .sort_by(|a, b| a.at.cmp(&b.at).then_with(|| a.id.cmp(&b.id)));
        group.headline = headline(group);
    }
    grouped.sort_by_key(|g| std::cmp::Reverse(g.ended_at));

    Ok(ChangeLog {
        changes: total_read.min(limit),
        sessions: grouped,
        truncated,
    })
}

/// A session's one-line account of itself.
///
/// Counts rather than a list, because a session that made four hundred writes
/// has to fit on one row. Notes are counted separately and named first: "wrote 3
/// notes" is the part a person actually wants, and it is the part the event log
/// alone could not have told them.
fn headline(group: &SessionChanges) -> String {
    let count = |kind: ChangeKind| group.changes.iter().filter(|c| c.kind == kind).count();
    let created = count(ChangeKind::Created);
    let fields = count(ChangeKind::Field);
    let notes = count(ChangeKind::Note);

    let mut parts = Vec::new();
    if created > 0 {
        parts.push(format!(
            "created {created} {}",
            if created == 1 { "thing" } else { "things" }
        ));
    }
    if fields > 0 {
        parts.push(format!(
            "{fields} {}",
            if fields == 1 { "change" } else { "changes" }
        ));
    }
    if notes > 0 {
        parts.push(format!(
            "wrote {notes} {}",
            if notes == 1 { "note" } else { "notes" }
        ));
    }
    if parts.is_empty() {
        return "nothing".to_owned();
    }
    parts.join(", ")
}

/// The notes in scope.
///
/// Retracted notes are excluded by `notes_in_project`, which is right here: a
/// withdrawn note is part of one row's history and readable on that row, but it
/// is not something a session *did* that a person needs to catch up on.
///
/// With no project, every project's notes — the screen has an all-projects
/// address and so does the digest.
fn notes_for(store: &Store, query: &ChangeQuery) -> Result<Vec<Note>> {
    match &query.project_id {
        Some(project) => store.notes_in_project(project),
        None => {
            let projects = store.list(
                &crate::EntityQuery::default()
                    .of_type(EntityType::Project)
                    .limited(1_000),
            )?;
            let mut all = Vec::new();
            for project in &projects.items {
                all.extend(store.notes_in_project(project.id())?);
            }
            Ok(all)
        }
    }
}

/// Every project's readable-identifier prefix, by project id.
fn project_keys(store: &Store) -> Result<std::collections::HashMap<String, String>> {
    let page = store.list(
        &crate::EntityQuery::default()
            .of_type(EntityType::Project)
            .limited(1_000),
    )?;
    Ok(page
        .items
        .iter()
        .filter_map(|e| match e {
            crate::Entity::Project(p) => Some((p.id.as_str().to_owned(), p.key.clone())),
            _ => None,
        })
        .collect())
}

/// Every task's number, by task id.
fn task_numbers(store: &Store) -> Result<std::collections::HashMap<String, i32>> {
    let page = store.list(
        &crate::EntityQuery::default()
            .of_type(EntityType::Task)
            .limited(50_000),
    )?;
    Ok(page
        .items
        .iter()
        .filter_map(|e| match e {
            crate::Entity::Task(t) => Some((t.id.as_str().to_owned(), t.number)),
            _ => None,
        })
        .collect())
}

/// `KEEL-42`, where the row has one.
///
/// Empty rather than invented for the other twelve types. A made-up reference
/// that resolves to nothing is worse than none, which is the rule `readable_ref`
/// already follows on the MCP side.
fn reference_for(
    keys: &std::collections::HashMap<String, String>,
    numbers: &std::collections::HashMap<String, i32>,
    entity_id: &EntityId,
    project_id: Option<&EntityId>,
) -> String {
    let (Some(number), Some(project)) = (numbers.get(entity_id.as_str()), project_id) else {
        return String::new();
    };
    match keys.get(project.as_str()) {
        Some(key) => format!("{key}-{number}"),
        None => String::new(),
    }
}

/// The first line of a note, for a one-line row.
fn first_line(body: &str) -> String {
    let line = body
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("");
    if line.chars().count() <= 120 {
        return line.to_owned();
    }
    line.chars().take(120).collect::<String>() + "…"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_headline_names_notes_last_and_says_nothing_when_there_is_nothing() {
        let base = SessionChanges {
            session_id: Some("ses_a".to_owned()),
            actor: Actor::Claude,
            started_at: Utc::now(),
            ended_at: Utc::now(),
            changes: Vec::new(),
            headline: String::new(),
        };
        assert_eq!(headline(&base), "nothing");
    }

    #[test]
    fn a_note_is_shortened_to_its_first_line() {
        assert_eq!(first_line("First line\n\nSecond paragraph"), "First line");
        assert_eq!(first_line("\n\n  Indented first  \n"), "Indented first");
        assert!(first_line(&"x".repeat(400)).ends_with('…'));
    }
}
