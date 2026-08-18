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
//!
//! # One row per action, not per field
//!
//! A close writes four events and a claim three, so the feed showed four rows
//! and three rows for two things a person did — several of them unreadable,
//! because a value that is prose or merely long is rendered as its size:
//! `close_message none → (1130 characters)`. KB, reading it: "those are kinda
//! meaningless to a user" (KEEL-300).
//!
//! The events of one write share an entity and a timestamp, so they can be
//! grouped exactly rather than guessed at, and [`action_summary`] names what
//! the group did. This is the same argument [`headline`] already makes about
//! session summaries — say what happened, not how many fields moved — applied
//! to the rows underneath it, which had gone on counting.

use crate::{Actor, EntityId, EntityStore, EntityType, Note, Result, store::Store};
use chrono::{DateTime, Utc};

/// One thing a session did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// `evt_…` or `nte_…`. Which it is says what kind of change this was.
    ///
    /// Where several events are one action, this is the lowest of their ids —
    /// chosen rather than taken first so that a row keeps the same identity
    /// however the log ordered them.
    pub id: String,
    /// Whether this came from the event log or the note stream.
    pub kind: ChangeKind,
    /// The row it happened to, so the screen can link to it.
    pub entity_id: EntityId,
    /// The row's type, for the icon and for the link's destination.
    pub entity_type: EntityType,
    /// `KEEL-42` where the row has one. Empty otherwise.
    pub reference: String,
    /// One line, as a person would read it, naming the act rather than the
    /// fields it wrote. See [`action_summary`].
    pub summary: String,
    /// The project's short key, e.g. `KEEL`. `None` for rows that belong to no
    /// project. The all-projects feed shows this; a project-scoped one does not
    /// need it.
    pub project_key: Option<String>,
    /// The field this action is recognised by: `close_reason` for a close,
    /// `claimed_by` for a claim, `status` for a bare move, or the only field
    /// where just one changed. Carried so that a headline can tell a close from
    /// a claim structurally — both are `status_changed`, and telling them apart
    /// by reading `summary` would be parsing prose written for a person.
    /// `None` for creations, for notes, and for an action that touched several
    /// fields at once without being one of the acts above.
    pub field: Option<String>,
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
    /// The projects this session touched, by short key, in the order they were
    /// first written to. Usually one. More than one is a session that moved
    /// between projects, which is exactly what the all-projects feed could not
    /// previously show.
    pub projects: Vec<String>,
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
    ///
    /// A change is one *act*, not one field written, so a close counts once and
    /// not four times. The same number therefore reaches further back through
    /// the log than it did before rows were collapsed.
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

    // One row per action, not per field written.
    //
    // The events of one write share an entity *and* a timestamp, because they
    // are appended in a single transaction — so `(entity, at)` groups an action
    // exactly, with no time window to tune and nothing to infer from ordering.
    // Verified on KEEL-298, whose claim wrote three events at 09:12:17.341683Z
    // and whose close wrote four at 09:31:30.630211Z.
    //
    // What makes the key safe rather than merely tidy is that the store has one
    // writer at a time, held by an advisory lock (B-60). Two sessions cannot be
    // mid-write on the same row at the same microsecond, so a group is always
    // one transaction and always one session's work — which is what lets the
    // collapsed row take its actor and session from any member.
    let mut actions: Vec<Vec<crate::Event>> = Vec::new();
    let mut slot: std::collections::HashMap<(String, DateTime<Utc>), usize> = Default::default();
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
        // A creation is already one event for one act, and its summary names
        // the thing created. Grouping it with anything would only lose that.
        if event.action == crate::Action::Created {
            actions.push(vec![event]);
            continue;
        }
        let key = (event.entity_id.as_str().to_owned(), event.created_at);
        match slot.get(&key) {
            Some(&i) => actions[i].push(event),
            None => {
                slot.insert(key, actions.len());
                actions.push(vec![event]);
            }
        }
    }

    for group in actions {
        let Some(first) = group.first() else { continue };
        let kind = if first.action == crate::Action::Created {
            ChangeKind::Created
        } else {
            ChangeKind::Field
        };
        let reference = reference_for(&keys, &numbers, &first.entity_id, first.project_id.as_ref());
        let project_key = first
            .project_id
            .as_ref()
            .and_then(|p| keys.get(p.as_str()).cloned());
        // The lowest id in the group, so a row's identity does not depend on
        // the order the log happened to return its events in.
        let Some(id) = group.iter().map(|e| e.id.as_str().to_owned()).min() else {
            continue;
        };
        owner.insert(id.clone(), (first.session_id.clone(), first.actor));
        all.push(Change {
            id,
            kind,
            entity_id: first.entity_id.clone(),
            entity_type: first.entity_type,
            reference,
            summary: action_summary(&group),
            project_key,
            field: action_field(&group),
            at: first.created_at,
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
            project_key: note
                .project_id
                .as_ref()
                .and_then(|p| keys.get(p.as_str()).cloned()),
            field: None,
            at: note.created_at,
        });
    }

    // Newest first, then cut, then grouped. Cutting before grouping is what
    // makes the limit mean "the most recent N changes" rather than "N changes
    // from whichever stream happened to be read first". Collapsing has already
    // happened by this point, so the cut counts acts and a close cannot lose
    // its own status event to the limit while keeping its reason.
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
                projects: Vec::new(),
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
        for change in &group.changes {
            if let Some(key) = &change.project_key
                && !group.projects.contains(key)
            {
                group.projects.push(key.clone());
            }
        }
        group.headline = headline(group);
    }
    grouped.sort_by_key(|g| std::cmp::Reverse(g.ended_at));

    Ok(ChangeLog {
        changes: total_read.min(limit),
        sessions: grouped,
        truncated,
    })
}

