//! The thirteen artifact structs, and the [`Entity`] enum that unifies them.
//!
//! Each struct mirrors its DuckDB table from SPEC §3.2 field for field. Where
//! the schema and this file disagree, the schema wins and this file is the
//! bug — `keel-cli fsck` exists partly to catch that drift.
//!
//! Optional fields are `Option`, list columns are `Vec`, and every struct ends
//! with an [`Audit`] block. Nothing here validates itself on construction:
//! validation happens on the way into storage, in one place, so that the same
//! rules apply whether a value arrived from MCP, the CLI or a fixture.

use crate::{
    ArtifactKind, Audit, BlobId, DecisionStatus, DesignState, EntityId, EntityType,
    EnvironmentStatus, Error, FeedbackKind, MetricDirection, MilestoneKind, MilestoneStatus,
    ProjectStatus, Provenance, QuestionKind, QuestionStatus, Result, RiskSeverity, Sentiment,
    SpecKind, SpecStatus, TaskKind, TaskPriority, TaskStatus,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Build the audit block a freshly constructed entity carries before storage
/// stamps it for real.
///
/// Constructors need *something* in the field, and leaving it `Option` would
/// put a null check on every read for the sake of a window that lasts until
/// the next function call. The store overwrites this in `create`.
fn provisional_audit() -> Audit {
    Audit::new(&Provenance::anonymous(crate::Actor::System), Utc::now())
}

/// Normalise a title for idempotency-key derivation.
///
/// Lowercased, trimmed, and internal whitespace collapsed, so that "Add
/// login  page", "add login page" and " Add Login Page " are one task rather
/// than three. This is the cheapest defence against R-6 (write amplification)
/// that costs nothing when it is not needed.
fn normalise_for_key(s: &str) -> String {
    s.split_whitespace()
        .map(str::to_lowercase)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Derive the default idempotency key for a create, per SPEC §7.2.
///
/// `hash(project_id, type, normalised_title)`. Truncated to 32 hex characters:
/// at Keel's scale that is 128 bits of collision resistance against a corpus
/// of thousands, and a full digest just makes the rows harder to read when
/// debugging.
pub fn derive_idempotency_key(
    project_id: Option<&EntityId>,
    entity_type: EntityType,
    natural_key: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(project_id.map(EntityId::as_str).unwrap_or("").as_bytes());
    hasher.update(b"\x1f");
    hasher.update(entity_type.as_str().as_bytes());
    hasher.update(b"\x1f");
    hasher.update(normalise_for_key(natural_key).as_bytes());
    let digest = hasher.finalize();
    digest.iter().take(16).map(|b| format!("{b:02x}")).collect()
}

/// The root container. Everything belongs to exactly one, except global terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// `prj_…`
    pub id: EntityId,
    /// URL-safe short name, unique across the store.
    pub slug: String,
    /// Display name.
    pub name: String,
    /// One or two lines on what this is.
    pub description: Option<String>,
    /// Whether it is being worked on.
    pub status: ProjectStatus,
    /// Repository URLs. Used by `keel_projects` for disambiguation (§6.4).
    pub repo_urls: Vec<String>,
    /// Local checkout, which is where the markdown mirror is written.
    pub root_path: Option<String>,
    /// Where the generated tracker goes, relative to `root_path`.
    ///
    /// Separate from the mirror because the tracker is task-shaped and the
    /// mirror is deliberately prose-only (TQ-5), but it is still a generated
    /// repository file. `None` means this project does not want one.
    pub status_path: Option<String>,
    /// Other names this project goes by. The main defence against UC-8's
    /// nine-near-duplicate-projects failure.
    pub aliases: Vec<String>,
    /// Idempotency key, unique across projects.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Project {
    /// A new project with the required fields.
    pub fn new(slug: impl Into<String>, name: impl Into<String>) -> Self {
        let slug = slug.into();
        let name = name.into();
        Project {
            id: EntityId::generate(EntityType::Project),
            idempotency_key: derive_idempotency_key(None, EntityType::Project, &slug),
            slug,
            name,
            description: None,
            status: ProjectStatus::default(),
            repo_urls: Vec::new(),
            root_path: None,
            status_path: None,
            aliases: Vec::new(),
            audit: provisional_audit(),
        }
    }
}

/// A planning or shipping unit. Replaces "epic".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Milestone {
    /// `mst_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Planning unit or release.
    pub kind: MilestoneKind,
    /// Display name.
    pub name: String,
    /// One line on what it covers.
    pub summary: Option<String>,
    /// Where it stands.
    pub status: MilestoneStatus,
    /// When it is meant to land.
    pub target_date: Option<NaiveDate>,
    /// When it actually landed.
    pub shipped_at: Option<DateTime<Utc>>,
    /// Releases only.
    pub version_string: Option<String>,
    /// Manual ordering for the roadmap view.
    pub sort_order: Option<i32>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Milestone {
    /// A new milestone with the required fields.
    pub fn new(project_id: EntityId, name: impl Into<String>) -> Self {
        let name = name.into();
        Milestone {
            id: EntityId::generate(EntityType::Milestone),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::Milestone,
                &name,
            ),
            project_id,
            kind: MilestoneKind::default(),
            name,
            summary: None,
            status: MilestoneStatus::default(),
            target_date: None,
            shipped_at: None,
            version_string: None,
            sort_order: None,
            audit: provisional_audit(),
        }
    }
}

