//! `keel_context` — the digest.
//!
//! The most important tool in the surface. G2 is "an agent can orient itself on
//! a project in a single MCP call", and REQ-3 budgets that call to something an
//! agent will actually pay: roughly 3–4k tokens.
//!
//! # What is never cut, and why
//!
//! Open questions and glossary terms are declared unbounded (SPEC §6.3). Every
//! other section degrades gracefully and reports what it dropped, but those two
//! do not, because they fail differently from the rest:
//!
//! - A truncated task list makes an agent **less informed**. It will ask.
//! - A truncated question register makes an agent **confidently wrong**. It
//!   re-opens a settled decision and argues for it.
//! - A truncated glossary makes an agent **use the wrong word** for a domain
//!   concept, which then propagates into everything it writes.
//!
//! If those two alone blow the budget, the digest returns them in full and sets
//! `budget_exceeded`. That is not a failure — it is the store telling you the
//! open-question register needs pruning, which is real information.

use keel_core::{
    Cursor, DuckStore, Entity, EntityId, EntityQuery, EntityStore, EntityType, MilestoneStatus,
    QuestionStatus, Result, TaskStatus,
};
use serde::Serialize;

/// How much to include.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Depth {
    /// A few hundred tokens: what it is, what is urgent, what is unresolved.
    Brief,
    /// The default. Budgeted to roughly 3–4k tokens.
    Standard,
    /// Most limits lifted, for when an agent is about to do something large.
    Full,
}

impl Depth {
    /// Parse the `depth` argument.
    pub fn parse(s: &str) -> std::result::Result<Self, String> {
        match s {
            "brief" => Ok(Depth::Brief),
            "standard" => Ok(Depth::Standard),
            "full" => Ok(Depth::Full),
            other => Err(format!(
                "`{other}` is not a depth. Expected: brief | standard | full"
            )),
        }
    }

    /// The soft character budget. Roughly four characters per token.
    fn budget_chars(self) -> usize {
        match self {
            Depth::Brief => 3_000,
            Depth::Standard => 14_000,
            Depth::Full => 60_000,
        }
    }

    /// How many items each bounded section may carry.
    fn section_limit(self) -> usize {
        match self {
            Depth::Brief => 3,
            Depth::Standard => 10,
            Depth::Full => 50,
        }
    }
}

/// A section that was cut, and by how much.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Truncation {
    /// Which section.
    pub section: String,
    /// How many were shown.
    pub shown: usize,
    /// How many exist.
    pub total: usize,
}

/// One line about a project in the cross-project roll-up.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProjectLine {
    /// `prj_…`
    pub id: EntityId,
    /// Display name.
    pub name: String,
    /// URL-safe short name.
    pub slug: String,
    /// active / paused / shipped / abandoned.
    pub status: String,
    /// Open tasks.
    pub open_tasks: usize,
    /// Open tasks at p0 or p1.
    pub urgent_tasks: usize,
    /// Blocked tasks.
    pub blocked_tasks: usize,
    /// Unresolved questions and risks.
    pub open_questions: usize,
    /// The milestone currently in flight, if any.
    pub active_milestone: Option<String>,
}

/// A compact reference to an artifact.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Item {
    /// `tsk_…` and friends.
    pub id: EntityId,
    /// The artifact type.
    pub entity_type: String,
    /// Its one-line label.
    pub label: String,
    /// Its status, for types that have one.
    pub status: Option<String>,
    /// Extra context — priority, severity, target date.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A glossary entry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TermEntry {
    /// The word.
    pub term: String,
    /// What it means here.
    pub definition: String,
    /// Whether it is global rather than project-scoped.
    pub global: bool,
}

/// The digest.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Digest {
    /// The project, or `None` for a cross-project roll-up.
    pub project: Option<ProjectLine>,
    /// One line per project. Only populated for the roll-up.
    pub projects: Vec<ProjectLine>,
    /// Milestones in flight.
    pub active: Vec<Item>,
    /// Urgent, blocked and overdue work.
    pub attention: Vec<Item>,
    /// Recent activity, summarised.
    pub recent: Vec<String>,
    /// The last few accepted decisions.
    pub decisions: Vec<Item>,
    /// **Never truncated.** Every unresolved question and unmitigated risk.
    pub questions: Vec<Item>,
    /// Current specs.
    pub specs: Vec<Item>,
    /// **Never truncated.** The glossary.
    pub terms: Vec<TermEntry>,
    /// What is live, at what version.
    pub environments: Vec<Item>,
    /// Suggested next actions, derived from state.
    pub next: Vec<String>,
    /// What was cut.
    pub truncated: Vec<Truncation>,
    /// Set when the unbounded sections alone exceed the budget.
    pub budget_exceeded: bool,
    /// Rough token estimate of this digest.
    pub estimated_tokens: usize,
}

