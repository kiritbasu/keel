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
//!   named. This is the *only* definition of blocked: a task is blocked when
//!   something live links to it with `blocks`, and there is no status that can
//!   disagree (TQ-25).
//!
//! Ready work is ordered by **what it unblocks first**, then priority, then
//! the milestone's own order. Unblocking first is deliberate: a p1 that
//! releases three other tasks moves the project further than a p0 that
//! releases nothing, and the count is derived from edges the human already
//! drew rather than from a judgement this module invents.

use crate::{
    Entity, EntityId, EntityQuery, EntityStore, EntityType, GraphStore, Relation, Result,
    TaskStatus,
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
    /// Its readable identifier — `KEEL-42`.
    ///
    /// Carried, rather than left to the caller to compose, because this is the
    /// list a session reads first and the identifier is the thing it will type
    /// back. Every caller would otherwise need the project key to hand.
    pub reference: String,
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

/// The open tasks in a project that something live is linked to as a blocker.
///
/// **The** definition of blocked, and the only one. It used to be two — this,
/// and a `blocked` value in the status column — which could and did disagree:
/// two tasks were marked blocked with nothing linked to them at all. The status
/// is gone (TQ-25) and every count now comes from here, so the app, the digest
/// and the generated tracker cannot report different numbers.
///
/// Only *live* blockers count. A blocker that is finished is not a blocker, and
/// leaving it in would freeze work forever behind something already done.
pub fn blocked_tasks(
    store: &(impl EntityStore + GraphStore),
    project_id: &EntityId,
) -> Result<std::collections::HashSet<EntityId>> {
    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Task)
            .limited(5_000),
    )?;

    // One query for every `blocks` edge in the project, rather than one per
    // open task. The old shape walked the graph from each task in turn, and the
    // digest asks this question three times in a single call — thirty tasks
    // meant ninety traversals plus a `get` per blocker, all under the daemon's
    // one lock. The answer is identical; only the number of round trips
    // changed.
    let edges = store.links_in_project(project_id, Relation::Blocks)?;
    let mut blockers_of: std::collections::HashMap<&EntityId, Vec<&EntityId>> = Default::default();
    for link in &edges {
        blockers_of
            .entry(&link.to_id)
            .or_default()
            .push(&link.from_id);
    }

    // Liveness for the blockers, resolved from what is already loaded wherever
    // possible. Most blockers are tasks in the same project, which this page
    // already holds.
    let known: std::collections::HashMap<&EntityId, &Entity> =
        page.items.iter().map(|e| (e.id(), e)).collect();

    let mut blocked = std::collections::HashSet::new();
    let mut liveness: std::collections::HashMap<EntityId, bool> = Default::default();
    for entity in &page.items {
        let Entity::Task(task) = entity else { continue };
        if is_closed(task.status) {
            continue;
        }

        // Fail closed. `matches!(store.get(…), Ok(Some(e)) if is_live(&e))`
        // read almost identically and treated a storage error as "no live
        // blocker" — so one unreadable link row promoted a genuinely blocked
        // task to ready, in `keel_ready` and in the module the docs call the
        // definition of blocked. That is the silent false-negative this
        // codebase is most afraid of: an answer that looks like work you can
        // start.
        //
        // Unreadable still means blocked. It is the conservative direction: the
        // worst case is a task that stays off the ready list until `fsck`
        // explains why, rather than one an agent picks up and cannot finish.
        let mut has_live_blocker = false;
        for blocker in blockers_of.get(&task.id).map(Vec::as_slice).unwrap_or(&[]) {
            let live = match known.get(*blocker) {
                Some(entity) => is_live(entity),
                None => match liveness.get(*blocker) {
                    Some(cached) => *cached,
                    // A blocker outside this project's task list — a question,
                    // a spec, a task somewhere else. Fetched one at a time,
                    // which is fine because it is the uncommon case and each
                    // answer is remembered for the rest of this pass.
                    None => {
                        let live = match store.get(blocker) {
                            Ok(Some(e)) => is_live(&e),
                            Ok(None) => false,
                            Err(e) => {
                                tracing::warn!(
                                    task = %task.id,
                                    blocker = %blocker,
                                    error = %e,
                                    "could not read a blocker; treating the task as blocked \
                                     rather than reporting it ready. Run `keel fsck`."
                                );
                                true
                            }
                        };
                        liveness.insert((*blocker).clone(), live);
                        live
                    }
                },
            };
            if live {
                has_live_blocker = true;
                break;
            }
        }
        if has_live_blocker {
            blocked.insert(task.id.clone());
        }
    }
    Ok(blocked)
}

