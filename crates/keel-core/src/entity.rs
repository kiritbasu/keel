//! The thirteen artifact types, and the vocabulary shared across all of them.
//!
//! Thirteen is a ceiling, not a starting point (PRD R-1). The enum is closed
//! and exhaustive matching is used everywhere on purpose: adding a fourteenth
//! variant should break the build in a dozen places and force a conversation,
//! rather than slipping in behind a `_ => {}` arm.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;

/// One of Keel's thirteen artifact types.
///
/// The serialised form is the singular snake-case name — `task`, `design`,
/// `metric_observation`. That string is what lands in `links.from_type`,
/// `events.entity_type` and `documents.entity_type`, and what an agent passes
/// as the `type` argument over MCP. It is *not* always the table name; see
/// [`EntityType::table`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    /// The root container. Everything else belongs to exactly one, except
    /// global terms.
    Project,
    /// A planning or shipping unit. Replaces "epic".
    Milestone,
    /// A unit of work: task, bug, chore or spike.
    Task,
    /// A prose document: PRD, spec, RFC, design doc or note.
    Spec,
    /// An architecture decision record.
    Decision,
    /// An open unknown: question, risk or assumption.
    Question,
    /// A glossary entry. May be global rather than project-scoped.
    Term,
    /// Raw input from the world: interview, support, sales, idea, competitor,
    /// observation.
    Feedback,
    /// A mockup, wireframe, screenshot or Figma node.
    Design,
    /// A deployment target.
    Environment,
    /// A named measure with a target.
    Metric,
    /// One timestamped value of a metric.
    MetricObservation,
    /// The escape hatch: files and links that fit nowhere else.
    Artifact,
}

impl EntityType {
    /// Every type, in a stable order.
    ///
    /// Stable because it drives `fsck`'s reporting order, the fixture loader,
    /// and the unified vertex view — all of which produce diffs a human reads.
    pub const ALL: [EntityType; 13] = [
        EntityType::Project,
        EntityType::Milestone,
        EntityType::Task,
        EntityType::Spec,
        EntityType::Decision,
        EntityType::Question,
        EntityType::Term,
        EntityType::Feedback,
        EntityType::Design,
        EntityType::Environment,
        EntityType::Metric,
        EntityType::MetricObservation,
        EntityType::Artifact,
    ];

