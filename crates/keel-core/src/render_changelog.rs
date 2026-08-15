//! The changelog renderer — `product/CHANGELOG.md` and its equivalents.
//!
//! What closed, newest first, and the event log underneath it.
//!
//! This exists because the tracker had swallowed it. `product/STATUS.md`
//! rendered every task a project had ever had, closed ones included, and on
//! this project that reached 488 KB with **87% of the file describing finished
//! work**. The consequence was not untidiness: the standing contract makes
//! "read the tracker" the first instruction of every session, and at that size
//! the read was refused for exceeding the reader's limit. An instruction that
//! cannot be carried out is worse than none, and closed work is the part that
//! grows without bound.
//!
//! So the split is by *what the reader is asking*. The tracker answers "what is
//! happening and what is next" and stays small enough to read. This answers
//! "what has happened", and may grow as large as it likes because nobody reads
//! it front to back.
//!
//! It is the same shape as [`crate::render_status`] and
//! [`crate::render_decisions`] — a view over rows, one-directional, generated
//! and never read back.
//!
//! **Its path is derived, not stored.** A project names its tracker
//! (`status_path`) and its decision log (`decisions_path`) in columns, and the
//! obvious third column was rejected on cost: a new column is a schema
//! migration, a schema migration is a version the updater refuses to apply
//! without a person, and that is a large price for somewhere to put a filename
//! nobody has asked to configure. The changelog is written beside the tracker
//! as `CHANGELOG.md`. If anyone ever needs it elsewhere, the column can be
//! added then and this becomes its default.

use crate::{Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result, Store};
use std::fmt::Write as _;

/// Where the changelog goes for a project whose tracker is at `status_path`.
///
/// Beside the tracker, because the two are read as a pair and a reader who
/// found one should not have to search for the other.
pub fn path_beside(status_path: &str) -> String {
    match status_path.rfind('/') {
        Some(cut) => format!("{}/CHANGELOG.md", &status_path[..cut]),
        None => "CHANGELOG.md".to_owned(),
    }
}

/// How many events the table carries. Stated in the file when it cuts, never
/// silently — the same rule every other list in Keel follows.
const EVENTS_SHOWN: usize = 200;

/// Render the changelog for one project.
pub fn render(store: &Store, project_id: &EntityId) -> Result<String> {
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Err(Error::NotFound {
            entity_type: EntityType::Project,
            id: project_id.to_string(),
        });
    };

    let mut out = String::new();
    let now = chrono::Utc::now();

    writeln!(out, "# {} — Changelog", project.name)?;
    writeln!(out)?;
    writeln!(
        out,
        "<!-- keel:generated project {} {} -->",
        project.id,
        now.format("%Y-%m-%dT%H:%M:%SZ")
    )?;
    writeln!(
        out,
        "> **Generated from the task rows and the event log. Do not edit — Keel is the source of \
         truth.**"
    )?;
    writeln!(out)?;
    writeln!(
        out,
        "What has finished. What is happening now is in the tracker beside this file."
    )?;
    writeln!(out)?;

    // --- What closed ------------------------------------------------------
    //
    // Sorted by when it closed rather than by id, because this is read as
    // history. A row closed before `closed_at` was recorded sorts last rather
    // than being given a date it does not have.
    let tasks = store
        .list(
            &EntityQuery::in_project(project_id.clone())
                .of_type(EntityType::Task)
                .limited(5_000),
        )?
        .items;

    let mut closed: Vec<_> = tasks
        .iter()
        .filter_map(|t| match t {
            Entity::Task(task) if !task.status.is_open() => Some(task),
            _ => None,
        })
        .collect();
    // Newest first. `Reverse` rather than a flipped comparator so clippy is
    // satisfied on every platform — and `None` still sorts last, because it is
    // less than any `Some` and reversing makes it greatest.
    closed.sort_by_key(|t| std::cmp::Reverse(t.closed_at));

    writeln!(out, "---")?;
    writeln!(out)?;
    writeln!(out, "## Closed work ({})", closed.len())?;
    writeln!(out)?;

    if closed.is_empty() {
        writeln!(out, "Nothing has closed yet.")?;
        writeln!(out)?;
    }

    let mut current_day: Option<String> = None;
    for task in &closed {
        let day = task
            .closed_at
            .map(|t| t.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "Before close dates were recorded".to_owned());
        if current_day.as_deref() != Some(day.as_str()) {
            writeln!(out, "### {day}")?;
            writeln!(out)?;
            current_day = Some(day);
        }

        // The reason, not the status: `wont_do`, `duplicate`, `superseded` and
        // `no_change` all land in one status, and telling them apart is the
        // whole point of the column existing.
        let reason = task
            .close_reason
            .map_or_else(|| task.status.to_string(), |r| r.to_string());
        writeln!(
            out,
            "- **{}-{}** {} — `{reason}`",
            project.key, task.number, task.title
        )?;

        if let Some(message) = task
            .close_message
            .as_deref()
            .map(str::trim)
            .filter(|m| !m.is_empty())
        {
            writeln!(out)?;
            for line in message.lines() {
                writeln!(out, "  {line}")?;
            }
            writeln!(out)?;
        }

        if !task.evidence.is_empty() {
            writeln!(out, "  <sub>{}</sub>", task.evidence.join(" · "))?;
            writeln!(out)?;
        }
    }

    // --- Every change -----------------------------------------------------
    //
    // Moved here from the tracker unchanged, including the cap and the line
    // that states it. Newest first from the engine: reading the oldest 5,000
    // and reversing was right until the log passed 5,000, at which point the
    // changelog would have frozen — still plausible, describing a week that had
    // scrolled off the top.
    let events = store.recent_events(crate::store::EventScope::Project(project_id), 5_000)?;
    if !events.items.is_empty() {
        writeln!(out, "---")?;
        writeln!(out)?;
        writeln!(out, "## Every change")?;
        writeln!(out)?;
        writeln!(out, "| Date | Actor | Change |")?;
        writeln!(out, "|---|---|---|")?;
        let shown = EVENTS_SHOWN.min(events.items.len());
        for e in events.items.iter().take(shown) {
            writeln!(
                out,
                "| {} | {} | {} |",
                e.created_at.format("%Y-%m-%d"),
                e.actor,
                // `publishable_summary`, never `summary`. This file is
                // committed, and the stored summary of any event written before
                // KEEL-215 still holds the whole prose value that changed —
                // including, in the case that found this, a value somebody had
                // just edited out. Events are immutable, so recomputing at
                // render time is the only thing that covers the ones already in
                // the log.
                e.publishable_summary().replace('|', "\\|")
            )?;
        }
        writeln!(out)?;
        if events.items.len() > shown {
            writeln!(
                out,
                "*Showing the {shown} most recent of {} changes. Use `keel_activity` for the \
                 rest.*",
                events.items.len()
            )?;
            writeln!(out)?;
        }
    }

    Ok(out)
}
