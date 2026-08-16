//! Reporting the rows that are already hard to read.
//!
//! Three rules landed after most of this store existed: a task needs a summary
//! (TQ-34), a close needs a reason and evidence (B-47), and prose should not
//! lean on a bare identifier nobody can look up. All three are enforced on the
//! way in, and none of them can be enforced backwards — ninety-four tasks
//! predate the first and a hundred and seven closes predate the second.
//!
//! # This reports and never rewrites
//!
//! That is the whole design, not a limitation of it. A machine filling in a
//! missing summary would produce exactly the confident, plausible, wrong prose
//! the requirement exists to prevent, and it would produce it in the one field a
//! reader is told to trust. Same reasoning that stops the mirror ever reading a
//! file back into the store.
//!
//! So `keel lint` prints a list for a person to work through, and the honest
//! measure of it is that the list gets shorter.
//!
//! # Why the identifier check is a heuristic, and why that is acceptable
//!
//! "Waiting on TQ-32." tells a reader six weeks later nothing at all; the same
//! sentence with the question in it tells them everything. There is no exact
//! test for that. What there is: an identifier sitting in a sentence with almost
//! no other content is nearly always the first case. So the rule is a word count
//! on the sentence around it, stated plainly here so nobody mistakes it for
//! semantics.
//!
//! A heuristic is safe *because* this only reports. A false positive costs
//! someone a glance; it cannot cost them a paragraph.

use crate::{
    Entity, EntityId, EntityQuery, EntityStore, EntityType, Result, TaskStatus, store::Store,
};

/// One row worth a person's attention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LintFinding {
    /// Which rule. Stable, so a report can be diffed between runs.
    pub check: &'static str,
    /// The artifact.
    pub id: EntityId,
    /// What to call it out loud — `KEEL-42`, or the title where there is no
    /// readable identifier.
    pub reference: String,
    /// What is wrong with this particular row.
    pub detail: String,
}

/// What the lint found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LintReport {
    /// Every finding, worst rule first and then by reference.
    pub findings: Vec<LintFinding>,
    /// How many rows were looked at.
    pub scanned: usize,
    /// How many findings there were before any limit was applied.
    pub total: usize,
    /// Whether the list was cut.
    pub truncated: bool,
    /// How many findings each rule produced, counted before the limit.
    ///
    /// Stored rather than derived from `findings`, and the difference matters:
    /// derived, the summary would count only what was shown, so a report cut at
    /// twelve would say "12 task_without_summary" under a total of 240. The
    /// per-rule counts are the part a person uses to decide what to work on, so
    /// they have to describe the project rather than the page.
    pub counts: Vec<(&'static str, usize)>,
}