/// What one action did, as a person would say it.
///
/// Handed every event of a single write at once, so it can name the act rather
/// than list the fields underneath it. A close writes `status`, `close_reason`,
/// `close_message` and `evidence`; the feed used to show those as four rows,
/// two of them reading `close_message none → (1130 characters)` and `evidence
/// [] → (112 characters)`. KB, looking at it: "those are kinda meaningless to a
/// user" (KEEL-300).
///
/// This is [`headline`]'s argument one level down. That function already
/// refuses to count field writes, on the grounds that a count measures the
/// storage model and not the work; the rows beneath it went on counting them.
///
/// **Prose is never quoted here, and brevity is not the reason.**
/// [`crate::event::render_field_value`] elides bodies and over-long values
/// because a redacted body was once republished into the committed changelog by
/// the very edit that removed it (KEEL-215). So a collapsed close says `closed
/// as done` — the `close_reason`, which is an enum — and never the close
/// message it was written with. Collapsing drops those rows; it must never
/// start quoting them.
fn action_summary(group: &[crate::Event]) -> String {
    if let Some(created) = group.iter().find(|e| e.action == crate::Action::Created) {
        return created.summary.clone();
    }
    let field_of = |name: &str| group.iter().find(|e| e.field.as_deref() == Some(name));

    // `close_reason` is the marker rather than `status`, because it is written
    // exactly once per close while a status event also fires for a claim and
    // for every other move a task makes. `headline` picks it for the same
    // reason, and the two must agree or a row and its session disagree about
    // what happened.
    if let Some(close) = field_of("close_reason") {
        return match close.after.as_ref().and_then(|v| v.as_str()) {
            Some(reason) => format!("closed as {}", reason.replace('_', " ")),
            None => "closed".to_owned(),
        };
    }
    if let Some(claim) = field_of("claimed_by") {
        let held_before = claim.before.as_ref().is_some_and(|v| !v.is_null());
        let held_after = claim.after.as_ref().is_some_and(|v| !v.is_null());
        return match (held_before, held_after) {
            // A takeover and a first claim are both `claimed_by` moving to a
            // session id, and they are not the same event to read: one of them
            // means somebody else's work was picked up.
            (true, true) => "taken over".to_owned(),
            (true, false) => "released".to_owned(),
            _ => "claimed".to_owned(),
        };
    }
    if let Some(status) = field_of("status") {
        return status.summary.clone();
    }

    if let [only] = group {
        return match only.field.as_deref() {
            // One field whose value cannot be shown. Naming it answers "what
            // did this session touch", which is the question the feed is for;
            // the size of the text answers nothing.
            Some(field) if !quotable(only) => format!("{} changed", field.replace('_', " ")),
            _ => only.summary.clone(),
        };
    }

    let mut named: Vec<String> = group
        .iter()
        .filter_map(|e| e.field.as_deref())
        .map(|f| f.replace('_', " "))
        .collect();
    named.sort();
    named.dedup();
    match named.is_empty() {
        true => group.first().map(|e| e.summary.clone()).unwrap_or_default(),
        false => format!("updated {}", named.join(", ")),
    }
}