/// Rank a project's open work.
pub fn rank(store: &(impl EntityStore + GraphStore), project_id: &EntityId) -> Result<NextUp> {
    // Fetched once for the whole ranking rather than per candidate: the key is
    // the only part of a readable identifier that does not live on the task.
    let key = match store.get(project_id)? {
        Some(Entity::Project(p)) => p.key,
        _ => String::new(),
    };

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

    // Every `blocks` edge in the project, once, rather than two traversals per
    // open task. Same reasoning as `blocked_tasks`, and the same answer.
    let edges = store.links_in_project(project_id, Relation::Blocks)?;
    let mut blockers_of: std::collections::HashMap<&EntityId, Vec<&EntityId>> = Default::default();
    let mut blocks_what: std::collections::HashMap<&EntityId, Vec<&EntityId>> = Default::default();
    for link in &edges {
        blockers_of
            .entry(&link.to_id)
            .or_default()
            .push(&link.from_id);
        blocks_what
            .entry(&link.from_id)
            .or_default()
            .push(&link.to_id);
    }
    let known: std::collections::HashMap<&EntityId, &Entity> =
        page.items.iter().map(|e| (e.id(), e)).collect();

    let mut out = NextUp::default();
    // Blockers from outside the task page, resolved once each rather than once
    // per task that names them.
    let mut fetched: std::collections::HashMap<EntityId, Option<Entity>> = Default::default();

    for task in &open {
        // Inbound `blocks` edges — what is stopping this. Only live blockers
        // count: a blocker that is finished is not a blocker, and leaving it
        // in would freeze work forever behind something already done.
        //
        // Fail closed here too, for the same reason as `blocked_tasks`: an
        // unreadable blocker becomes a named blocker rather than no blocker,
        // so this task stays off the ready list. The label says why, because a
        // blocked task with no visible reason is its own kind of dead end.
        let mut blockers: Vec<String> = Vec::new();
        for blocker in blockers_of.get(&task.id).map(Vec::as_slice).unwrap_or(&[]) {
            if let Some(entity) = known.get(*blocker) {
                if is_live(entity) {
                    blockers.push(entity.label().to_owned());
                }
                continue;
            }
            if !fetched.contains_key(*blocker) {
                let resolved = match store.get(blocker) {
                    Ok(found) => found,
                    Err(e) => {
                        tracing::warn!(
                            task = %task.id,
                            blocker = %blocker,
                            error = %e,
                            "could not read a blocker while ranking"
                        );
                        blockers.push(format!("{blocker} (unreadable — run `keel fsck`)"));
                        continue;
                    }
                };
                fetched.insert((*blocker).clone(), resolved);
            }
            if let Some(Some(entity)) = fetched.get(*blocker)
                && is_live(entity)
            {
                blockers.push(entity.label().to_owned());
            }
        }

        // Outbound `blocks` edges to work that is still open — what finishing
        // this would release.
        let unblocks = blocks_what
            .get(&task.id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter(|id| open_ids.contains(**id))
            .count();

        let priority = task.priority.as_str().to_owned();
        let waiting = task.labels.iter().any(|l| l == DECISION_LABEL);

        if !blockers.is_empty() {
            out.blocked.push(Candidate {
                id: task.id.clone(),
                reference: format!("{key}-{}", task.number),
                title: task.title.clone(),
                priority,
                unblocks,
                why: format!("blocked by {}", join_names(&blockers)),
            });
        } else if waiting {
            out.waiting_on_you.push(Candidate {
                id: task.id.clone(),
                reference: format!("{key}-{}", task.number),
                title: task.title.clone(),
                priority,
                unblocks,
                why: "a decision, not work — nothing can start until it is made".to_owned(),
            });
        } else {
            out.ready.push(Candidate {
                id: task.id.clone(),
                reference: format!("{key}-{}", task.number),
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

/// What to narrow `keel ready` to.
///
/// Every field is "no opinion" by default, so an empty filter is the whole ready
/// list. That matters for the promise the three surfaces make: the CLI, the tool
/// and the app all call [`ready`], and a default that meant something would be a
/// fourth answer nobody asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReadyFilter {
    /// Only tasks nobody is holding.
    pub unclaimed: bool,
    /// Only tasks carrying all of these labels.
    pub labels: Vec<String>,
    /// Exclude tasks carrying any of these labels.
    pub without_labels: Vec<String>,
    /// Only tasks under this milestone — "what is next in Phase 8".
    pub milestone: Option<EntityId>,
    /// How many to return. `None` means every one of them.
    pub limit: Option<usize>,
}

/// The ready list, and what it left out.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ready {
    /// Pick these up, best first.
    pub items: Vec<Candidate>,
    /// How many were ready before the limit was applied.
    pub total: usize,
    /// Whether the limit cut the list.
    ///
    /// Reported rather than inferred from `items.len() == limit`, because
    /// hard constraint 4 is that no list is ever silently truncated: a caller
    /// who sees ten of ten has no way to tell that from ten of ninety.
    pub truncated: bool,
}

/// What can be worked on right now.
///
/// Open, nothing live blocking it, and not a parent — because a parent's
/// children are the actual work, and offering both puts the same job in the list
/// twice with the vaguer of the two ranked higher.
///
/// The ranking is [`rank`]'s, unchanged: most-unblocking first, then priority.
/// This adds the front door and the filters, and it is the one computation the
/// CLI, the MCP tool and the app all read — which is what stops the app showing
/// a different answer from the one a session was given.
pub fn ready(
    store: &(impl EntityStore + GraphStore),
    project_id: &EntityId,
    filter: &ReadyFilter,
) -> Result<Ready> {
    let ranked = rank(store, project_id)?;

    // Parents are excluded by id, which needs the whole task list rather than
    // only the ranked one: a parent whose children are all done is still a
    // parent, and its child may be closed and therefore absent from `ranked`.
    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Task)
            .limited(5_000),
    )?;
    let parents: std::collections::HashSet<EntityId> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Task(t) => t.parent_id.clone(),
            _ => None,
        })
        .collect();
    let by_id: std::collections::HashMap<&EntityId, &crate::Task> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Task(t) => Some((&t.id, t)),
            _ => None,
        })
        .collect();

    let now = chrono::Utc::now();
    let mut items: Vec<Candidate> = Vec::new();
    for candidate in ranked.ready {
        let Some(task) = by_id.get(&candidate.id) else {
            continue;
        };
        if parents.contains(&candidate.id) {
            continue;
        }
        if filter.unclaimed && task.claim_is_live(now) {
            continue;
        }
        if !filter
            .labels
            .iter()
            .all(|want| task.labels.iter().any(|l| l == want))
        {
            continue;
        }
        if filter
            .without_labels
            .iter()
            .any(|skip| task.labels.iter().any(|l| l == skip))
        {
            continue;
        }
        if let Some(milestone) = &filter.milestone
            && task.milestone_id.as_ref() != Some(milestone)
        {
            continue;
        }
        items.push(candidate);
    }

    let total = items.len();
    let truncated = match filter.limit {
        Some(limit) if total > limit => {
            items.truncate(limit);
            true
        }
        _ => false,
    };

    Ok(Ready {
        items,
        total,
        truncated,
    })
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
