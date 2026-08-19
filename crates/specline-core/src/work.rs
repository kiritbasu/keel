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
    CloseReason, Document, DocumentStore, Entity, EntityId, EntityStore, EntityType, Error,
    Feedback, NewLink, Provenance, Relation, Result, SpecKind, Task, TaskStatus,
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

/// What triage decided about a signal.
///
/// Two outcomes, and the words are chosen rather than inherited. *Picked up*
/// and *set down* say what actually happens: a signal that is set down is not
/// destroyed, its argument is written where search will find it, and it can be
/// picked up again when the same idea arrives in four months. "Rejected" would
/// make it sound like the tombstone it deliberately is not, which is the
/// property B-90 turns on and the one thing no ticketing system has.
#[derive(Debug, Clone)]
pub enum TriageOutcome {
    /// It becomes a feature. The named spec carries the why.
    PickedUp {
        /// A `spc_…` whose kind is `feature`.
        feature: EntityId,
    },
    /// Not this, and here is the argument.
    SetDown {
        /// Why, in enough words to be worth finding later.
        reason: String,
    },
    /// The same want, already recorded. The other signal keeps the history.
    ///
    /// Distinct from setting down, and the difference is not pedantry: a
    /// duplicate is evidence that more than one person wants the thing, which
    /// is the demand signal the Inbox has no other way to carry. Collapsing it
    /// into a set-down would record "we said no to this twice" where the truth
    /// is "two people asked".
    Duplicate {
        /// The `fbk_…` that keeps the history.
        of: EntityId,
    },
}

/// What a triage did.
#[derive(Debug, Clone)]
pub struct Triaged {
    /// The signal, now out of the Inbox.
    pub signal: Feedback,
    /// The `derived_from` edge a pick-up drew.
    pub linked: Option<(Relation, EntityId)>,
    /// The revision a set-down wrote.
    pub revision: Option<Document>,
}

/// The shortest set-down reason worth storing.
///
/// Not a style rule. B-91 lets the reasoning live on the signal rather than in
/// the decision log *on the grounds that it stays findable*, and "no" is not
/// findable — nobody searches for it and it answers nothing when found. The
/// number is deliberately low: the bar is "a sentence", not "an essay".
const SHORTEST_USEFUL_REASON: usize = 20;