impl Digest {
    /// Render the digest as the prose an agent actually reads.
    pub fn to_prose(&self) -> String {
        let mut out = String::new();

        if let Some(p) = &self.project {
            out.push_str(&format!("# {} ({})\n", p.name, p.slug));
            out.push_str(&format!(
                "status: {} · {} open task(s), {} urgent, {} blocked · {} open question(s)\n",
                p.status, p.open_tasks, p.urgent_tasks, p.blocked_tasks, p.open_questions
            ));
            if let Some(m) = &p.active_milestone {
                out.push_str(&format!("active milestone: {m}\n"));
            }
        } else {
            out.push_str("# All projects\n");
            for p in &self.projects {
                out.push_str(&format!(
                    "- {} ({}) — {} · {} open, {} urgent, {} blocked · {} question(s)\n",
                    p.name,
                    p.slug,
                    p.status,
                    p.open_tasks,
                    p.urgent_tasks,
                    p.blocked_tasks,
                    p.open_questions
                ));
            }
        }

        let section = |out: &mut String, title: &str, items: &[Item]| {
            if items.is_empty() {
                return;
            }
            out.push_str(&format!("\n## {title}\n"));
            for i in items {
                let status = i.status.as_deref().unwrap_or("");
                let detail = i.detail.as_deref().unwrap_or("");
                out.push_str(&format!(
                    "- {} [{}{}{}] {}\n",
                    i.label,
                    status,
                    if detail.is_empty() { "" } else { " · " },
                    detail,
                    i.id
                ));
            }
        };

        section(&mut out, "Active", &self.active);
        section(&mut out, "Needs attention", &self.attention);
        section(&mut out, "Open questions and risks", &self.questions);
        section(&mut out, "Recent decisions", &self.decisions);
        section(&mut out, "Specs", &self.specs);
        section(&mut out, "Environments", &self.environments);

        if !self.terms.is_empty() {
            out.push_str("\n## Glossary\n");
            for t in &self.terms {
                out.push_str(&format!(
                    "- **{}**{}: {}\n",
                    t.term,
                    if t.global { " (global)" } else { "" },
                    t.definition
                ));
            }
        }

        if !self.recent.is_empty() {
            out.push_str("\n## Recently\n");
            for line in &self.recent {
                out.push_str(&format!("- {line}\n"));
            }
        }

        if !self.next.is_empty() {
            out.push_str("\n## Suggested next\n");
            for line in &self.next {
                out.push_str(&format!("- {line}\n"));
            }
        }

        if !self.truncated.is_empty() {
            out.push_str("\n---\n");
            for t in &self.truncated {
                out.push_str(&format!(
                    "{}: showing {} of {}. Use keel_search or keel_get for the rest.\n",
                    t.section, t.shown, t.total
                ));
            }
        }
        if self.budget_exceeded {
            out.push_str(
                "\nThis digest is over its size budget because the open questions and \
                 glossary are returned in full — they are never trimmed. That usually means \
                 the question register needs pruning.\n",
            );
        }

        out
    }
}

/// Estimate tokens from characters. Deliberately crude — the budget is a
/// guardrail, not an accounting system, and a real tokeniser here would be a
/// dependency and a decision for no gain.
fn estimate_tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