/// A unit of work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Task {
    /// `tsk_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// The milestone this serves, if any.
    pub milestone_id: Option<EntityId>,
    /// Task, bug, chore or spike.
    pub kind: TaskKind,
    /// One line naming the work.
    pub title: String,
    /// Short detail. Anything long-form belongs in a spec, linked with
    /// `implements`.
    pub body: Option<String>,
    /// Where it stands.
    pub status: TaskStatus,
    /// How urgent.
    pub priority: TaskPriority,
    /// Free-text labels.
    pub labels: Vec<String>,
    /// PR or issue URL.
    pub external_ref: Option<String>,
    /// When it reached a terminal status.
    pub closed_at: Option<DateTime<Utc>>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Task {
    /// A new task with the required fields.
    pub fn new(project_id: EntityId, title: impl Into<String>) -> Self {
        let title = title.into();
        Task {
            id: EntityId::generate(EntityType::Task),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Task, &title),
            project_id,
            milestone_id: None,
            kind: TaskKind::default(),
            title,
            body: None,
            status: TaskStatus::default(),
            priority: TaskPriority::default(),
            labels: Vec::new(),
            external_ref: None,
            closed_at: None,
            audit: provisional_audit(),
        }
    }
}

/// A prose document's header. The body lives in the Lance `documents` dataset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Spec {
    /// `spc_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// PRD, spec, RFC, design doc or note.
    pub kind: SpecKind,
    /// Display title.
    pub title: String,
    /// How settled it is.
    pub status: SpecStatus,
    /// Pointer into `documents.version`. Zero means no body has been written
    /// yet — deliberately distinct from version 1, so "created but empty" is
    /// visible rather than inferred.
    pub current_doc_version: i32,
    /// Where the generated markdown mirror of this document lives.
    pub mirror_path: Option<String>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Spec {
    /// A new spec with the required fields.
    pub fn new(project_id: EntityId, title: impl Into<String>) -> Self {
        let title = title.into();
        Spec {
            id: EntityId::generate(EntityType::Spec),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Spec, &title),
            project_id,
            kind: SpecKind::default(),
            title,
            status: SpecStatus::default(),
            current_doc_version: 0,
            mirror_path: None,
            audit: provisional_audit(),
        }
    }
}