impl LintReport {
    /// How many findings each rule produced, across the whole project.
    pub fn by_check(&self) -> &[(&'static str, usize)] {
        &self.counts
    }

    /// How many findings a given rule produced, across the whole project.
    pub fn count_of(&self, check: &str) -> usize {
        self.counts
            .iter()
            .find(|(name, _)| *name == check)
            .map_or(0, |(_, n)| *n)
    }
}

/// A task with no summary. Nothing but the title says what it is.
pub const TASK_WITHOUT_SUMMARY: &str = "task_without_summary";

/// A bare `TQ-14` or `B-12` in a sentence with nothing else in it.
pub const UNEXPANDED_IDENTIFIER: &str = "unexpanded_identifier";

/// A closed task that never said why. Only rows that predate the rule.
pub const CLOSED_WITHOUT_REASON: &str = "closed_without_reason";

/// A spec, decision, question or feedback row with no prose in it at all.
///
/// The types with no summary column, where the document *is* the content, so a
/// row without one records that somebody decided something and loses what.
/// Since KEEL-171 the create path refuses it; ten rows landed before that,
/// six of them accepted decisions, and this is what keeps them visible rather
/// than being quietly tidied away or quietly forgotten.
///
/// Reported rather than repaired, deliberately. Writing the missing reasoning
/// means a later session inferring an argument from the code and presenting it
/// as what somebody thought — which is the one thing a decision log must not
/// contain. The same reasoning left ninety-four task summaries unwritten.
pub const DOCUMENT_WITHOUT_PROSE: &str = "document_without_prose";

/// The identifier families this checks. The same ones `fsck` recognises, plus
/// the readable task and requirement references that prose actually writes.
const FAMILIES: &[&str] = &["TQ", "Q", "B", "D", "R", "REQ"];

/// How many content words a sentence needs around an identifier before the
/// identifier counts as explained.
///
/// Six, arrived at by reading the rows rather than by principle. "Waiting on
/// TQ-32" is two; "Blocked on TQ-30, which says the app stays read-only" is
/// seven and is genuinely readable. Between four and eight all behave much the
/// same on this store, so the number is not load-bearing — what matters is that
/// it is a word count and that this comment says so.
const ENOUGH_WORDS: usize = 6;

/// Look at every row in a project and report what a reader would struggle with.
///
/// `limit` caps the findings returned, and the report says what it left out —
/// hard constraint 4 applies to a lint as much as to anything else, and a person
/// shown twenty of ninety with no total will believe they are nearly done.
pub fn lint(store: &Store, project_id: &EntityId, limit: Option<usize>) -> Result<LintReport> {
    let key = match store.get(project_id)? {
        Some(Entity::Project(p)) => p.key,
        _ => String::new(),
    };

    let page = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Task)
            .limited(10_000),
    )?;

    let mut findings = Vec::new();
    let mut scanned = 0usize;

    for entity in &page.items {
        let Entity::Task(task) = entity else { continue };
        scanned += 1;
        let reference = format!("{key}-{}", task.number);

        if task.summary.as_deref().unwrap_or("").trim().is_empty() {
            findings.push(LintFinding {
                check: TASK_WITHOUT_SUMMARY,
                id: task.id.clone(),
                reference: reference.clone(),
                detail: format!(
                    "no summary — a list shows “{}” and nothing else",
                    shorten(&task.title, 56)
                ),
            });
        }

        let terminal = matches!(task.status, TaskStatus::Done | TaskStatus::WontDo);
        if terminal && task.close_reason.is_none() {
            findings.push(LintFinding {
                check: CLOSED_WITHOUT_REASON,
                id: task.id.clone(),
                reference: reference.clone(),
                detail: format!(
                    "{} with no reason recorded, so nothing says what happened",
                    task.status
                ),
            });
        }

        // The summary and the body both. The summary matters more — it is what
        // lists render — but a body that says only "see TQ-32" is the same dead
        // end one click further in.
        for (field, text) in [
            ("summary", task.summary.as_deref().unwrap_or("")),
            ("body", task.body.as_deref().unwrap_or("")),
        ] {
            for id in unexpanded(text) {
                findings.push(LintFinding {
                    check: UNEXPANDED_IDENTIFIER,
                    id: task.id.clone(),
                    reference: reference.clone(),
                    detail: format!(
                        "{field} leans on {id} with no gloss beside it — a reader has to go and \
                         look it up to learn anything"
                    ),
                });
            }
        }
    }

    // The prose-bearing types, which the loop above does not reach because it
    // asks only for tasks. A row here has no summary column to fall back on, so
    // "no prose" means the page is empty rather than terse.
    for entity_type in [
        EntityType::Spec,
        EntityType::Decision,
        EntityType::Question,
        EntityType::Feedback,
    ] {
        let page = store.list(
            &EntityQuery::in_project(project_id.clone())
                .of_type(entity_type)
                .limited(10_000),
        )?;
        for entity in &page.items {
            scanned += 1;
            if entity.current_doc_version().unwrap_or(0) > 0 {
                continue;
            }
            // Decisions carry a readable number like tasks do; the others are
            // known by their title, so that is what a person is shown.
            let reference = match entity {
                Entity::Decision(d) => format!("{key}-B{}", d.number),
                other => shorten(other.label(), 48),
            };
            findings.push(LintFinding {
                check: DOCUMENT_WITHOUT_PROSE,
                id: entity.id().clone(),
                reference,
                detail: format!(
                    "{entity_type} with no prose — the title says something was decided and \
                     nothing says what"
                ),
            });
        }
    }

    // Rules in the order a person would work through them: the field lists
    // render first, then the prose, then the historical closes.
    let order = |check: &str| match check {
        TASK_WITHOUT_SUMMARY => 0,
        UNEXPANDED_IDENTIFIER => 1,
        DOCUMENT_WITHOUT_PROSE => 2,
        _ => 3,
    };
    findings.sort_by(|a, b| {
        order(a.check)
            .cmp(&order(b.check))
            .then_with(|| a.reference.cmp(&b.reference))
            .then_with(|| a.detail.cmp(&b.detail))
    });

    let mut tally: std::collections::BTreeMap<&'static str, usize> = Default::default();
    for finding in &findings {
        *tally.entry(finding.check).or_default() += 1;
    }

    let total = findings.len();
    let truncated = match limit {
        Some(n) if total > n => {
            findings.truncate(n);
            true
        }
        _ => false,
    };

    Ok(LintReport {
        findings,
        scanned,
        total,
        truncated,
        counts: tally.into_iter().collect(),
    })
}