/// Build the digest.
pub fn build(
    store: &DuckStore,
    project: Option<&EntityId>,
    depth: Depth,
    since: Option<chrono::DateTime<chrono::Utc>>,
) -> Result<Digest> {
    let limit = depth.section_limit();

    let mut digest = Digest {
        project: None,
        projects: Vec::new(),
        active: Vec::new(),
        attention: Vec::new(),
        recent: Vec::new(),
        decisions: Vec::new(),
        questions: Vec::new(),
        specs: Vec::new(),
        terms: Vec::new(),
        environments: Vec::new(),
        next: Vec::new(),
        truncated: Vec::new(),
        budget_exceeded: false,
        estimated_tokens: 0,
    };

    match project {
        None => {
            // Cross-project roll-up: one line each, plus anything at risk.
            let projects = store.list(&EntityQuery::default().of_type(EntityType::Project))?;
            for entity in &projects.items {
                if let Entity::Project(p) = entity {
                    digest.projects.push(project_line(store, p)?);
                }
            }
            digest.projects.sort_by(|a, b| {
                b.urgent_tasks
                    .cmp(&a.urgent_tasks)
                    .then_with(|| b.blocked_tasks.cmp(&a.blocked_tasks))
            });

            // Questions across every project are still never truncated: a
            // settled decision in project A is just as re-litigable from a
            // conversation that started in project B.
            digest.questions = open_questions(store, None)?;
            digest.terms = glossary(store, None)?;

            // UC-6 asks "what shipped this week" across every project, so the
            // roll-up needs activity too. Leaving it empty made the Sunday
            // review say "no activity yet" against a store with hundreds of
            // events, which is worse than saying nothing.
            digest.recent = recent_activity(store, None, since, limit * 2)?;

            // Milestones in flight, across everything — the other half of
            // "what is happening right now".
            let mut active = Vec::new();
            for line in &digest.projects {
                if line.active_milestone.is_some() {
                    active.extend(active_milestones(store, &line.id, 2)?);
                }
            }
            digest.active = active;

            digest.next = rollup_suggestions(&digest.projects);
        }
        Some(project_id) => {
            let Some(Entity::Project(p)) = store.get(project_id)? else {
                return Err(keel_core::Error::NotFound {
                    entity_type: EntityType::Project,
                    id: project_id.to_string(),
                });
            };
            let line = project_line(store, &p)?;

            digest.active = active_milestones(store, project_id, limit)?;
            let (attention, attention_total) = needs_attention(store, project_id, limit)?;
            digest.attention = attention;
            if attention_total > digest.attention.len() {
                digest.truncated.push(Truncation {
                    section: "attention".to_owned(),
                    shown: digest.attention.len(),
                    total: attention_total,
                });
            }

            digest.questions = open_questions(store, Some(project_id))?;
            digest.terms = glossary(store, Some(project_id))?;

            let (decisions, decisions_total) = recent_decisions(store, project_id, 5.min(limit))?;
            digest.decisions = decisions;
            if decisions_total > digest.decisions.len() {
                digest.truncated.push(Truncation {
                    section: "decisions".to_owned(),
                    shown: digest.decisions.len(),
                    total: decisions_total,
                });
            }

            let (specs, specs_total) = current_specs(store, project_id, limit)?;
            digest.specs = specs;
            if specs_total > digest.specs.len() {
                digest.truncated.push(Truncation {
                    section: "specs".to_owned(),
                    shown: digest.specs.len(),
                    total: specs_total,
                });
            }

            digest.environments = environments(store, project_id)?;
            digest.recent = recent_activity(store, Some(project_id), since, limit)?;
            digest.next = suggestions(&line, &digest);
            digest.project = Some(line);
        }
    }

    // Measure, then trim only the sections that may be trimmed.
    let mut rendered = digest.to_prose();
    if rendered.len() > depth.budget_chars() {
        // Trim in order of what an agent can most cheaply re-fetch. Recent
        // activity first — `keel_activity` is one call away and rarely
        // changes a decision.
        for section in ["recent", "specs", "decisions", "attention"] {
            if rendered.len() <= depth.budget_chars() {
                break;
            }
            trim_section(&mut digest, section);
            rendered = digest.to_prose();
        }
        // Questions and terms are never touched. If the digest is still over
        // budget, say so rather than trimming them.
        if rendered.len() > depth.budget_chars() {
            digest.budget_exceeded = true;
            rendered = digest.to_prose();
        }
    }

    digest.estimated_tokens = estimate_tokens(&rendered);
    Ok(digest)
}

/// Halve a trimmable section, recording what was dropped.
fn trim_section(digest: &mut Digest, section: &str) {
    let (len, keep) = match section {
        "recent" => (digest.recent.len(), digest.recent.len() / 2),
        "specs" => (digest.specs.len(), digest.specs.len() / 2),
        "decisions" => (digest.decisions.len(), digest.decisions.len() / 2),
        "attention" => (digest.attention.len(), digest.attention.len() / 2),
        _ => return,
    };
    if len == 0 {
        return;
    }
    match section {
        "recent" => digest.recent.truncate(keep),
        "specs" => digest.specs.truncate(keep),
        "decisions" => digest.decisions.truncate(keep),
        "attention" => digest.attention.truncate(keep),
        _ => {}
    }
    match digest.truncated.iter_mut().find(|t| t.section == section) {
        Some(existing) => existing.shown = keep,
        None => digest.truncated.push(Truncation {
            section: section.to_owned(),
            shown: keep,
            total: len,
        }),
    }
}