/// An architecture decision record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// `dec_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Display title.
    pub title: String,
    /// Proposed, accepted, superseded or rejected. Content becomes immutable
    /// at `accepted`; that rule is enforced in `keel-core`, not the schema.
    pub status: DecisionStatus,
    /// When it was accepted.
    pub decided_at: Option<DateTime<Utc>>,
    /// Pointer into `documents.version`.
    pub current_doc_version: i32,
    /// Where the generated markdown mirror lives.
    pub mirror_path: Option<String>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Decision {
    /// A new decision with the required fields.
    pub fn new(project_id: EntityId, title: impl Into<String>) -> Self {
        let title = title.into();
        Decision {
            id: EntityId::generate(EntityType::Decision),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::Decision,
                &title,
            ),
            project_id,
            title,
            status: DecisionStatus::default(),
            decided_at: None,
            current_doc_version: 0,
            mirror_path: None,
            audit: provisional_audit(),
        }
    }
}

/// An open unknown: question, risk or assumption.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// `que_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Question, risk or assumption.
    pub kind: QuestionKind,
    /// The unknown, stated in one line.
    pub title: String,
    /// Where it stands.
    pub status: QuestionStatus,
    /// Risks only.
    pub severity: Option<RiskSeverity>,
    /// When it stopped being open.
    pub resolved_at: Option<DateTime<Utc>>,
    /// Pointer into `documents.version` — the full body is a document like any
    /// other.
    pub current_doc_version: i32,
    /// Where this appears in the mirror. Points at the shared `questions.md`,
    /// so it answers "where does this show up", not "which file is this".
    pub mirror_path: Option<String>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Question {
    /// A new question with the required fields.
    pub fn new(project_id: EntityId, title: impl Into<String>) -> Self {
        let title = title.into();
        Question {
            id: EntityId::generate(EntityType::Question),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::Question,
                &title,
            ),
            project_id,
            kind: QuestionKind::default(),
            title,
            status: QuestionStatus::default(),
            severity: None,
            resolved_at: None,
            current_doc_version: 0,
            mirror_path: None,
            audit: provisional_audit(),
        }
    }
}

/// A glossary entry. Global when `project_id` is `None`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Term {
    /// `trm_…`
    pub id: EntityId,
    /// Owning project, or `None` for a global term. Per-project terms override
    /// a global of the same name; resolution is project-first (Q-4).
    pub project_id: Option<EntityId>,
    /// The word.
    pub term: String,
    /// What it means *in this project*.
    pub definition: String,
    /// Other spellings.
    pub aliases: Vec<String>,
    /// Where this appears in the mirror — the shared `glossary.md`.
    pub mirror_path: Option<String>,
    /// Idempotency key, unique within the project (or globally).
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Term {
    /// A new term. Pass `None` for `project_id` to define it globally.
    pub fn new(
        project_id: Option<EntityId>,
        term: impl Into<String>,
        definition: impl Into<String>,
    ) -> Self {
        let term = term.into();
        Term {
            id: EntityId::generate(EntityType::Term),
            idempotency_key: derive_idempotency_key(project_id.as_ref(), EntityType::Term, &term),
            project_id,
            term,
            definition: definition.into(),
            aliases: Vec::new(),
            mirror_path: None,
            audit: provisional_audit(),
        }
    }
}

/// Raw input from the world. The verbatim body lives in `documents`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Feedback {
    /// `fbk_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Where it came from.
    pub kind: FeedbackKind,
    /// Who or where, in free text.
    pub source: Option<String>,
    /// A way to reach them.
    pub contact: Option<String>,
    /// How they felt.
    pub sentiment: Option<Sentiment>,
    /// When it happened, which is usually not when it was recorded.
    pub occurred_at: Option<DateTime<Utc>>,
    /// Whether it has been looked at and turned into something.
    pub triaged: bool,
    /// Pointer into `documents.version` — the verbatim body.
    pub current_doc_version: i32,
    /// A short label for lists and search results. Not a schema column; see
    /// [`Entity::label`].
    pub summary: String,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Feedback {
    /// A new feedback item. `summary` is the one-line label; the verbatim body
    /// is written separately as a document revision.
    pub fn new(project_id: EntityId, summary: impl Into<String>) -> Self {
        let summary = summary.into();
        Feedback {
            id: EntityId::generate(EntityType::Feedback),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::Feedback,
                &summary,
            ),
            project_id,
            kind: FeedbackKind::default(),
            source: None,
            contact: None,
            sentiment: None,
            occurred_at: None,
            triaged: false,
            current_doc_version: 0,
            summary,
            audit: provisional_audit(),
        }
    }
}