    /// The wire name — what appears in `links.from_type`, `events.entity_type`
    /// and MCP arguments.
    pub const fn as_str(self) -> &'static str {
        match self {
            EntityType::Project => "project",
            EntityType::Milestone => "milestone",
            EntityType::Task => "task",
            EntityType::Spec => "spec",
            EntityType::Decision => "decision",
            EntityType::Question => "question",
            EntityType::Term => "term",
            EntityType::Feedback => "feedback",
            EntityType::Design => "design",
            EntityType::Environment => "environment",
            EntityType::Metric => "metric",
            EntityType::MetricObservation => "metric_observation",
            EntityType::Artifact => "artifact",
        }
    }

    /// The DuckDB table this type lives in.
    ///
    /// Separate from [`EntityType::as_str`] because two of them disagree:
    /// `design` is stored in `design_artifacts` and `feedback` is its own
    /// plural-less table. Deriving one from the other by appending an `s`
    /// would be wrong in exactly those two places, which is the kind of bug
    /// that only shows up for the artifact types you test last.
    pub const fn table(self) -> &'static str {
        match self {
            EntityType::Project => "projects",
            EntityType::Milestone => "milestones",
            EntityType::Task => "tasks",
            EntityType::Spec => "specs",
            EntityType::Decision => "decisions",
            EntityType::Question => "questions",
            EntityType::Term => "terms",
            EntityType::Feedback => "feedback",
            EntityType::Design => "design_artifacts",
            EntityType::Environment => "environments",
            EntityType::Metric => "metrics",
            EntityType::MetricObservation => "metric_observations",
            EntityType::Artifact => "artifacts",
        }
    }

    /// The three-letter ULID prefix, e.g. `tsk` for `tsk_01H8…`.
    pub const fn prefix(self) -> &'static str {
        match self {
            EntityType::Project => "prj",
            EntityType::Milestone => "mst",
            EntityType::Task => "tsk",
            EntityType::Spec => "spc",
            EntityType::Decision => "dec",
            EntityType::Question => "que",
            EntityType::Term => "trm",
            EntityType::Feedback => "fbk",
            EntityType::Design => "dsg",
            EntityType::Environment => "env",
            EntityType::Metric => "mtr",
            EntityType::MetricObservation => "obs",
            EntityType::Artifact => "art",
        }
    }

    /// Whether this type's body lives in the Lance `documents` dataset.
    ///
    /// The five that do are exactly SPEC §2.1's `entity_type` domain. Any type
    /// answering `true` here must also carry `current_doc_version` in DuckDB —
    /// `fsck` checks that the two agree.
    pub const fn has_document(self) -> bool {
        matches!(
            self,
            EntityType::Spec
                | EntityType::Decision
                | EntityType::Question
                | EntityType::Feedback
                | EntityType::Design
        )
    }

    /// Whether rows of this type carry a `project_id`, and whether it may be
    /// null.
    ///
    /// Three shapes exist and the difference matters for validation:
    /// `Project` has no such column at all, `Term` has a nullable one (null
    /// means global, per Q-4), and everything else requires it.
    pub const fn project_scope(self) -> ProjectScope {
        match self {
            EntityType::Project => ProjectScope::IsTheProject,
            EntityType::Term => ProjectScope::Optional,
            _ => ProjectScope::Required,
        }
    }

    /// Whether this type participates in text search at all.
    ///
    /// Metrics and observations are excluded by design (REQ-4): they are
    /// numeric, and reaching them is a filter rather than a query.
    pub const fn is_searchable(self) -> bool {
        !matches!(self, EntityType::Metric | EntityType::MetricObservation)
    }

    /// Parse a wire name back into a type.
    pub fn parse(s: &str) -> Result<Self> {
        EntityType::ALL
            .into_iter()
            .find(|t| t.as_str() == s)
            .ok_or_else(|| Error::MalformedId {
                supplied: s.to_owned(),
                problem: format!("`{s}` is not a Keel entity type"),
                expected: Self::wire_names().join(" | "),
            })
    }

    /// Parse a three-letter ULID prefix back into a type.
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        EntityType::ALL.into_iter().find(|t| t.prefix() == prefix)
    }

    /// Every wire name, for building error messages that tell a model what it
    /// could have said instead.
    pub fn wire_names() -> Vec<&'static str> {
        EntityType::ALL
            .into_iter()
            .map(EntityType::as_str)
            .collect()
    }
}

impl fmt::Display for EntityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How a type relates to the project that owns it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectScope {
    /// The type *is* the project — there is no `project_id` column.
    IsTheProject,
    /// `project_id` is `NOT NULL`.
    Required,
    /// `project_id` may be null, meaning global. Only `terms`.
    Optional,
}

/// Who performed an act.
///
/// SPEC §3.1 calls this "provenance vocabulary": one concept in two shapes.
/// Entity rows record state (`created_by`, `updated_by`), the event log
/// records the act (`actor`), and both draw from this set. An entity's
/// `updated_by` always equals the `actor` of the event that produced it —
/// `fsck` asserts exactly that.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Actor {
    /// KB, typing directly.
    Human,
    /// A Claude session, on any surface.
    Claude,
    /// The GitHub App, acting on a webhook.
    Github,
    /// Keel itself — migrations, fixtures, scheduled jobs.
    System,
}

impl Actor {
    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Actor::Human => "human",
            Actor::Claude => "claude",
            Actor::Github => "github",
            Actor::System => "system",
        }
    }

    /// Parse a stored string.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "human" => Ok(Actor::Human),
            "claude" => Ok(Actor::Claude),
            "github" => Ok(Actor::Github),
            "system" => Ok(Actor::System),
            other => Err(Error::MalformedId {
                supplied: other.to_owned(),
                problem: format!("`{other}` is not a known actor"),
                expected: "human | claude | github | system".to_owned(),
            }),
        }
    }
}

