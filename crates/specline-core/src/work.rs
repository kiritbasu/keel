//! Taking a task and finishing one.
//!
//! Two verbs that already existed as instructions in `product/CLAUDE.md` and
//! nowhere else. "Move a task to `in_progress` before starting it" is a rule a
//! session has to remember, and across sixty-six tasks the number of
//! transitions into that state before work began was **zero**. "A task is done
//! only when it meets the definition of done" is a seven-item checklist an agent
//! is asked to honour.
//!
//! Both live here so the CLI and MCP call the same code rather than two
//! implementations that agree until they do not. The rule that closing states a
//! reason is enforced deeper still — in [`crate::EntityStore::update`] — so even
//! a caller who moves the status by hand cannot skip it.
//!
//! # Why there is no lock
//!
//! Claiming is atomic because it goes through the ordinary optimistic-concurrency
//! update with the version it read. Two sessions racing for the same task both
//! read version 7; the first writes version 8 and the second is rejected with a
//! stale-version error naming the current state. That is the same mechanism
//! every other write uses, and adding a lock would mean a second answer to a
//! question the store already answers.

use crate::{
    CloseReason, Entity, EntityId, EntityStore, EntityType, Error, NewLink, Provenance, Relation,
    Result, Task, TaskStatus,
};
use chrono::Utc;

/// What a claim did.
#[derive(Debug, Clone)]
pub struct Claimed {
    /// The task, now `in_progress` and carrying the claim.
    pub task: Task,
    /// The session whose claim was taken over, when one was.
    ///
    /// Reported rather than silently overwritten. A claim reused because it went
    /// stale is a fact the caller wants in its output — the other session may
    /// still be alive and simply slow.
    pub took_over_from: Option<String>,
}

/// Take a task, recording who is working on it and when they started.
///
/// Refuses a task somebody else is holding, unless the claim has gone stale
/// (see [`Task::claim_is_live`]) or `force` is passed. Re-claiming your own task
/// is a no-op rather than an error, so a session that retries is not punished
/// for it.
///
/// A claim needs a session id, and this is the one place that refuses a write
/// for want of one. Everywhere else an anonymous write is merely less traceable;
/// here the session *is* the content — a claim naming nobody excludes the task
/// from `specline ready --unclaimed` while telling no one who to ask about it.
pub fn claim(
    store: &mut impl EntityStore,
    id: &EntityId,
    force: bool,
    provenance: &Provenance,
) -> Result<Claimed> {
    let Some(session) = provenance.session_id.clone() else {
        return Err(Error::invalid(
            EntityType::Task,
            "session_id",
            "a claim has to name the session doing the work, and this call carried none",
            "pass the session id you were given for this conversation — Specline never invents \
             one, so a claim without it would say that the task is taken and not by whom",
        ));
    };

    let task = read_task(store, id)?;

    if task.status == TaskStatus::Done || task.status == TaskStatus::WontDo {
        return Err(Error::invalid(
            EntityType::Task,
            "status",
            format!(
                "{} is already closed ({})",
                task.title,
                task.close_reason
                    .map_or_else(|| task.status.as_str().to_owned(), |r| r.to_string())
            ),
            "reopen it with specline_update first if the work turned out not to be finished",
        ));
    }

    let now = Utc::now();
    let held_by = task.claimed_by.clone().filter(|_| task.claim_is_live(now));
    let mut took_over_from = None;

    match held_by {
        Some(holder) if holder == session => {
            // Already ours. Nothing to write, and nothing to complain about.
            return Ok(Claimed {
                task,
                took_over_from: None,
            });
        }
        Some(holder) if !force => {
            return Err(Error::invalid(
                EntityType::Task,
                "claimed_by",
                format!(
                    "{} is claimed by session {holder}, since {}",
                    task.title,
                    task.claimed_at
                        .map_or_else(|| "an unrecorded time".to_owned(), |at| at.to_rfc3339())
                ),
                format!(
                    "pick something else from specline_next, or pass force to take it over. A \
                     claim releases itself after {} days, and closing the task releases it \
                     immediately.",
                    crate::CLAIM_STALE_AFTER.num_days()
                ),
            ));
        }
        Some(holder) => took_over_from = Some(holder),
        None => {
            // Unclaimed, or claimed by a session that has gone quiet for long
            // enough that the claim no longer holds anything.
            if task.claimed_by.is_some() {
                took_over_from = task.claimed_by.clone();
            }
        }
    }

    let changes = serde_json::json!({
        "status": TaskStatus::InProgress.as_str(),
        "claimed_by": session,
        "claimed_at": now,
    });
    let Some(changes) = changes.as_object() else {
        return Err(Error::Invariant {
            operation: format!("claim {id}"),
            problem: "the claim did not serialise to an object".to_owned(),
        });
    };

    let updated = store.update(id, task.audit.version, changes, provenance)?;
    Ok(Claimed {
        task: expect_task(updated, id)?,
        took_over_from,
    })
}