/// A mockup, wireframe, screenshot or Figma node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Design {
    /// `dsg_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Display name.
    pub name: String,
    /// Proposed, approved or built. UC-5 renders these side by side.
    pub state: DesignState,
    /// A Figma node reference, if it came from there.
    pub figma_ref: Option<String>,
    /// The stored image in the Lance `blobs` dataset.
    pub blob_id: Option<BlobId>,
    /// Pointer into `documents.version` — caption and rationale.
    pub current_doc_version: i32,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Design {
    /// A new design artifact with the required fields.
    pub fn new(project_id: EntityId, name: impl Into<String>) -> Self {
        let name = name.into();
        Design {
            id: EntityId::generate(EntityType::Design),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Design, &name),
            project_id,
            name,
            state: DesignState::default(),
            figma_ref: None,
            blob_id: None,
            current_doc_version: 0,
            audit: provisional_audit(),
        }
    }
}

/// A deployment target. Answers "what is actually live".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    /// `env_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// production, staging, preview, …
    pub name: String,
    /// Where it is.
    pub url: Option<String>,
    /// The shipped application version.
    ///
    /// Named distinctly from `current_doc_version` on purpose: one is the
    /// deployed build, the other is a document revision pointer, and a single
    /// shared name would eventually get them confused.
    pub deployed_version: Option<String>,
    /// The commit that build came from.
    pub deployed_commit: Option<String>,
    /// Whether it is healthy.
    pub status: EnvironmentStatus,
    /// When it last changed.
    pub last_deployed_at: Option<DateTime<Utc>>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Environment {
    /// A new environment with the required fields.
    pub fn new(project_id: EntityId, name: impl Into<String>) -> Self {
        let name = name.into();
        Environment {
            id: EntityId::generate(EntityType::Environment),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::Environment,
                &name,
            ),
            project_id,
            name,
            url: None,
            deployed_version: None,
            deployed_commit: None,
            status: EnvironmentStatus::default(),
            last_deployed_at: None,
            audit: provisional_audit(),
        }
    }
}

/// A named measure with a target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metric {
    /// `mtr_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// What is being measured.
    pub name: String,
    /// The unit, for display.
    pub unit: Option<String>,
    /// The number that counts as success. PRD success criteria are fiction
    /// without this.
    pub target_value: Option<f64>,
    /// Which way is good.
    pub direction: MetricDirection,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Metric {
    /// A new metric with the required fields.
    pub fn new(project_id: EntityId, name: impl Into<String>) -> Self {
        let name = name.into();
        Metric {
            id: EntityId::generate(EntityType::Metric),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Metric, &name),
            project_id,
            name,
            unit: None,
            target_value: None,
            direction: MetricDirection::default(),
            audit: provisional_audit(),
        }
    }
}

/// One timestamped value of a metric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricObservation {
    /// `obs_…`
    pub id: EntityId,
    /// The metric this observes.
    pub metric_id: EntityId,
    /// Denormalised from the metric, so filtering by project does not need a
    /// join.
    pub project_id: EntityId,
    /// The measured value.
    pub value: f64,
    /// When it was measured, which is not when it was recorded.
    pub observed_at: DateTime<Utc>,
    /// Anything worth saying about this reading.
    pub note: Option<String>,
    /// Idempotency key. Derived from metric and timestamp rather than a title,
    /// because an observation has no title and re-recording the same reading
    /// twice is exactly the duplicate worth suppressing.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl MetricObservation {
    /// A new observation.
    pub fn new(
        metric_id: EntityId,
        project_id: EntityId,
        value: f64,
        observed_at: DateTime<Utc>,
    ) -> Self {
        MetricObservation {
            id: EntityId::generate(EntityType::MetricObservation),
            idempotency_key: derive_idempotency_key(
                Some(&project_id),
                EntityType::MetricObservation,
                &format!("{metric_id}@{}", observed_at.to_rfc3339()),
            ),
            metric_id,
            project_id,
            value,
            observed_at,
            note: None,
            audit: provisional_audit(),
        }
    }
}