fn project_line(store: &DuckStore, p: &keel_core::Project) -> Result<ProjectLine> {
    let tasks = store.list(
        &EntityQuery::in_project(p.id.clone())
            .of_type(EntityType::Task)
            .limited(2000),
    )?;

    let mut open = 0;
    let mut urgent = 0;
    let mut blocked = 0;
    for t in &tasks.items {
        if let Entity::Task(t) = t {
            if !t.status.is_open() {
                continue;
            }
            open += 1;
            if t.priority.is_urgent() {
                urgent += 1;
            }
            if t.status == TaskStatus::Blocked {
                blocked += 1;
            }
        }
    }

    let questions = store.list(
        &EntityQuery::in_project(p.id.clone())
            .of_type(EntityType::Question)
            .with_status([QuestionStatus::Open.as_str()])
            .limited(2000),
    )?;

    let milestones = store.list(
        &EntityQuery::in_project(p.id.clone())
            .of_type(EntityType::Milestone)
            .with_status([MilestoneStatus::Active.as_str()])
            .limited(10),
    )?;

    Ok(ProjectLine {
        id: p.id.clone(),
        name: p.name.clone(),
        slug: p.slug.clone(),
        status: p.status.as_str().to_owned(),
        open_tasks: open,
        urgent_tasks: urgent,
        blocked_tasks: blocked,
        open_questions: questions.total,
        active_milestone: milestones.items.first().map(|m| m.label().to_owned()),
    })
}

fn active_milestones(store: &DuckStore, project: &EntityId, limit: usize) -> Result<Vec<Item>> {
    let page = store.list(
        &EntityQuery::in_project(project.clone())
            .of_type(EntityType::Milestone)
            .with_status([
                MilestoneStatus::Active.as_str(),
                MilestoneStatus::Blocked.as_str(),
            ])
            .limited(limit),
    )?;
    Ok(page
        .items
        .iter()
        .map(|e| {
            let detail = match e {
                Entity::Milestone(m) => m.target_date.map(|d| format!("target {d}")),
                _ => None,
            };
            item(e, detail)
        })
        .collect())
}

fn needs_attention(
    store: &DuckStore,
    project: &EntityId,
    limit: usize,
) -> Result<(Vec<Item>, usize)> {
    let page = store.list(
        &EntityQuery::in_project(project.clone())
            .of_type(EntityType::Task)
            .with_status([
                TaskStatus::Todo.as_str(),
                TaskStatus::InProgress.as_str(),
                TaskStatus::Blocked.as_str(),
                TaskStatus::Review.as_str(),
            ])
            .limited(2000),
    )?;

    let mut urgent: Vec<(u8, Item)> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Task(t) if t.priority.is_urgent() || t.status == TaskStatus::Blocked => {
                // Blocked first, then by priority — a blocked p1 needs a human
                // more than an unblocked p0 does.
                let rank = match (t.status, t.priority) {
                    (TaskStatus::Blocked, _) => 0,
                    (_, keel_core::TaskPriority::P0) => 1,
                    _ => 2,
                };
                Some((rank, item(e, Some(format!("{}", t.priority)))))
            }
            _ => None,
        })
        .collect();
    urgent.sort_by_key(|(rank, _)| *rank);

    let total = urgent.len();
    Ok((
        urgent.into_iter().take(limit).map(|(_, i)| i).collect(),
        total,
    ))
}

/// Every unresolved question and risk. **Never truncated.**
fn open_questions(store: &DuckStore, project: Option<&EntityId>) -> Result<Vec<Item>> {
    let mut query = EntityQuery::default()
        .of_type(EntityType::Question)
        .with_status([QuestionStatus::Open.as_str()]);
    query.project_id = project.cloned();
    // A high explicit limit rather than the default: this list must not be cut
    // by the store's paging either. If a project ever has 5,000 open
    // questions, the digest saying so is the useful outcome.
    query.limit = Some(5_000);

    let page = store.list(&query)?;
    Ok(page
        .items
        .iter()
        .map(|e| {
            let detail = match e {
                Entity::Question(q) => {
                    let mut parts = vec![q.kind.to_string()];
                    if let Some(s) = q.severity {
                        parts.push(s.to_string());
                    }
                    Some(parts.join(" · "))
                }
                _ => None,
            };
            item(e, detail)
        })
        .collect())
}