/// The field a collapsed row still answers for.
///
/// [`headline`] counts a close by looking for `close_reason` on a change, so a
/// collapsed close has to keep carrying it. The row is one line now; the thing
/// that makes it a close is still the field underneath.
fn action_field(group: &[crate::Event]) -> Option<String> {
    for marker in ["close_reason", "claimed_by", "status"] {
        if group.iter().any(|e| e.field.as_deref() == Some(marker)) {
            return Some(marker.to_owned());
        }
    }
    match group {
        [only] => only.field.clone(),
        _ => None,
    }
}

/// Whether both sides of this event can be shown as themselves.
///
/// Two separate reasons a value cannot be, kept apart on purpose.
///
/// [`crate::event::is_elided`] is the redaction rule, and it is asked rather
/// than inferred from the rendered summary so that the feed and the renderer
/// cannot reach different conclusions about the same value. It is safety, and
/// nothing here should widen it.
///
/// [`is_an_id`] is a readability rule, and it is this module's own. A ULID is
/// short enough and prosaic enough to be quoted, so `milestone_id none →
/// mst_01KZSNZS9H4E4TA1J1SD72DB53` passed every test the redaction rule
/// applies — and it is as unreadable as the size it replaced. Twenty-one rows
/// of the store's sixteen hundred looked like that.
fn quotable(event: &crate::Event) -> bool {
    let Some(field) = event.field.as_deref() else {
        return true;
    };
    let shown = |value: &Option<serde_json::Value>| {
        value
            .as_ref()
            .is_none_or(|v| !crate::event::is_elided(field, v) && !v.as_str().is_some_and(is_an_id))
    };
    shown(&event.before) && shown(&event.after)
}

/// Whether this text is one of Specline's own identifiers.
///
/// Asks the id types to parse it rather than matching a shape, so a new
/// *artifact* type is covered the day it exists rather than the day somebody
/// remembers to add its prefix here.
///
/// The two are asked separately because a blob is not an entity: `blob_id none
/// → blb_01KZQ6C14JD500VB3VESY5ZR65` is the one row in sixteen hundred that
/// [`EntityId::parse`] alone still let through. Anything with an id type that
/// is neither will need a line here, which is the cost of not guessing from the
/// shape.
///
/// What a reader actually wants in place of an id is the *name* — "moved to
/// Phase 11" rather than "milestone id changed" — and that needs a lookup this
/// function does not have. Naming the field is the honest floor; the better
/// version is its own piece of work rather than something smuggled in here.
fn is_an_id(text: &str) -> bool {
    EntityId::parse(text).is_ok() || crate::BlobId::parse(text).is_ok()
}

/// A session's one-line account of itself.
///
/// Names what the session *did*, rather than counting what it wrote. One claim
/// writes three events and one close writes four, so a count of changes measures
/// the storage model and not the work: eight sessions all rendered as "created N
/// things, N changes, wrote N notes", which is the same sentence eight times
/// (KEEL-292).
///
/// Closes lead, because "what got finished" is the first thing somebody coming
/// back to the machine wants. The raw count survives on the end, quietly, so a
/// session that closed one task after four hours of thrashing still reads
/// differently from one that closed a task cleanly.
fn headline(group: &SessionChanges) -> String {
    let mut closed: Vec<&str> = Vec::new();
    let mut closed_total = 0usize;
    let mut created: Vec<(EntityType, usize)> = Vec::new();
    let mut notes = 0usize;

    for change in &group.changes {
        match change.kind {
            ChangeKind::Note => notes += 1,
            ChangeKind::Created => match created.iter_mut().find(|(t, _)| *t == change.entity_type)
            {
                Some((_, n)) => *n += 1,
                None => created.push((change.entity_type, 1)),
            },
            // `close_reason` is the marker rather than `status`, because it is
            // written exactly once per close, while a status event also fires
            // for a claim and for every other move a task makes.
            ChangeKind::Field => {
                if change.field.as_deref() == Some("close_reason") {
                    closed_total += 1;
                    if !change.reference.is_empty() {
                        closed.push(&change.reference);
                    }
                }
            }
        }
    }

    let mut parts = Vec::new();
    if closed_total > 0 {
        parts.push(format!("closed {}", names(&closed, closed_total)));
    }
    if !created.is_empty() {
        // Biggest group first, then alphabetically, so the same session always
        // renders the same way.
        created.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.as_str().cmp(b.0.as_str())));
        let filed: Vec<String> = created
            .iter()
            .map(|(t, n)| format!("{n} {}", plural(*t, *n)))
            .collect();
        parts.push(format!("filed {}", filed.join(", ")));
    }
    if notes > 0 {
        parts.push(format!(
            "{notes} {}",
            if notes == 1 { "note" } else { "notes" }
        ));
    }
    let total = group.changes.len();
    let count = format!("{total} {}", if total == 1 { "change" } else { "changes" });

    if parts.is_empty() {
        // Nothing was closed, created or noted, but the session may still have
        // edited rows — a claim writes three fields and a document revision
        // writes one, and neither is an act this function names. Falling
        // through to "nothing" would call a session that did work idle.
        return if group.changes.is_empty() {
            "nothing".to_owned()
        } else {
            count
        };
    }

    format!("{} · {count}", parts.join(" · "))
}