/// The escape hatch: files and links that fit nowhere else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    /// `art_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// Display name.
    pub name: String,
    /// Link, file, image or other.
    pub kind: ArtifactKind,
    /// Where it points.
    pub url: Option<String>,
    /// The stored bytes, if any.
    pub blob_id: Option<BlobId>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Artifact {
    /// A new artifact with the required fields.
    pub fn new(project_id: EntityId, name: impl Into<String>) -> Self {
        let name = name.into();
        Artifact {
            id: EntityId::generate(EntityType::Artifact),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Artifact, &name),
            project_id,
            name,
            kind: ArtifactKind::default(),
            url: None,
            blob_id: None,
            audit: provisional_audit(),
        }
    }
}

/// Any one of the thirteen, for the polymorphic paths: `keel_get`, search
/// results, the event log, the fixture loader.
///
/// Matching on this is always exhaustive. That is the point — a fourteenth
/// artifact type should not be addable without the compiler listing every
/// place that has to think about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Entity {
    /// A project.
    Project(Project),
    /// A milestone.
    Milestone(Milestone),
    /// A task.
    Task(Task),
    /// A spec.
    Spec(Spec),
    /// A decision.
    Decision(Decision),
    /// A question.
    Question(Question),
    /// A term.
    Term(Term),
    /// A feedback item.
    Feedback(Feedback),
    /// A design artifact.
    Design(Design),
    /// An environment.
    Environment(Environment),
    /// A metric.
    Metric(Metric),
    /// A metric observation.
    MetricObservation(MetricObservation),
    /// A generic artifact.
    Artifact(Artifact),
}

/// Apply the same expression to whichever variant is present.
macro_rules! dispatch {
    ($self:expr, $inner:ident => $body:expr) => {
        match $self {
            Entity::Project($inner) => $body,
            Entity::Milestone($inner) => $body,
            Entity::Task($inner) => $body,
            Entity::Spec($inner) => $body,
            Entity::Decision($inner) => $body,
            Entity::Question($inner) => $body,
            Entity::Term($inner) => $body,
            Entity::Feedback($inner) => $body,
            Entity::Design($inner) => $body,
            Entity::Environment($inner) => $body,
            Entity::Metric($inner) => $body,
            Entity::MetricObservation($inner) => $body,
            Entity::Artifact($inner) => $body,
        }
    };
}

impl Entity {
    /// Which of the thirteen this is.
    pub fn entity_type(&self) -> EntityType {
        dispatch!(self, e => e.id.entity_type())
    }

    /// The identifier.
    pub fn id(&self) -> &EntityId {
        dispatch!(self, e => &e.id)
    }

    /// The owning project, if this type has one.
    ///
    /// `None` has two distinct meanings and callers must not conflate them: a
    /// `Project` *is* the scope, while a global `Term` deliberately has no
    /// project. [`EntityType::project_scope`] distinguishes them.
    pub fn project_id(&self) -> Option<&EntityId> {
        match self {
            Entity::Project(_) => None,
            Entity::Term(t) => t.project_id.as_ref(),
            Entity::Milestone(e) => Some(&e.project_id),
            Entity::Task(e) => Some(&e.project_id),
            Entity::Spec(e) => Some(&e.project_id),
            Entity::Decision(e) => Some(&e.project_id),
            Entity::Question(e) => Some(&e.project_id),
            Entity::Feedback(e) => Some(&e.project_id),
            Entity::Design(e) => Some(&e.project_id),
            Entity::Environment(e) => Some(&e.project_id),
            Entity::Metric(e) => Some(&e.project_id),
            Entity::MetricObservation(e) => Some(&e.project_id),
            Entity::Artifact(e) => Some(&e.project_id),
        }
    }