/// Triage a signal: pick it up, or set it down with the argument.
///
/// The invariant this exists to enforce is that **a signal cannot leave the
/// Inbox without an outcome**. Marking `triaged` through the ordinary update
/// path would let a signal be cleared with nothing recorded, which is exactly
/// the silent loss B-90 is built to prevent — so this is the way, and it is
/// checked here rather than asked for.
///
/// A set-down **appends** to the body rather than replacing it. The signal's
/// body is the verbatim, and overwriting it with the reason for setting it
/// down would destroy what somebody actually said in the act of recording why
/// we are not doing it. Revisions keep the original either way; appending
/// means the *current* text — the one search reads — carries both.
pub fn triage(
    store: &mut (impl EntityStore + crate::GraphStore + DocumentStore),
    id: &EntityId,
    outcome: &TriageOutcome,
    provenance: &Provenance,
) -> Result<Triaged> {
    let signal = read_signal(store, id)?;

    if signal.triaged {
        return Err(Error::invalid(
            EntityType::Feedback,
            "triaged",
            format!("{id} has already been triaged"),
            "read what it became before triaging it again — a second outcome would replace \
             the first and the first is the one somebody reasoned about",
        ));
    }

    let mut linked = None;
    let mut revision = None;

    match outcome {
        TriageOutcome::PickedUp { feature } => {
            let entity = store.get(feature)?.ok_or_else(|| Error::NotFound {
                entity_type: feature.entity_type(),
                id: feature.to_string(),
            })?;
            // A feature and not merely a spec, because "picked up" means an
            // argued case exists. Pointing at a PRD or a design doc would
            // record an outcome that does not say why.
            match &entity {
                Entity::Spec(spec) if spec.kind == SpecKind::Feature => {}
                Entity::Spec(spec) => {
                    return Err(Error::invalid(
                        EntityType::Feedback,
                        "feature",
                        format!("{feature} is a `{}` spec, not a `feature`", spec.kind),
                        "the feature spec holding the case for building this — create one with \
                         kind `feature`, or set the signal down instead",
                    ));
                }
                other => {
                    return Err(Error::invalid(
                        EntityType::Feedback,
                        "feature",
                        format!("{feature} is a {}, not a spec", other.entity_type()),
                        "a `spc_…` whose kind is `feature`",
                    ));
                }
            }
            store.link(
                NewLink::new(feature.clone(), Relation::DerivedFrom, id.clone()),
                provenance,
            )?;
            linked = Some((Relation::DerivedFrom, feature.clone()));
        }
        TriageOutcome::Duplicate { of } => {
            if of == id {
                return Err(Error::invalid(
                    EntityType::Feedback,
                    "of",
                    "a signal cannot be a duplicate of itself".to_owned(),
                    "the signal that keeps the history",
                ));
            }
            let other = read_signal(store, of)?;
            if other.triaged {
                // Pointing a live signal at one already dealt with would put
                // the want on a row nobody will look at again, which is the
                // opposite of what recording a duplicate is for.
                return Err(Error::invalid(
                    EntityType::Feedback,
                    "of",
                    format!("{of} has already been triaged"),
                    "an untriaged signal — the one still carrying the want",
                ));
            }
            store.link(
                NewLink::new(id.clone(), Relation::Duplicates, of.clone()),
                provenance,
            )?;
            linked = Some((Relation::Duplicates, of.clone()));
        }
        TriageOutcome::SetDown { reason } => {
            let reason = reason.trim();
            if reason.chars().count() < SHORTEST_USEFUL_REASON {
                return Err(Error::invalid(
                    EntityType::Feedback,
                    "reason",
                    "a set-down needs an argument, not a word".to_owned(),
                    "why this is not worth doing, in a sentence somebody will find useful when \
                     the same idea arrives again in four months",
                ));
            }

            let current = store.revision(id, None)?;
            let title = current
                .as_ref()
                .map_or_else(|| signal.summary.clone(), |d| d.title.clone());
            let body = match &current {
                Some(existing) => format!("{}\n\n---\n\n**Set down.** {reason}", existing.body),
                None => format!("**Set down.** {reason}"),
            };
            revision = Some(store.write_revision(Document::revision(
                EntityType::Feedback,
                id.clone(),
                Some(signal.project_id.clone()),
                title,
                body,
                provenance.actor,
                Utc::now(),
                current.as_ref().map(|d| d.version),
            )?)?);
        }
    }

    let changes = serde_json::json!({ "triaged": true });
    let Some(changes) = changes.as_object() else {
        return Err(Error::Invariant {
            operation: format!("triage {id}"),
            problem: "the triage did not serialise to an object".to_owned(),
        });
    };
    // Read the version again rather than reusing the one from the top: a
    // set-down has written a revision since, which bumps the row.
    let version = read_signal(store, id)?.audit.version;
    let updated = store.update(id, version, changes, provenance)?;

    Ok(Triaged {
        signal: expect_signal(updated, id)?,
        linked,
        revision,
    })
}

/// Read a signal, or say what was found instead.
fn read_signal(store: &impl EntityStore, id: &EntityId) -> Result<Feedback> {
    let entity = store.get(id)?.ok_or_else(|| Error::NotFound {
        entity_type: id.entity_type(),
        id: id.to_string(),
    })?;
    expect_signal(entity, id)
}

/// Narrow an entity to a signal.
fn expect_signal(entity: Entity, id: &EntityId) -> Result<Feedback> {
    let entity_type = entity.entity_type();
    match entity {
        Entity::Feedback(f) => Ok(f),
        _ => Err(Error::invalid(
            entity_type,
            "id",
            format!("{id} is a {entity_type}, and only a signal can be triaged"),
            "a `fbk_…` id — the thing somebody said, not what it became",
        )),
    }
}