impl fmt::Display for Actor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where an act happened.
///
/// SPEC §3.1's audit block lists four values; §6.5 additionally names `cli` as
/// a fixed sentinel for the command line. The two passages disagree, and this
/// enum reconciles them by carrying all five — see DECISIONS B-8. The column
/// is a bare `VARCHAR` with no check constraint, so this costs nothing at the
/// storage layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Surface {
    /// Claude chat.
    Chat,
    /// Cowork.
    Cowork,
    /// Claude Code.
    Code,
    /// The Tauri desktop app.
    Ui,
    /// `keel-cli`.
    Cli,
}

impl Surface {
    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Surface::Chat => "chat",
            Surface::Cowork => "cowork",
            Surface::Code => "code",
            Surface::Ui => "ui",
            Surface::Cli => "cli",
        }
    }

    /// Parse a stored string.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "chat" => Ok(Surface::Chat),
            "cowork" => Ok(Surface::Cowork),
            "code" => Ok(Surface::Code),
            "ui" => Ok(Surface::Ui),
            "cli" => Ok(Surface::Cli),
            other => Err(Error::MalformedId {
                supplied: other.to_owned(),
                problem: format!("`{other}` is not a known surface"),
                expected: "chat | cowork | code | ui | cli".to_owned(),
            }),
        }
    }
}

impl fmt::Display for Surface {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn wire_names_round_trip() {
        for t in EntityType::ALL {
            assert_eq!(EntityType::parse(t.as_str()).unwrap(), t);
        }
    }

    #[test]
    fn prefixes_round_trip_and_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for t in EntityType::ALL {
            assert!(seen.insert(t.prefix()), "duplicate prefix {}", t.prefix());
            assert_eq!(EntityType::from_prefix(t.prefix()), Some(t));
            assert_eq!(t.prefix().len(), 3, "{t} prefix must be three characters");
        }
    }

    #[test]
    fn table_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for t in EntityType::ALL {
            assert!(seen.insert(t.table()), "duplicate table {}", t.table());
        }
    }

    #[test]
    fn design_and_feedback_tables_are_not_the_wire_name_plus_s() {
        // The two cases that would break a naive pluralisation. Asserted so a
        // future refactor to `format!("{}s", self.as_str())` fails loudly.
        assert_eq!(EntityType::Design.table(), "design_artifacts");
        assert_eq!(EntityType::Feedback.table(), "feedback");
    }

    #[test]
    fn exactly_five_types_carry_documents() {
        let with_docs: Vec<_> = EntityType::ALL
            .into_iter()
            .filter(|t| t.has_document())
            .collect();
        assert_eq!(
            with_docs,
            vec![
                EntityType::Spec,
                EntityType::Decision,
                EntityType::Question,
                EntityType::Feedback,
                EntityType::Design,
            ],
            "SPEC §2.1 fixes the documents dataset's entity_type domain at these five"
        );
    }

    #[test]
    fn metrics_are_excluded_from_search() {
        assert!(!EntityType::Metric.is_searchable());
        assert!(!EntityType::MetricObservation.is_searchable());
        assert_eq!(
            EntityType::ALL
                .into_iter()
                .filter(|t| t.is_searchable())
                .count(),
            11
        );
    }

    #[test]
    fn unknown_type_names_say_what_was_valid() {
        let err = EntityType::parse("epic").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("epic"),
            "should quote what was supplied: {msg}"
        );
        assert!(
            msg.contains("milestone"),
            "should list valid options: {msg}"
        );
    }

    #[test]
    fn actor_and_surface_reject_nonsense() {
        assert!(Actor::parse("robot").is_err());
        assert!(Surface::parse("fax").is_err());
        for a in [Actor::Human, Actor::Claude, Actor::Github, Actor::System] {
            assert_eq!(Actor::parse(a.as_str()).unwrap(), a);
        }
        for s in [
            Surface::Chat,
            Surface::Cowork,
            Surface::Code,
            Surface::Ui,
            Surface::Cli,
        ] {
            assert_eq!(Surface::parse(s.as_str()).unwrap(), s);
        }
    }
}
