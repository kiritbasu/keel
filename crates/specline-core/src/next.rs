//! What to do next.
//!
//! The one question a project spine exists to answer, and until TQ-16 the one
//! it did not. `specline_context` used to return counts and advice — "3 task(s)
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
    Entity, EntityId, EntityQuery, EntityStore, EntityType, GraphStore, Relation, Result, TaskKind,
    TaskStatus,
};
use chrono::{DateTime, Utc};

/// The label that marks a task as a decision someone has to make.
///
/// A convention rather than a column. The alternative considered was a new
/// `TaskKind`, which is a schema change to express something a label already
/// expresses, and `product/CLAUDE.md` is explicit that a new type or field is
/// almost always the wrong answer to an awkward modelling problem.
pub const DECISION_LABEL: &str = "decision-needed";

/// Which bucket a ready task belongs in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadyGroup {
    /// In a milestone that is currently open. The only signal carrying a
    /// person's intent about what matters now.
    Active,
    /// A bug, in no active milestone. Broken beats unbuilt.
    Bug,
    /// Everything else, oldest first.
    Rest,
}

impl ReadyGroup {
    /// The word the CLI and the API use.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Bug => "bug",
            Self::Rest => "rest",
        }
    }
}

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
    /// Which bucket this belongs in: `active`, `bug`, or `rest`.
    ///
    /// Ready used to render one numbered list of everything, which implied an
    /// ordering the inputs could not support — measured on this store,
    /// `unblocks` was 0 on all 29 open tasks and priority was p2 on 21 of them
    /// (B-83). Grouping lets the page be honest about where the real judgement
    /// is, which is which group leads rather than which row is 14th.
    pub group: ReadyGroup,
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
        // task to ready, in `specline_next` and in the module the docs call the
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
                                     rather than reporting it ready. Run `specline fsck`."
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
    // Read once for the whole pass. Calling `now` per task would let two rows
    // created a second apart fall into different day buckets depending on when
    // the loop reached them.
    let now = Utc::now();
    // Which milestones are open, by id. This is the only signal on a task that
    // carries a person's intent about what matters now, so it decides the
    // groups — see B-83 for why `blocks` and priority could not.
    let active_milestones: std::collections::HashSet<EntityId> = store
        .list(
            &crate::EntityQuery::default()
                .of_type(EntityType::Milestone)
                .limited(1_000),
        )?
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Milestone(m) if m.status == crate::MilestoneStatus::Open => Some(m.id.clone()),
            _ => None,
        })
        .collect();
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
                        blockers.push(format!("{blocker} (unreadable — run `specline fsck`)"));
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
                group: ReadyGroup::Rest,
                why: format!("blocked by {}", join_names(&blockers)),
            });
        } else if waiting {
            out.waiting_on_you.push(Candidate {
                id: task.id.clone(),
                reference: format!("{key}-{}", task.number),
                title: task.title.clone(),
                priority,
                unblocks,
                group: ReadyGroup::Rest,
                why: "a decision, not work — nothing can start until it is made".to_owned(),
            });
        } else {
            let in_active = task
                .milestone_id
                .as_ref()
                .is_some_and(|m| active_milestones.contains(m));
            let group = if in_active {
                ReadyGroup::Active
            } else if task.kind == TaskKind::Bug {
                ReadyGroup::Bug
            } else {
                ReadyGroup::Rest
            };
            let why = reason(
                unblocks,
                group,
                days_waiting(task.audit.created_at, now),
                task.priority.as_str(),
            );
            out.ready.push(Candidate {
                id: task.id.clone(),
                reference: format!("{key}-{}", task.number),
                title: task.title.clone(),
                priority,
                unblocks,
                group,
                why,
            });
        }
    }

    out.ready.sort_by(|a, b| {
        // `unblocks` still leads, for the stores where it means something. On
        // this one it is 0 everywhere, so the group is what actually orders the
        // list (B-83).
        //
        // The last word is the id, and that is oldest-first rather than an
        // arbitrary tiebreak: entity ids are ULIDs, so ordering by id is
        // ordering by creation time — the same property the event log relies on
        // to answer "what changed since T" with a range scan. The screen says
        // "oldest first" on the strength of this line, so if ids ever stop
        // being time-ordered, that label becomes a lie and this needs a
        // `created_at` to sort on instead.
        b.unblocks
            .cmp(&a.unblocks)
            .then_with(|| group_rank(a.group).cmp(&group_rank(b.group)))
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

/// What to narrow `specline ready` to.
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

/// The priority every task gets unless someone says otherwise. Shown in a
/// reason only when it differs, since a value on every row explains nothing.
const DEFAULT_PRIORITY: &str = "p2";

/// Which group comes first.
const fn group_rank(group: ReadyGroup) -> u8 {
    match group {
        ReadyGroup::Active => 0,
        ReadyGroup::Bug => 1,
        ReadyGroup::Rest => 2,
    }
}

/// Whole days between two instants, floored at zero.
fn days_waiting(created: DateTime<Utc>, now: DateTime<Utc>) -> i64 {
    (now - created).num_days().max(0)
}

/// Why a ready task sits where it does.
///
/// This replaced `nothing is blocking it · p2`, which was true of every row on
/// the page — everything in Ready is unblocked, that being what Ready means —
/// so it was the definition of the page repeated 29 times and told a reader
/// nothing about the order. A reason has to differ between rows or it is not a
/// reason, and the test beside this one asserts exactly that.
fn reason(unblocks: usize, group: ReadyGroup, days: i64, priority: &str) -> String {
    let waited = match days {
        0 => "today".to_owned(),
        1 => "waiting a day".to_owned(),
        n => format!("waiting {n} days"),
    };
    let head = match (unblocks, group) {
        (0, ReadyGroup::Active) => format!("in an active phase · {waited}"),
        (0, ReadyGroup::Bug) => format!("a bug, in no phase · {waited}"),
        (0, ReadyGroup::Rest) => format!("in no phase · {waited}"),
        (1, _) => format!("unblocks 1 other task · {waited}"),
        (n, _) => format!("unblocks {n} other tasks · {waited}"),
    };
    // Only when it is not the default. Printing "· p2" on every row is how the
    // old reason came to say nothing; printing p0 is worth the space.
    if priority == DEFAULT_PRIORITY {
        head
    } else {
        format!("{head} · {priority}")
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn day(n: i64) -> DateTime<Utc> {
        Utc::now() - chrono::Duration::days(n)
    }

    /// The property the whole change turns on. "nothing is blocking it · p2"
    /// was printed on all 29 rows, so it explained no position — a reason that
    /// is the same everywhere is not a reason. This asserts the replacement
    /// actually distinguishes rows rather than being a differently-worded
    /// tautology.
    #[test]
    fn the_reason_differs_between_the_groups() {
        let active = reason(0, ReadyGroup::Active, 3, "p2");
        let bug = reason(0, ReadyGroup::Bug, 3, "p2");
        let rest = reason(0, ReadyGroup::Rest, 3, "p2");
        assert_ne!(active, bug);
        assert_ne!(bug, rest);
        assert_ne!(active, rest);
    }

    /// Two tasks in the same group still separate, because age varies even when
    /// nothing else does — which on this store is the usual case.
    #[test]
    fn two_rows_in_one_group_still_read_differently_by_age() {
        assert_ne!(
            reason(0, ReadyGroup::Rest, 1, "p2"),
            reason(0, ReadyGroup::Rest, 9, "p2")
        );
    }

    /// Where `unblocks` is real it still leads, because it is a stronger reason
    /// than age. This store has none, but another might.
    #[test]
    fn unblocking_still_leads_the_reason_where_it_is_real() {
        assert!(reason(2, ReadyGroup::Rest, 4, "p2").starts_with("unblocks 2 other tasks"));
        assert!(reason(1, ReadyGroup::Active, 0, "p2").starts_with("unblocks 1 other task"));
    }

    #[test]
    fn waiting_reads_as_a_person_would_say_it() {
        assert!(reason(0, ReadyGroup::Rest, 0, "p2").ends_with("today"));
        assert!(reason(0, ReadyGroup::Rest, 1, "p2").ends_with("waiting a day"));
        assert!(reason(0, ReadyGroup::Rest, 5, "p2").ends_with("waiting 5 days"));
    }

    /// A clock that has stepped backwards must not produce "waiting -2 days".
    #[test]
    fn a_task_created_in_the_future_waits_zero_days() {
        assert_eq!(days_waiting(day(-2), Utc::now()), 0);
        assert_eq!(days_waiting(day(3), Utc::now()), 3);
    }

    /// The default is on almost every row, so showing it is how "· p2" became
    /// noise. Anything else is a deliberate mark and worth the space.
    #[test]
    fn priority_shows_only_when_somebody_set_it() {
        assert!(!reason(0, ReadyGroup::Rest, 2, "p2").contains("p2"));
        assert!(reason(0, ReadyGroup::Rest, 2, "p0").ends_with("· p0"));
        assert!(reason(0, ReadyGroup::Active, 2, "p3").ends_with("· p3"));
    }

    #[test]
    fn an_active_phase_outranks_a_bug_which_outranks_the_rest() {
        assert!(group_rank(ReadyGroup::Active) < group_rank(ReadyGroup::Bug));
        assert!(group_rank(ReadyGroup::Bug) < group_rank(ReadyGroup::Rest));
    }
}