/// The references, listed while listing them still fits on a row.
///
/// `total` rather than `refs.len()` because a row without a reference still
/// happened, and a headline that silently dropped it would undercount the one
/// number a reader is most likely to check.
fn names(refs: &[&str], total: usize) -> String {
    match (refs.len(), total) {
        (_, 0) => String::new(),
        (0, n) => format!("{n} {}", if n == 1 { "task" } else { "tasks" }),
        (1, 1) => refs[0].to_owned(),
        (2, 2) => format!("{} and {}", refs[0], refs[1]),
        (3, 3) => format!("{}, {} and {}", refs[0], refs[1], refs[2]),
        (r, n) => {
            let shown = r.min(2);
            format!("{} and {} more", refs[..shown].join(", "), n - shown)
        }
    }
}

/// An artifact type as a person says it, singular or plural.
///
/// `metric_observation` has to lose its underscore, and "feedback" is already
/// plural — an automatic "s" produces "2 feedbacks", which is the kind of thing
/// that makes a page look machine-written.
fn plural(entity: EntityType, n: usize) -> String {
    let base = entity.as_str().replace('_', " ");
    if n == 1 || base == "feedback" {
        return base;
    }
    format!("{base}s")
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
            projects: Vec::new(),
            changes: Vec::new(),
            headline: String::new(),
        };
        assert_eq!(headline(&base), "nothing");
    }

    #[test]
    fn references_are_listed_while_listing_them_still_fits() {
        assert_eq!(names(&["KEEL-1"], 1), "KEEL-1");
        assert_eq!(names(&["KEEL-1", "KEEL-2"], 2), "KEEL-1 and KEEL-2");
        assert_eq!(
            names(&["KEEL-1", "KEEL-2", "KEEL-3"], 3),
            "KEEL-1, KEEL-2 and KEEL-3"
        );
        assert_eq!(
            names(&["KEEL-1", "KEEL-2", "KEEL-3", "KEEL-4"], 4),
            "KEEL-1, KEEL-2 and 2 more"
        );
    }

    /// A row with no reference still happened. Counting only what could be named
    /// would quietly report four closes as two.
    #[test]
    fn a_close_with_no_reference_is_still_counted() {
        assert_eq!(names(&[], 1), "1 task");
        assert_eq!(names(&[], 3), "3 tasks");
        assert_eq!(names(&["KEEL-1"], 3), "KEEL-1 and 2 more");
    }

    #[test]
    fn a_type_is_pluralised_the_way_a_person_says_it() {
        assert_eq!(plural(EntityType::Task, 1), "task");
        assert_eq!(plural(EntityType::Task, 2), "tasks");
        assert_eq!(plural(EntityType::Decision, 3), "decisions");
        // Already plural. "2 feedbacks" reads as machine-written.
        assert_eq!(plural(EntityType::Feedback, 2), "feedback");
        // The underscore is a column name, not something anybody says.
        assert_eq!(
            plural(EntityType::MetricObservation, 2),
            "metric observations"
        );
    }

    #[test]
    fn a_note_is_shortened_to_its_first_line() {
        assert_eq!(first_line("First line\n\nSecond paragraph"), "First line");
        assert_eq!(first_line("\n\n  Indented first  \n"), "Indented first");
        assert!(first_line(&"x".repeat(400)).ends_with('…'));
    }
}