/// The glossary. **Never truncated.**
fn glossary(store: &DuckStore, project: Option<&EntityId>) -> Result<Vec<TermEntry>> {
    let mut query = EntityQuery::default().of_type(EntityType::Term);
    query.project_id = project.cloned();
    query.limit = Some(5_000);

    let page = store.list(&query)?;
    let mut terms: Vec<TermEntry> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Term(t) => Some(TermEntry {
                term: t.term.clone(),
                definition: t.definition.clone(),
                global: t.project_id.is_none(),
            }),
            _ => None,
        })
        .collect();

    // Project-first resolution (Q-4): a project-scoped term overrides a global
    // one of the same name, so drop the global when both are present.
    let scoped: std::collections::HashSet<String> = terms
        .iter()
        .filter(|t| !t.global)
        .map(|t| t.term.to_lowercase())
        .collect();
    terms.retain(|t| !t.global || !scoped.contains(&t.term.to_lowercase()));
    terms.sort_by(|a, b| a.term.to_lowercase().cmp(&b.term.to_lowercase()));
    Ok(terms)
}

fn recent_decisions(
    store: &DuckStore,
    project: &EntityId,
    limit: usize,
) -> Result<(Vec<Item>, usize)> {
    let page = store.list(
        &EntityQuery::in_project(project.clone())
            .of_type(EntityType::Decision)
            .with_status([keel_core::DecisionStatus::Accepted.as_str()])
            .limited(2000),
    )?;
    let total = page.items.len();
    Ok((
        page.items
            .iter()
            .take(limit)
            .map(|e| item(e, None))
            .collect(),
        total,
    ))
}

fn current_specs(
    store: &DuckStore,
    project: &EntityId,
    limit: usize,
) -> Result<(Vec<Item>, usize)> {
    let page = store.list(
        &EntityQuery::in_project(project.clone())
            .of_type(EntityType::Spec)
            .with_status([
                keel_core::SpecStatus::Draft.as_str(),
                keel_core::SpecStatus::Review.as_str(),
                keel_core::SpecStatus::Approved.as_str(),
            ])
            .limited(2000),
    )?;
    let total = page.items.len();
    Ok((
        page.items
            .iter()
            .take(limit)
            .map(|e| {
                let detail = match e {
                    Entity::Spec(s) => Some(s.kind.to_string()),
                    _ => None,
                };
                item(e, detail)
            })
            .collect(),
        total,
    ))
}

fn environments(store: &DuckStore, project: &EntityId) -> Result<Vec<Item>> {
    let page = store.list(
        &EntityQuery::in_project(project.clone())
            .of_type(EntityType::Environment)
            .limited(20),
    )?;
    Ok(page
        .items
        .iter()
        .map(|e| {
            let detail = match e {
                Entity::Environment(env) => env.deployed_version.clone(),
                _ => None,
            };
            item(e, detail)
        })
        .collect())
}

/// Recent activity. `project` of `None` spans every project.
fn recent_activity(
    store: &DuckStore,
    project: Option<&EntityId>,
    since: Option<chrono::DateTime<chrono::Utc>>,
    limit: usize,
) -> Result<Vec<String>> {
    let cursor = match since {
        Some(t) => Cursor::Since(t),
        None => Cursor::Beginning,
    };
    // Read generously and keep the tail: `events` returns oldest-first so a
    // cursor caller sees no gaps, but "recently" wants the newest.
    let page = store.events(&cursor, project, 2_000)?;
    Ok(page
        .items
        .iter()
        .rev()
        .take(limit)
        .map(|e| {
            format!(
                "{} — {} ({})",
                e.created_at.format("%Y-%m-%d"),
                e.summary,
                e.actor
            )
        })
        .collect())
}

/// Suggested next actions, derived from state rather than invented.
fn suggestions(line: &ProjectLine, digest: &Digest) -> Vec<String> {
    let mut out = Vec::new();

    if line.blocked_tasks > 0 {
        out.push(format!(
            "{} task(s) are blocked. Check what is blocking them with keel_get(depth: 2, \
             direction: \"inbound\", rels: [\"blocks\"]).",
            line.blocked_tasks
        ));
    }
    if line.open_questions > 0 {
        out.push(format!(
            "{} question(s) are unresolved. Resolving one usually unblocks more than it costs.",
            line.open_questions
        ));
    }
    let review = digest
        .attention
        .iter()
        .filter(|i| i.status.as_deref() == Some("review"))
        .count();
    if review > 0 {
        out.push(format!(
            "{review} task(s) are in review — confirm with the human whether they are done."
        ));
    }
    if digest.active.is_empty() && line.open_tasks > 0 {
        out.push(
            "No milestone is active, but there is open work. Consider grouping it under one."
                .to_owned(),
        );
    }
    if line.urgent_tasks == 0 && line.open_tasks > 0 {
        out.push(
            "Nothing is marked p0 or p1. If something is actually urgent, say so on the task."
                .to_owned(),
        );
    }
    out
}

