//! What to do next.
//!
//! The one question a project spine exists to answer, and until TQ-16 the one
//! it did not. `keel_context` used to return counts and advice — "3 task(s)
//! are blocked, check what is blocking them" — which restates the problem
//! rather than answering it, and left both an agent and a human to work out
//! the ordering themselves from a board with no ordering in it.
//!
//! # The ranking
//!
//! Three buckets, because they need different responses and mixing them is
//! most of why the board was unreadable:
//!
//! - **Ready** — nothing is blocking it, so it can be picked up now.
//! - **Waiting on a human** — a decision someone has to make. Real work
//!   cannot start on it, and it should not sit in the same list competing
//!   with things that can.
//! - **Blocked** — something concrete is in the way, and that something is
//!   named. A task marked `blocked` with nothing blocking it is reported as
//!   its own problem rather than silently ranked.
//!
//! Ready work is ordered by **what it unblocks first**, then priority, then
//! the milestone's own order. Unblocking first is deliberate: a p1 that
//! releases three other tasks moves the project further than a p0 that
//! releases nothing, and the count is derived from edges the human already
//! drew rather than from a judgement this module invents.

use crate::{
    Direction, Entity, EntityId, EntityQuery, EntityStore, EntityType, GraphStore, Relation,
    Result, TaskStatus,
};

/// The label that marks a task as a decision someone has to make.
///
/// A convention rather than a column. The alternative considered was a new
/// `TaskKind`, which is a schema change to express something a label already
/// expresses, and `product/CLAUDE.md` is explicit that a new type or field is
/// almost always the wrong answer to an awkward modelling problem.
pub const DECISION_LABEL: &str = "decision-needed";

/// One candidate, with the reason it is where it is.
///
/// The reason is carried rather than recomputed by callers, because the digest
/// and the desktop app would otherwise word it differently and drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The task.
    pub id: EntityId,
    /// Its title.
    pub title: String,
    /// `p0`…`p3`.
    pub priority: String,
    /// How many open tasks this one is blocking.
    pub unblocks: usize,
    /// One line on why it is ranked here, or what is in the way.
    pub why: String,
}

/// What is ready, what is waiting on a person, and what is stuck.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NextUp {
    /// Pick these up, best first.
    pub ready: Vec<Candidate>,
    /// Decisions a human owes the project.
    pub waiting_on_you: Vec<Candidate>,
    /// Stuck, with the blocker named.
    pub blocked: Vec<Candidate>,
}

impl NextUp {
    /// Whether there is anything at all to report.
    pub fn is_empty(&self) -> bool {
        self.ready.is_empty() && self.waiting_on_you.is_empty() && self.blocked.is_empty()
    }
}

/// Rank a project's open work.
pub fn rank(store: &(impl EntityStore + GraphStore), project_id: &EntityId) -> Result<NextUp> {
    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Task)
            .limited(5_000),
    )?;

    // Open means "still wants doing". `review` counts: it is waiting on a
    // person to confirm, not finished.
    let open: Vec<_> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Task(t) if !is_closed(t.status) => Some(t),
            _ => None,
        })
        .collect();
    let open_ids: std::collections::HashSet<&EntityId> = open.iter().map(|t| &t.id).collect();

    let mut out = NextUp::default();

    for task in &open {
        // Inbound `blocks` edges — what is stopping this. Only live blockers
        // count: a blocker that is finished is not a blocker, and leaving it
        // in would freeze work forever behind something already done.
        let blockers: Vec<String> = store
            .neighbours(&task.id, Direction::Inbound, &[Relation::Blocks], 1)?
            .into_iter()
            .filter_map(|n| match store.get(&n.id) {
                Ok(Some(e)) if is_live(&e) => Some(e.label().to_owned()),
                _ => None,
            })
            .collect();

        // Outbound `blocks` edges to work that is still open — what finishing
        // this would release.
        let unblocks = store
            .neighbours(&task.id, Direction::Outbound, &[Relation::Blocks], 1)?
            .into_iter()
            .filter(|n| open_ids.contains(&n.id))
            .count();

        let priority = task.priority.as_str().to_owned();
        let waiting = task.labels.iter().any(|l| l == DECISION_LABEL);

        if !blockers.is_empty() {
            out.blocked.push(Candidate {
                id: task.id.clone(),
                title: task.title.clone(),
                priority,
                unblocks,
                why: format!("blocked by {}", join_names(&blockers)),
            });
        } else if task.status == TaskStatus::Blocked {
            // Marked blocked, nothing blocking it. Reported rather than
            // ranked: the honest answer is that the data is wrong, and
            // silently treating it as ready would hide that.
            out.blocked.push(Candidate {
                id: task.id.clone(),
                title: task.title.clone(),
                priority,
                unblocks,
                why: "marked blocked, but nothing links to it with `blocks` — either link the \
                      blocker or move it out of blocked"
                    .to_owned(),
            });
        } else if waiting {
            out.waiting_on_you.push(Candidate {
                id: task.id.clone(),
                title: task.title.clone(),
                priority,
                unblocks,
                why: "a decision, not work — nothing can start until it is made".to_owned(),
            });
        } else {
            out.ready.push(Candidate {
                id: task.id.clone(),
                title: task.title.clone(),
                priority,
                unblocks,
                why: reason(unblocks, task.priority.as_str()),
            });
        }
    }

    out.ready.sort_by(|a, b| {
        b.unblocks
            .cmp(&a.unblocks)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    out.waiting_on_you.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });
    out.blocked.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| a.id.as_str().cmp(b.id.as_str()))
    });

    Ok(out)
}

/// Why a ready task sits where it does.
fn reason(unblocks: usize, priority: &str) -> String {
    match (unblocks, priority) {
        (0, p) => format!("nothing is blocking it · {p}"),
        (1, p) => format!("unblocks 1 other task · {p}"),
        (n, p) => format!("unblocks {n} other tasks · {p}"),
    }
}

/// Join blocker names for a one-line reason, without a trailing comma soup.
fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => format!("“{one}”"),
        [one, two] => format!("“{one}” and “{two}”"),
        [one, rest @ ..] => format!("“{one}” and {} other(s)", rest.len()),
    }
}

/// Whether a task is finished as far as ranking is concerned.
fn is_closed(status: TaskStatus) -> bool {
    matches!(status, TaskStatus::Done | TaskStatus::WontDo)
}

/// Whether a blocker is still standing.
///
/// A done task and an answered question stop blocking. Anything else — a spec,
/// a decision, an open question — still counts, because there is no general
/// notion of a "finished" spec and assuming one would quietly release work
/// that is genuinely waiting.
fn is_live(entity: &Entity) -> bool {
    if entity.audit().archived_at.is_some() {
        return false;
    }
    match entity {
        Entity::Task(t) => !is_closed(t.status),
        Entity::Question(q) => q.status.is_unresolved(),
        _ => true,
    }
}