    /// The audit block.
    pub fn audit(&self) -> &Audit {
        dispatch!(self, e => &e.audit)
    }

    /// The audit block, mutably. Used by the storage layer to stamp
    /// provenance; callers should not reach for this.
    pub fn audit_mut(&mut self) -> &mut Audit {
        dispatch!(self, e => &mut e.audit)
    }

    /// The idempotency key.
    pub fn idempotency_key(&self) -> &str {
        dispatch!(self, e => &e.idempotency_key)
    }

    /// Replace the idempotency key with a caller-supplied one.
    pub fn set_idempotency_key(&mut self, key: impl Into<String>) {
        let key = key.into();
        dispatch!(self, e => e.idempotency_key = key);
    }

    /// The one-line human label: whatever this type calls its name.
    ///
    /// Exists because `name`, `title`, `term` and `summary` are four different
    /// column names for the same idea, and every list, search result, mirror
    /// header and event summary needs one of them.
    pub fn label(&self) -> &str {
        match self {
            Entity::Project(e) => &e.name,
            Entity::Milestone(e) => &e.name,
            Entity::Task(e) => &e.title,
            Entity::Spec(e) => &e.title,
            Entity::Decision(e) => &e.title,
            Entity::Question(e) => &e.title,
            Entity::Term(e) => &e.term,
            Entity::Feedback(e) => &e.summary,
            Entity::Design(e) => &e.name,
            Entity::Environment(e) => &e.name,
            Entity::Metric(e) => &e.name,
            Entity::MetricObservation(e) => e.note.as_deref().unwrap_or("observation"),
            Entity::Artifact(e) => &e.name,
        }
    }

    /// Where this artifact's prose belongs in the repository, if it has
    /// adopted a file.
    ///
    /// `None` for the nine types that carry no prose, and for prose artifacts
    /// that were born in Keel and have no natural home in a repository —
    /// those go to the `.keel/` mirror at a generated path instead.
    pub fn mirror_path(&self) -> Option<&str> {
        match self {
            Entity::Spec(e) => e.mirror_path.as_deref(),
            Entity::Decision(e) => e.mirror_path.as_deref(),
            Entity::Question(e) => e.mirror_path.as_deref(),
            Entity::Term(e) => e.mirror_path.as_deref(),
            _ => None,
        }
    }

    /// The current status as a string, for types that have one.
    ///
    /// `None` for the four types with no lifecycle — term, metric,
    /// observation, artifact. Callers rendering a status column should show
    /// nothing rather than inventing one.
    pub fn status(&self) -> Option<&'static str> {
        match self {
            Entity::Project(e) => Some(e.status.as_str()),
            Entity::Milestone(e) => Some(e.status.as_str()),
            Entity::Task(e) => Some(e.status.as_str()),
            Entity::Spec(e) => Some(e.status.as_str()),
            Entity::Decision(e) => Some(e.status.as_str()),
            Entity::Question(e) => Some(e.status.as_str()),
            Entity::Design(e) => Some(e.state.as_str()),
            Entity::Environment(e) => Some(e.status.as_str()),
            Entity::Term(_)
            | Entity::Feedback(_)
            | Entity::Metric(_)
            | Entity::MetricObservation(_)
            | Entity::Artifact(_) => None,
        }
    }

    /// The current document revision pointer, for prose-bearing types.
    ///
    /// Always `Some` exactly when [`EntityType::has_document`] is true — a
    /// property `fsck` asserts, because a mismatch means either a body with no
    /// pointer or a pointer with no body.
    pub fn current_doc_version(&self) -> Option<i32> {
        match self {
            Entity::Spec(e) => Some(e.current_doc_version),
            Entity::Decision(e) => Some(e.current_doc_version),
            Entity::Question(e) => Some(e.current_doc_version),
            Entity::Feedback(e) => Some(e.current_doc_version),
            Entity::Design(e) => Some(e.current_doc_version),
            _ => None,
        }
    }

    /// Set the document revision pointer. Fails loudly for types that have no
    /// document, rather than silently doing nothing.
    pub fn set_current_doc_version(&mut self, version: i32) -> Result<()> {
        match self {
            Entity::Spec(e) => e.current_doc_version = version,
            Entity::Decision(e) => e.current_doc_version = version,
            Entity::Question(e) => e.current_doc_version = version,
            Entity::Feedback(e) => e.current_doc_version = version,
            Entity::Design(e) => e.current_doc_version = version,
            other => {
                return Err(Error::Invariant {
                    operation: format!("set current_doc_version on {}", other.id()),
                    problem: format!(
                        "{} has no prose body; only spec, decision, question, feedback and design do",
                        other.entity_type()
                    ),
                });
            }
        }
        Ok(())
    }
}