/// Suggestions for the cross-project view.
fn rollup_suggestions(projects: &[ProjectLine]) -> Vec<String> {
    let mut out = Vec::new();
    let at_risk: Vec<&ProjectLine> = projects
        .iter()
        .filter(|p| p.blocked_tasks > 0 || p.urgent_tasks > 0)
        .collect();
    if !at_risk.is_empty() {
        out.push(format!(
            "At risk: {}. Open one with keel_context(project: …).",
            at_risk
                .iter()
                .map(|p| p.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    let stale: Vec<&ProjectLine> = projects
        .iter()
        .filter(|p| p.status == "active" && p.open_tasks == 0)
        .collect();
    if !stale.is_empty() {
        out.push(format!(
            "Active but with no open work: {}. Either they shipped or the tracker is stale.",
            stale
                .iter()
                .map(|p| p.slug.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    out
}

/// Compact an entity into a digest item.
fn item(entity: &Entity, detail: Option<String>) -> Item {
    Item {
        id: entity.id().clone(),
        entity_type: entity.entity_type().as_str().to_owned(),
        label: entity.label().to_owned(),
        status: entity.status().map(str::to_owned),
        detail,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn depth_parses_and_rejects_nonsense() {
        assert_eq!(Depth::parse("brief").unwrap(), Depth::Brief);
        assert_eq!(Depth::parse("standard").unwrap(), Depth::Standard);
        assert_eq!(Depth::parse("full").unwrap(), Depth::Full);
        let err = Depth::parse("verbose").unwrap_err();
        assert!(err.contains("brief | standard | full"), "{err}");
    }

    #[test]
    fn budgets_increase_with_depth() {
        assert!(Depth::Brief.budget_chars() < Depth::Standard.budget_chars());
        assert!(Depth::Standard.budget_chars() < Depth::Full.budget_chars());
    }

    #[test]
    fn the_standard_budget_is_about_four_thousand_tokens() {
        // REQ-3. Four characters per token is crude but it is the guardrail,
        // not an accounting system.
        assert_eq!(estimate_tokens(&"x".repeat(14_000)), 3_500);
    }

    #[test]
    fn trimming_never_touches_questions_or_terms() {
        let mut d = Digest {
            project: None,
            projects: vec![],
            active: vec![],
            attention: vec![],
            recent: vec!["a".into(), "b".into(), "c".into(), "d".into()],
            decisions: vec![],
            questions: vec![Item {
                id: EntityId::generate(EntityType::Question),
                entity_type: "question".into(),
                label: "Where does the store live?".into(),
                status: Some("open".into()),
                detail: None,
            }],
            specs: vec![],
            terms: vec![TermEntry {
                term: "Digest".into(),
                definition: "…".into(),
                global: false,
            }],
            environments: vec![],
            next: vec![],
            truncated: vec![],
            budget_exceeded: false,
            estimated_tokens: 0,
        };

        for section in ["recent", "questions", "terms"] {
            trim_section(&mut d, section);
        }
        assert_eq!(d.recent.len(), 2, "recent is trimmable");
        assert_eq!(d.questions.len(), 1, "questions must never be trimmed");
        assert_eq!(d.terms.len(), 1, "terms must never be trimmed");
    }

    #[test]
    fn trimming_records_what_it_dropped() {
        let mut d = Digest {
            project: None,
            projects: vec![],
            active: vec![],
            attention: vec![],
            recent: (0..10).map(|i| format!("event {i}")).collect(),
            decisions: vec![],
            questions: vec![],
            specs: vec![],
            terms: vec![],
            environments: vec![],
            next: vec![],
            truncated: vec![],
            budget_exceeded: false,
            estimated_tokens: 0,
        };
        trim_section(&mut d, "recent");
        assert_eq!(d.truncated.len(), 1);
        assert_eq!(d.truncated[0].section, "recent");
        assert_eq!(d.truncated[0].shown, 5);
        assert_eq!(d.truncated[0].total, 10);
        assert!(d.to_prose().contains("showing 5 of 10"));
    }
}