/// Close a signal: the same verb as closing a task, applied to a want.
///
/// B-94. "This is dealt with, here is why, and here is the proof" is one
/// sentence for both, and `close` already enforces exactly what triage needs —
/// a reason, a message on every reason, evidence on `done` — in the storage
/// layer where the CLI and MCP cannot disagree. So triage reaches MCP without
/// a fourteenth tool.
///
/// This **translates and delegates**. [`triage`] stays the only path a signal
/// can leave the Inbox by, so there is one set of invariants rather than two
/// that have to be kept in step.
///
/// Three of the five reasons mean something about a want:
///
/// - `done` — picked up. Evidence names the feature spec, the same demand
///   `done` already makes of a task: show the thing that proves it.
/// - `wont_do` — set down. The message is the argument.
/// - `duplicate` — the same want, already recorded.
///
/// `superseded` and `no_change` are refused. A signal is not replaced by a
/// later signal the way a decision is by a later decision, and "nothing
/// changed" describes work rather than an idea.
pub fn close_signal(
    store: &mut (impl EntityStore + crate::GraphStore + DocumentStore),
    id: &EntityId,
    close: &Close,
    provenance: &Provenance,
) -> Result<Triaged> {
    let outcome = match close.reason {
        CloseReason::Done => TriageOutcome::PickedUp {
            feature: feature_from_evidence(&close.evidence)?,
        },
        // The message is already required for every reason, so a set-down
        // arriving through this door cannot be reasonless — `triage` still
        // checks it is more than a word.
        CloseReason::WontDo => TriageOutcome::SetDown {
            reason: close.message.clone(),
        },
        CloseReason::Duplicate => {
            let of = close.other.clone().ok_or_else(|| {
                Error::invalid(
                    EntityType::Feedback,
                    "other",
                    "closing a signal as `duplicate` has to name the other signal".to_owned(),
                    "the `fbk_…` that keeps the history",
                )
            })?;
            if of.entity_type() != EntityType::Feedback {
                return Err(Error::invalid(
                    EntityType::Feedback,
                    "other",
                    format!("{of} is a {}, not a signal", of.entity_type()),
                    "a `fbk_…` — two people asking for the same thing, not a task",
                ));
            }
            TriageOutcome::Duplicate { of }
        }
        reason @ (CloseReason::Superseded | CloseReason::NoChange) => {
            return Err(Error::invalid(
                EntityType::Feedback,
                "reason",
                format!("`{reason}` does not describe what happens to a signal"),
                "`done` if it became a feature, `wont_do` to set it down with the argument, \
                 or `duplicate` if somebody had already asked",
            ));
        }
    };

    triage(store, id, &outcome, provenance)
}

/// Pull the feature spec out of a close's evidence.
///
/// `done` on a task means "show me the commit"; on a signal it means "show me
/// the case you made". Reusing evidence rather than adding an argument keeps
/// the two closes one shape, and it means the feature is recorded where
/// anybody reading the closed signal already looks.
fn feature_from_evidence(evidence: &[String]) -> Result<EntityId> {
    let wanted = "the feature spec this became, as `doc:spc_…`";
    let specs: Vec<&str> = evidence
        .iter()
        .filter_map(|e| e.strip_prefix("doc:"))
        .filter(|v| v.starts_with("spc_"))
        .collect();

    match specs.as_slice() {
        [one] => EntityId::parse_as(one, EntityType::Spec),
        [] => Err(Error::invalid(
            EntityType::Feedback,
            "evidence",
            "closing a signal as `done` means it became a feature, and none is named".to_owned(),
            wanted,
        )),
        // Two would leave "which one did this become?" unanswerable, and the
        // `derived_from` edge can only point at one.
        many => Err(Error::invalid(
            EntityType::Feedback,
            "evidence",
            format!(
                "{} feature specs are named, and a signal becomes one",
                many.len()
            ),
            wanted,
        )),
    }
}