/// What a close did.
#[derive(Debug, Clone)]
pub struct Closed {
    /// The task in its terminal state.
    pub task: Task,
    /// The edge this close drew, if the reason named another task.
    pub linked: Option<(Relation, EntityId)>,
}

/// Everything a close has to say.
///
/// A struct rather than five arguments because the whole point of the task is
/// that the reason, the message and the evidence arrive *together* — as one
/// argument list a caller cannot half-fill.
#[derive(Debug, Clone)]
pub struct Close {
    /// Which of the five reasons.
    pub reason: CloseReason,
    /// What happened, in a sentence or two.
    pub message: String,
    /// Typed proof: `commit:<sha>`, `pr:<url>`, `test:<command>`, and so on.
    pub evidence: Vec<String>,
    /// The task this one duplicates or is superseded by.
    ///
    /// Required for those two reasons and meaningless for the other three,
    /// which is checked rather than documented: "closed as a duplicate" with
    /// nothing named is the same dead end as no reason at all.
    pub other: Option<EntityId>,
}

/// Close a task, stating why.
///
/// The reason, the message and — for `done` — the evidence are validated in the
/// storage layer, so this function cannot be the only thing standing between a
/// caller and an unexplained close. What it adds on top is the automatic edge:
/// `duplicate` draws `duplicates` and `superseded` draws `supersedes`, so
/// "a duplicate of what?" is answerable by the graph rather than by reading the
/// message.
pub fn close(
    store: &mut (impl EntityStore + crate::GraphStore),
    id: &EntityId,
    close: &Close,
    provenance: &Provenance,
) -> Result<Closed> {
    let task = read_task(store, id)?;

    let relation = close.reason.relation();
    match (relation, &close.other) {
        (Some(rel), None) => {
            return Err(Error::invalid(
                EntityType::Task,
                "other",
                format!("closing as `{}` has to name the other task", close.reason),
                format!(
                    "the task this one {} — `KEEL-42` or a `tsk_…` id",
                    match rel {
                        Relation::Duplicates => "duplicates",
                        _ => "was superseded by",
                    }
                ),
            ));
        }
        (None, Some(other)) => {
            return Err(Error::invalid(
                EntityType::Task,
                "other",
                format!(
                    "closing as `{}` does not name another task, but {other} was given",
                    close.reason
                ),
                "use `duplicate` or `superseded` if this closes in favour of another task, \
                 or drop the argument",
            ));
        }
        _ => {}
    }

    if let Some(other) = &close.other
        && other == id
    {
        return Err(Error::invalid(
            EntityType::Task,
            "other",
            "a task cannot be a duplicate of itself",
            "name the task that keeps the history",
        ));
    }

    let changes = serde_json::json!({
        "status": close.reason.status().as_str(),
        "close_reason": close.reason.as_str(),
        "close_message": close.message,
        "evidence": close.evidence,
    });
    let Some(changes) = changes.as_object() else {
        return Err(Error::Invariant {
            operation: format!("close {id}"),
            problem: "the close did not serialise to an object".to_owned(),
        });
    };

    let updated = store.update(id, task.audit.version, changes, provenance)?;

    // The edge is drawn after the status lands, not before. A close that is
    // going to be refused should not leave an edge behind claiming a
    // relationship that was never recorded — the same ordering the image path
    // uses for the same reason.
    let mut linked = None;
    if let (Some(rel), Some(other)) = (relation, &close.other) {
        store.link(NewLink::new(id.clone(), rel, other.clone()), provenance)?;
        linked = Some((rel, other.clone()));
    }

    Ok(Closed {
        task: expect_task(updated, id)?,
        linked,
    })
}

/// Read a task, or say what was found instead.
fn read_task(store: &impl EntityStore, id: &EntityId) -> Result<Task> {
    let entity = store.get(id)?.ok_or_else(|| Error::NotFound {
        entity_type: id.entity_type(),
        id: id.to_string(),
    })?;
    expect_task(entity, id)
}

/// Narrow an entity to a task.
fn expect_task(entity: Entity, id: &EntityId) -> Result<Task> {
    let entity_type = entity.entity_type();
    match entity {
        Entity::Task(t) => Ok(t),
        _ => Err(Error::invalid(
            entity_type,
            "id",
            format!("{id} is a {entity_type}, and only a task can be claimed or closed"),
            "a `tsk_…` id or a readable task identifier such as `KEEL-42`",
        )),
    }
}