/// Every entity struct can be lifted into the enum.
macro_rules! impl_from {
    ($($variant:ident($ty:ty)),+ $(,)?) => {$(
        impl From<$ty> for Entity {
            fn from(v: $ty) -> Entity { Entity::$variant(v) }
        }
    )+};
}

impl_from!(
    Project(Project),
    Milestone(Milestone),
    Task(Task),
    Spec(Spec),
    Decision(Decision),
    Question(Question),
    Term(Term),
    Feedback(Feedback),
    Design(Design),
    Environment(Environment),
    Metric(Metric),
    MetricObservation(MetricObservation),
    Artifact(Artifact),
);

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn project() -> EntityId {
        EntityId::generate(EntityType::Project)
    }

    /// One of every type, for sweeps that must cover all thirteen.
    fn one_of_each() -> Vec<Entity> {
        let p = project();
        let metric = Metric::new(p.clone(), "activation rate");
        vec![
            Project::new("keel", "Keel").into(),
            Milestone::new(p.clone(), "Phase 0").into(),
            Task::new(p.clone(), "Wire up the schema").into(),
            Spec::new(p.clone(), "Storage spec").into(),
            Decision::new(p.clone(), "Use DuckDB").into(),
            Question::new(p.clone(), "Where does the store live?").into(),
            Term::new(Some(p.clone()), "Digest", "The keel_context summary").into(),
            Feedback::new(p.clone(), "Onboarding felt slow").into(),
            Design::new(p.clone(), "Home screen").into(),
            Environment::new(p.clone(), "production").into(),
            MetricObservation::new(metric.id.clone(), p.clone(), 0.42, Utc::now()).into(),
            metric.into(),
            Artifact::new(p, "Competitor teardown").into(),
        ]
    }

    #[test]
    fn every_type_is_constructible_and_self_describing() {
        let all = one_of_each();
        assert_eq!(all.len(), 13, "one of each of the thirteen");

        let mut seen = std::collections::HashSet::new();
        for e in &all {
            assert!(
                seen.insert(e.entity_type()),
                "duplicate type {}",
                e.entity_type()
            );
            assert_eq!(e.id().entity_type(), e.entity_type());
            assert!(
                !e.label().is_empty(),
                "{} has an empty label",
                e.entity_type()
            );
            assert!(!e.idempotency_key().is_empty());
        }
        assert_eq!(seen.len(), EntityType::ALL.len());
    }

    #[test]
    fn doc_version_is_present_exactly_for_prose_types() {
        for e in one_of_each() {
            assert_eq!(
                e.current_doc_version().is_some(),
                e.entity_type().has_document(),
                "{} disagrees with EntityType::has_document",
                e.entity_type()
            );
        }
    }

    #[test]
    fn setting_a_doc_version_on_a_task_is_an_error_not_a_no_op() {
        let mut task: Entity = Task::new(project(), "t").into();
        let err = task.set_current_doc_version(3).unwrap_err();
        assert!(err.to_string().contains("no prose body"), "{err}");

        let mut spec: Entity = Spec::new(project(), "s").into();
        spec.set_current_doc_version(3).unwrap();
        assert_eq!(spec.current_doc_version(), Some(3));
    }

    #[test]
    fn project_scope_distinguishes_the_two_kinds_of_none() {
        let p: Entity = Project::new("k", "K").into();
        assert_eq!(p.project_id(), None);
        assert_eq!(
            p.entity_type().project_scope(),
            crate::ProjectScope::IsTheProject
        );

        let global: Entity = Term::new(None, "Digest", "…").into();
        assert_eq!(global.project_id(), None);
        assert_eq!(
            global.entity_type().project_scope(),
            crate::ProjectScope::Optional
        );

        let scoped: Entity = Term::new(Some(project()), "Digest", "…").into();
        assert!(scoped.project_id().is_some());
    }

    #[test]
    fn idempotency_keys_ignore_case_and_whitespace() {
        let p = project();
        let a = Task::new(p.clone(), "Add login page");
        let b = Task::new(p.clone(), "  add   LOGIN   page ");
        assert_eq!(
            a.idempotency_key, b.idempotency_key,
            "trivially different titles must collapse to one task (R-6)"
        );

        let c = Task::new(p, "Add logout page");
        assert_ne!(a.idempotency_key, c.idempotency_key);
    }

    #[test]
    fn idempotency_keys_are_scoped_by_project_and_type() {
        let p1 = project();
        let p2 = project();
        assert_ne!(
            Task::new(p1.clone(), "Ship it").idempotency_key,
            Task::new(p2, "Ship it").idempotency_key,
            "the same title in two projects is two tasks"
        );
        assert_ne!(
            Task::new(p1.clone(), "Ship it").idempotency_key,
            Milestone::new(p1, "Ship it").idempotency_key,
            "a task and a milestone with one name are two things"
        );
    }

    #[test]
    fn global_and_scoped_terms_of_the_same_name_are_distinct() {
        // Q-4: a per-project term overrides a global one, so they must be able
        // to coexist rather than collide on the idempotency key.
        let global = Term::new(None, "Digest", "generic");
        let scoped = Term::new(Some(project()), "Digest", "specific");
        assert_ne!(global.idempotency_key, scoped.idempotency_key);
    }

    #[test]
    fn observations_of_one_metric_at_different_times_are_distinct() {
        let p = project();
        let m = EntityId::generate(EntityType::Metric);
        let t1 = DateTime::from_timestamp(1_000_000, 0).unwrap();
        let t2 = DateTime::from_timestamp(1_000_060, 0).unwrap();
        let a = MetricObservation::new(m.clone(), p.clone(), 1.0, t1);
        let b = MetricObservation::new(m.clone(), p.clone(), 2.0, t2);
        let c = MetricObservation::new(m, p, 9.9, t1);
        assert_ne!(a.idempotency_key, b.idempotency_key);
        assert_eq!(
            a.idempotency_key, c.idempotency_key,
            "the same metric at the same instant is one reading, whatever the value"
        );
    }

    #[test]
    fn status_is_absent_for_the_types_that_have_no_lifecycle() {
        for e in one_of_each() {
            let expected = !matches!(
                e.entity_type(),
                EntityType::Term
                    | EntityType::Feedback
                    | EntityType::Metric
                    | EntityType::MetricObservation
                    | EntityType::Artifact
            );
            assert_eq!(e.status().is_some(), expected, "{}", e.entity_type());
        }
    }

    #[test]
    fn entity_serialisation_is_tagged_by_type() {
        let e: Entity = Task::new(project(), "Ship it").into();
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "task");
        assert_eq!(json["title"], "Ship it");
        let back: Entity = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }
}