/// The identifiers in `text` that sit in a sentence with nothing else in it.
///
/// Code fences and inline code are stripped first, for the same reason the style
/// checker strips them: an identifier inside backticks is usually being *named*
/// rather than leaned on, and a rule that cannot tell the difference makes
/// quoting anything a lint failure.
fn unexpanded(text: &str) -> Vec<String> {
    let prose = strip_code(text);
    let mut out = Vec::new();
    for sentence in prose.split(['.', '!', '?', '\n']) {
        let ids = identifiers(sentence);
        if ids.is_empty() {
            continue;
        }
        let words = sentence
            .split_whitespace()
            .filter(|w| {
                let word = w.trim_matches(|c: char| !c.is_ascii_alphanumeric());
                word.len() > 2 && !ids.iter().any(|id| w.contains(id.as_str()))
            })
            .count();
        if words < ENOUGH_WORDS {
            out.extend(ids);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Bare `PREFIX-number` tokens, matched the way `fsck` matches them.
fn identifiers(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        let token = raw.trim_matches('-');
        let Some((prefix, number)) = token.split_once('-') else {
            continue;
        };
        if !FAMILIES.contains(&prefix) {
            continue;
        }
        if !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()) {
            out.push(token.to_owned());
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Remove fenced blocks and inline code.
fn strip_code(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_fence = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let mut in_code = false;
        for ch in line.chars() {
            if ch == '`' {
                in_code = !in_code;
                continue;
            }
            if !in_code {
                out.push(ch);
            }
        }
        out.push('\n');
    }
    out
}

/// Shorten a title for a finding.
fn shorten(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        return s.to_owned();
    }
    s.chars().take(n).collect::<String>() + "…"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_sentence_that_only_names_an_identifier_is_reported() {
        assert_eq!(unexpanded("Waiting on TQ-32."), vec!["TQ-32".to_owned()]);
        assert_eq!(unexpanded("Blocked on TQ-30."), vec!["TQ-30".to_owned()]);
    }

    #[test]
    fn a_sentence_that_says_what_the_identifier_is_about_is_not() {
        assert!(
            unexpanded(
                "Blocked on TQ-30, which asks whether the desktop app may write at all, \
                 because hard constraint 7 says it may not."
            )
            .is_empty()
        );
    }

    // The exemption that makes the rule usable. An identifier in backticks is
    // being named — in a list of what a parser recognises, say — rather than
    // leaned on, and refusing those would make writing about the check fail it.
    #[test]
    fn an_identifier_inside_code_is_left_alone() {
        assert!(unexpanded("The parser knows `TQ-14` and `B-12`.").is_empty());
        assert!(unexpanded("```\nTQ-14\n```\nThe fence above is quoted, not cited.").is_empty());
    }

    #[test]
    fn a_prefix_that_is_not_an_identifier_family_is_ignored() {
        // Version numbers, dates and hyphenated words all look like this.
        assert!(unexpanded("Shipped in 0-1.").is_empty());
        assert!(unexpanded("See PR-14.").is_empty());
    }

    #[test]
    fn the_report_counts_each_rule_separately() {
        let report = LintReport {
            findings: vec![
                LintFinding {
                    check: TASK_WITHOUT_SUMMARY,
                    id: EntityId::generate(EntityType::Task),
                    reference: "KEEL-1".to_owned(),
                    detail: String::new(),
                },
                LintFinding {
                    check: TASK_WITHOUT_SUMMARY,
                    id: EntityId::generate(EntityType::Task),
                    reference: "KEEL-2".to_owned(),
                    detail: String::new(),
                },
                LintFinding {
                    check: UNEXPANDED_IDENTIFIER,
                    id: EntityId::generate(EntityType::Task),
                    reference: "KEEL-3".to_owned(),
                    detail: String::new(),
                },
            ],
            scanned: 3,
            total: 3,
            truncated: false,
            counts: vec![(TASK_WITHOUT_SUMMARY, 2), (UNEXPANDED_IDENTIFIER, 1)],
        };
        assert_eq!(report.count_of(TASK_WITHOUT_SUMMARY), 2);
        assert_eq!(report.count_of(UNEXPANDED_IDENTIFIER), 1);
        assert_eq!(
            report.by_check(),
            [(TASK_WITHOUT_SUMMARY, 2), (UNEXPANDED_IDENTIFIER, 1)],
            "a rule with no findings is absent rather than reported as zero"
        );
        assert_eq!(report.count_of(CLOSED_WITHOUT_REASON), 0);
    }
}
