//! The thirteen artifact structs, and the [`Entity`] enum that unifies them.
//!
//! Each struct mirrors its SQLite table from SPEC §3.2 field for field. Where
//! the schema and this file disagree, the schema wins and this file is the
//! bug — `keel-cli fsck` exists partly to catch that drift.
//!
//! Optional fields are `Option`, list columns are `Vec`, and every struct ends
//! with an [`Audit`] block. Nothing here validates itself on construction:
//! validation happens on the way into storage, in one place, so that the same
//! rules apply whether a value arrived from MCP, the CLI or a fixture.

use crate::{
    ArtifactKind, Audit, BlobId, CloseReason, DecisionStatus, DesignState, EntityId, EntityType,
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

/// How alike two labels are, as a token-set overlap between 0 and 1.
///
/// Exists because the idempotency key is a *hash* of the normalised title, so
/// it treats "Validate constituent phases to 0–360 degrees" and "Validate
/// constituent phases to 0–360" as unrelated. Two gate runs produced exactly
/// that pair. With one store per session nothing noticed; in a shared store
/// they are two rows for one task — UC-8's failure arriving one level below
/// projects, where nobody was watching for it.
///
/// Jaccard over normalised tokens rather than an edit distance: word order
/// varies freely between two descriptions of the same work, and character
/// distance punishes a longer, better title.
///
/// Overlap alone is not enough to merge on — see [`same_thing`].
pub fn title_similarity(a: &str, b: &str) -> f64 {
    let tokens = |s: &str| -> std::collections::BTreeSet<String> {
        normalise_for_key(s)
            .split_whitespace()
            .filter(|t| t.chars().any(char::is_alphanumeric))
            .map(str::to_owned)
            .collect()
    };
    let (a, b) = (tokens(a), tokens(b));
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let shared = a.intersection(&b).count() as f64;
    let total = a.union(&b).count() as f64;
    shared / total
}

/// Minimum overlap before two titles can be considered the same thing.
///
/// A false merge hides work that was genuinely new, which is worse than a
/// duplicate: a duplicate is visible and mergeable, a hidden row is neither.
pub const SAME_THING_THRESHOLD: f64 = 0.8;

/// Whether two titles name the same thing.
///
/// **Overlap plus containment**, and the containment half is what makes this
/// safe. One token set must be a subset of the other — the difference can only
/// be *added* words, never *substituted* ones.
///
/// The distinction is the whole rule. "Validate phases to 0–360" against
/// "Validate phases to 0–360 degrees" is an addition: the same thing said with
/// one more word. "Open question number 4" against "Open question number 5" is
/// a substitution, and the substituted token is the only thing distinguishing
/// them.
///
/// Overlap alone got this wrong, and a test caught it: sixty questions
/// differing by a single digit scored 0.875 — fourteen shared tokens out of
/// sixteen — and collapsed into two rows. High similarity says "these are
/// mostly the same words"; only containment says "one of these adds nothing
/// the other lacks".
pub fn same_thing(a: &str, b: &str) -> bool {
    let tokens = |s: &str| -> std::collections::BTreeSet<String> {
        normalise_for_key(s)
            .split_whitespace()
            .filter(|t| t.chars().any(char::is_alphanumeric))
            .map(str::to_owned)
            .collect()
    };
    let (ta, tb) = (tokens(a), tokens(b));
    if ta.is_empty() || tb.is_empty() {
        return false;
    }
    // Containment: no substituted tokens in either direction.
    if !(ta.is_subset(&tb) || tb.is_subset(&ta)) {
        return false;
    }
    // And enough overlap that a one-word title does not swallow everything it
    // is a prefix of — "Fix" is a subset of "Fix the login page" and is not
    // the same task.
    title_similarity(a, b) >= SAME_THING_THRESHOLD
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

/// How deep a chain of parent tasks may go.
///
/// Six, because work nested deeper than that is a milestone wearing a task's
/// clothes — and because a bound is what stops a malformed chain becoming an
/// unbounded walk while something is trying to render it.
pub const MAX_PARENT_DEPTH: usize = 6;

/// The longest a project key may be.
///
/// Four, because the point of `KEEL-42` is that it fits in a sentence and in a
/// column heading, and because four is where a truncation still reads as an
/// abbreviation rather than as a word cut in half — `harbour` gives `HARB`,
/// not `HARBO`. A key long enough to be descriptive has stopped being a key.
///
/// The migration's `substr` must agree with this. They are two expressions of
/// one rule, and the only thing keeping them in step is a test.
pub const MAX_PROJECT_KEY: usize = 4;

/// A project key derived from a slug: `keel` → `KEEL`, `harbour` → `HARB`.
///
/// Letters and digits only, uppercased, truncated. A slug with nothing usable
/// in it falls back to `P` rather than to the empty string — an empty key would
/// make every task in that project read as `-42`, and would collide with the
/// next such project rather than being visibly odd.
///
/// Uniqueness is *not* handled here, because it cannot be: it needs to know
/// about the other projects. [`crate::EntityStore::create`] resolves collisions
/// on the way in.
pub fn derive_project_key(slug: &str) -> String {
    let letters: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(MAX_PROJECT_KEY)
        .collect::<String>()
        .to_uppercase();
    if letters.is_empty() {
        "P".to_owned()
    } else {
        letters
    }
}

/// Split a readable identifier into its parts: `KEEL-42` → `("KEEL", 42)`.
///
/// Returns `None` for anything that is not one — a ULID, a sentence, a key with
/// no number. Deliberately strict about the number: `KEEL-42x` is not a
/// reference, and treating it as `KEEL-42` would silently resolve a typo to a
/// real task.
pub fn parse_readable_ref(reference: &str) -> Option<(String, i32)> {
    let (key, number) = reference.trim().rsplit_once('-')?;
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let number: i32 = number.parse().ok()?;
    if number <= 0 {
        return None;
    }
    Some((key.to_uppercase(), number))
}

/// Split a decision reference into its parts: `KEEL-B12` → `("KEEL", 12)`.
///
/// A separate function rather than a flag on [`parse_readable_ref`] because the
/// two namespaces are numbered independently: `KEEL-12` and `KEEL-B12` are both
/// valid and name different artifacts. Folding them into one parser would make
/// the `B` optional, and an optional discriminator between two live namespaces
/// resolves typos to real rows.
///
/// Bare `B-12`, as written in prose, is deliberately *not* accepted here: it
/// carries no project, and `B` would otherwise parse as a project key. Callers
/// that already know the project — `fsck` scanning one project's documents —
/// look the number up directly.
pub fn parse_decision_ref(reference: &str) -> Option<(String, i32)> {
    let (key, number) = reference.trim().rsplit_once("-B")?;
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    let number: i32 = number.parse().ok()?;
    if number <= 0 {
        return None;
    }
    Some((key.to_uppercase(), number))
}

/// The root container. Everything belongs to exactly one, except global terms.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    /// `prj_…`
    pub id: EntityId,
    /// URL-safe short name, unique across the store.
    pub slug: String,
    /// The short prefix in a task's readable identifier — the `KEEL` of
    /// `KEEL-42`. Unique across projects, because otherwise the readable
    /// identifier is not an identifier. Derived from the slug on creation and
    /// settable afterwards; nothing stores the composed string, so re-keying a
    /// project does not rewrite anything.
    pub key: String,
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
    /// Where the generated decision log goes, relative to `root_path`.
    ///
    /// Same reasoning as [`Project::status_path`]: the decision log is rendered
    /// from the decision rows rather than being one artifact's prose, so no
    /// document can adopt the path and the destination has to belong to the
    /// project. `None` means this project does not want one, and its decisions
    /// appear only as one file each under `.keel/decisions/`.
    pub decisions_path: Option<String>,
    /// Other names this project goes by. The main defence against UC-8's
    /// nine-near-duplicate-projects failure.
    pub aliases: Vec<String>,
    /// What this project calls a milestone.
    ///
    /// This one says "Phase" on every screen, and until this column existed the
    /// interface said "milestone" anyway — so the vocabulary was Keel's rather
    /// than the project's, and a session learned the project's word one rejected
    /// `keel_create` at a time.
    ///
    /// A display noun and nothing more. It never changes what is stored, which is
    /// the rule that keeps it from becoming a fourteenth type: `EntityType` still
    /// has thirteen values and a milestone is still a milestone. `None` means the
    /// project has no opinion, and the interface says "milestone".
    pub milestone_noun: Option<String>,
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
            key: derive_project_key(&slug),
            slug,
            name,
            description: None,
            status: ProjectStatus::default(),
            repo_urls: Vec::new(),
            root_path: None,
            status_path: None,
            decisions_path: None,
            aliases: Vec::new(),
            milestone_noun: None,
            audit: provisional_audit(),
        }
    }

    /// What to call a milestone when talking to this project's reader.
    ///
    /// Capitalised as the project wrote it, so "Phase 8" reads the way KB says it
    /// out loud. Falls back to Keel's own word rather than to nothing — an
    /// interface with a blank where a noun should be is worse than one using the
    /// generic term.
    pub fn milestone_word(&self) -> &str {
        match self.milestone_noun.as_deref().map(str::trim) {
            Some(word) if !word.is_empty() => word,
            _ => "Milestone",
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

/// The longest a milestone summary may be.
///
/// Generous enough for the two sentences the house style allows, short enough
/// to refuse a paragraph. The eight that already exist run 8 to 15 words, so
/// this is roughly double the observed need rather than a target to fill.
pub const MILESTONE_SUMMARY_MAX: usize = 280;

impl Milestone {
    /// A new milestone with the required fields.
    ///
    /// `summary` is an argument rather than a field set afterwards, and that is
    /// the whole point of the signature. It used to be settable later and
    /// nothing on the create path set it: `keel_create` accepted a `body` for a
    /// milestone and dropped it on the floor, so every milestone written
    /// through the tool surface reached the roadmap as a bare name. Making it
    /// positional means the compiler finds any call site that forgets, which is
    /// the check that cannot itself be forgotten.
    ///
    /// The value is not validated here — see [`Milestone::validate`], which the
    /// store calls on the way in so the CLI and MCP cannot disagree about what
    /// is acceptable.
    pub fn new(project_id: EntityId, name: impl Into<String>, summary: impl Into<String>) -> Self {
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
            summary: Some(summary.into()),
            status: MilestoneStatus::default(),
            target_date: None,
            shipped_at: None,
            version_string: None,
            sort_order: None,
            audit: provisional_audit(),
        }
    }

    /// Refuse a milestone whose explainer is missing, empty or a paragraph.
    ///
    /// The roadmap is the one screen answering "what is this project doing, and
    /// in what order", and a phase whose row is a bare name answers that only
    /// for whoever wrote it. Eight milestones against a hundred tasks means the
    /// milestone is the unit a human actually reads, so an unreadable one costs
    /// more per row than an unreadable task does.
    ///
    /// What this can check is structure, not quality — it cannot tell a good
    /// sentence from a bad one and never will. The register is carried by the
    /// tool description, which a model reads at the moment of writing; this
    /// catches the two failures that are objectively detectable.
    pub fn validate(&self) -> Result<()> {
        let summary = self.summary.as_deref().unwrap_or("").trim();

        if summary.is_empty() {
            return Err(Error::invalid(
                EntityType::Milestone,
                "summary",
                "a milestone needs a plain-English explainer and this one is empty",
                "one or two sentences saying what this phase covers, in the words a \
                 reader who has not seen the code would use — for example \
                 \"Make the everyday loop work: file a bug in seconds, see what's \
                 ready to start, and read the board without opening every card.\"",
            ));
        }

        if summary.chars().count() > MILESTONE_SUMMARY_MAX {
            return Err(Error::invalid(
                EntityType::Milestone,
                "summary",
                format!(
                    "the explainer is {} characters, and a milestone summary is capped at {}",
                    summary.chars().count(),
                    MILESTONE_SUMMARY_MAX
                ),
                "one or two sentences, not a paragraph. The detail belongs in a spec \
                 linked to this milestone; the summary is what someone reads on the \
                 roadmap without opening anything",
            ));
        }

        Ok(())
    }
}

impl Task {
    /// Refuse a task whose summary is missing, empty, or a restatement of its
    /// own title.
    ///
    /// **Two checks and only two**, which is KB's call in TQ-34. The mechanism
    /// is the required property on the tool, not this function: a model cannot
    /// complete the call without confronting it, on every surface, whether or
    /// not any skill loaded. This is the backstop.
    ///
    /// Checking harder was the other option and it was declined for a reason
    /// worth keeping: refused for something it does not agree with, a model
    /// satisfies the letter of the rule — swaps the word, keeps the same weak
    /// sentence — so the prose ends up both bad and compliant while the check
    /// reports success. A false rejection is worse than a mediocre summary.
    ///
    /// Existing rows are exempt by construction: this runs on the create path,
    /// and the ninety-four that predate the rule are reported by `keel lint`
    /// rather than rewritten. A machine inventing a summary would produce
    /// exactly the confident, plausible, wrong prose the rule exists to stop.
    pub fn validate_summary(&self) -> Result<()> {
        let summary = self.summary.as_deref().unwrap_or("").trim();

        if summary.is_empty() {
            return Err(Error::invalid(
                EntityType::Task,
                "summary",
                "a task needs one or two plain sentences and this one is empty",
                "what is wrong or wanted, what it affects, and what done looks like — \
                 readable cold by someone who was not in this conversation. For example: \
                 \"The board shows a task's priority but never which phase it belongs to, \
                 so you have to open each one to find out. Done when every row shows its \
                 milestone.\"",
            ));
        }

        if restates(&self.title, summary) {
            return Err(Error::invalid(
                EntityType::Task,
                "summary",
                "this summary only reorders the title — it adds nothing a reader does not \
                 already have",
                "say what the title cannot: why it matters, what it affects, or what done \
                 looks like",
            ));
        }

        Ok(())
    }

    /// Whether a claim on this task is still standing.
    ///
    /// A claim goes stale after [`CLAIM_STALE_AFTER`], which is the same three
    /// days `fsck` already warns on for a task sitting in `in_progress`. Reusing
    /// that number rather than choosing a second one is deliberate: two
    /// thresholds for "this session is probably gone" would eventually disagree,
    /// and the disagreement would show as work that `fsck` calls abandoned and
    /// `keel claim` refuses to take.
    pub fn claim_is_live(&self, now: DateTime<Utc>) -> bool {
        match (&self.claimed_by, self.claimed_at) {
            (Some(_), Some(at)) => now.signed_duration_since(at) < CLAIM_STALE_AFTER,
            // A session with no timestamp is a claim from before this column
            // existed. Treated as live so a claim is never silently ignored;
            // `--force` is the way past it.
            (Some(_), None) => true,
            _ => false,
        }
    }

    /// Check that a close carries what its reason demands.
    ///
    /// Called by the store on every path into a terminal status, which is what
    /// makes this an invariant rather than a convention. The definition of done
    /// in `product/CLAUDE.md` is a seven-item checklist an agent is *asked* to
    /// honour; a message and a piece of evidence being arguments of the
    /// transition is a rule that cannot be forgotten.
    pub fn validate_close(&self) -> Result<()> {
        let Some(reason) = self.close_reason else {
            return Err(Error::invalid(
                EntityType::Task,
                "close_reason",
                "a task cannot become done or wont_do without saying why",
                format!(
                    "one of {} — use keel_close, which asks for the reason, the message and \
                     the evidence together",
                    CloseReason::options()
                ),
            ));
        };

        if self
            .close_message
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
        {
            return Err(Error::invalid(
                EntityType::Task,
                "close_message",
                format!("closing as `{reason}` needs a message and this one is empty"),
                "one or two sentences on what actually happened — what was built, or why it \
                 is not being done. It is what the next session reads instead of guessing \
                 from a status.",
            ));
        }

        if reason.needs_evidence() && self.evidence.is_empty() {
            return Err(Error::invalid(
                EntityType::Task,
                "evidence",
                "a task cannot be done with nothing to show for it",
                format!("at least one of {EVIDENCE_KINDS_HELP}"),
            ));
        }

        for item in &self.evidence {
            validate_evidence(item)?;
        }

        Ok(())
    }
}

/// How long a claim stands before another session may take it.
///
/// Three days, matching `fsck`'s existing warning for work parked in
/// `in_progress`. See [`Task::claim_is_live`] for why it is not a second number.
pub const CLAIM_STALE_AFTER: chrono::TimeDelta = chrono::TimeDelta::days(3);

/// The evidence prefixes, in the order they are worth reaching for.
pub const EVIDENCE_KINDS: &[&str] = &["commit", "pr", "test", "doc", "url", "image"];

/// The evidence prefixes as an error message says them.
const EVIDENCE_KINDS_HELP: &str = "commit:<sha>, pr:<url>, test:<command>, doc:<entity-id>, \
                                   url:<url>, image:<blob-id>";

/// Check one piece of evidence.
///
/// Typed rather than free text, and checked rather than merely documented. The
/// point of the prefix is that "what shipped this week, with the commits" is a
/// query; a bare sha in the list makes that query wrong in a way nothing
/// reports.
pub fn validate_evidence(item: &str) -> Result<()> {
    let Some((kind, value)) = item.split_once(':') else {
        return Err(Error::invalid(
            EntityType::Task,
            "evidence",
            format!("`{item}` does not say what kind of evidence it is"),
            format!("one of {EVIDENCE_KINDS_HELP}"),
        ));
    };
    if !EVIDENCE_KINDS.contains(&kind) {
        return Err(Error::invalid(
            EntityType::Task,
            "evidence",
            format!("`{kind}` is not a kind of evidence Keel records"),
            format!("one of {EVIDENCE_KINDS_HELP}"),
        ));
    }
    if value.trim().is_empty() {
        return Err(Error::invalid(
            EntityType::Task,
            "evidence",
            format!("`{item}` names a kind of evidence but no evidence"),
            format!("a value after the colon, as in {EVIDENCE_KINDS_HELP}"),
        ));
    }
    Ok(())
}

/// Whether `summary` says nothing `title` did not already say.
///
/// Containment rather than similarity, reusing the rule KEEL-65 arrived at for
/// near-duplicate titles. One token set must be a subset of the other, so the
/// difference can only be *added* words — never substituted ones. That is what
/// makes it safe to refuse on: "Fix the board filter so it survives a reload"
/// against the title "Fix the board filter" is an addition and passes, where an
/// overlap score would have called it a duplicate.
fn restates(title: &str, summary: &str) -> bool {
    let words = |s: &str| -> Vec<String> {
        s.to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() > 2)
            .map(str::to_owned)
            .collect()
    };
    let title_words = words(title);
    let summary_words = words(summary);

    if title_words.is_empty() || summary_words.is_empty() {
        return false;
    }
    summary_words.iter().all(|w| title_words.contains(w))
}

/// A unit of work.
///
/// `PartialEq` but not `Eq`: `rank` is a float, and a total equality on a type
/// carrying one would be a lie about NaN rather than a convenience.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Task {
    /// `tsk_…`
    pub id: EntityId,
    /// Owning project.
    pub project_id: EntityId,
    /// The milestone this serves, if any.
    pub milestone_id: Option<EntityId>,
    /// Task, bug, chore or spike.
    pub kind: TaskKind,
    /// The number in this task's readable identifier — the `42` of `KEEL-42`.
    /// Unique within the project, assigned on creation in creation order, and
    /// never reused. Zero means "not yet assigned", which is only ever true
    /// between constructing a `Task` and storing it.
    pub number: i32,
    /// One line naming the work.
    pub title: String,
    /// Short detail. Anything long-form belongs in a spec, linked with
    /// `implements`.
    pub body: Option<String>,
    /// One or two plain sentences a colleague could read cold six weeks later,
    /// without having been in the conversation.
    ///
    /// Required on the create path and nullable in storage, which is not a
    /// compromise: ninety-four rows predate the rule, and a NOT NULL column
    /// would make them unreadable rather than merely unlabelled. The
    /// requirement belongs where it can be met.
    ///
    /// Lists render this and never fall back to `body`. A silent fallback is
    /// how a requirement quietly stops being one — the gap has to show.
    pub summary: Option<String>,
    /// Where it stands.
    pub status: TaskStatus,
    /// How urgent.
    pub priority: TaskPriority,
    /// Free-text labels.
    pub labels: Vec<String>,
    /// PR and issue URLs. More than one, because a task routinely spans a pull
    /// request and the issue it closes (TQ-23, KB confirmed 2026-08-10).
    pub external_refs: Vec<String>,
    /// The task this one is part of, if any.
    ///
    /// A column rather than an edge: `blocks` means "must happen first", and
    /// composition is a different relation. Modelling "is part of" as a
    /// blocking edge is what made rollups impossible, and it would corrupt the
    /// ranking, which reads every inbound `blocks` as something in the way.
    pub parent_id: Option<EntityId>,
    /// Where this sits in a deliberate order. Lower is earlier.
    ///
    /// Fractional so that inserting between two neighbours is their midpoint
    /// and touches one row. Set through `rank_after`/`rank_before` rather than
    /// by choosing a number — see [`crate::EntityStore::rank_between`].
    pub rank: f64,
    /// When it reached a terminal status.
    pub closed_at: Option<DateTime<Utc>>,
    /// The session that claimed this task, if it is claimed.
    ///
    /// A claim is what makes "in progress" mean something. Before it existed,
    /// moving a task to `in_progress` was an instruction in the contract that
    /// sessions had to remember, and across sixty-six tasks the number of
    /// transitions into that state before work began was zero.
    ///
    /// Cleared on close, so a finished task is not reported as being worked on.
    pub claimed_by: Option<String>,
    /// When the claim was taken.
    ///
    /// Carried alongside the session rather than derived from the event log,
    /// because staleness is the whole reason the field exists: a claim from a
    /// session that died three days ago must not hold work hostage, and asking
    /// the event log that question means a scan per task.
    pub claimed_at: Option<DateTime<Utc>>,
    /// Why it stopped being open.
    ///
    /// Set only by the close path, which is what makes it trustworthy. `done`
    /// and `wont_do` are the two terminal statuses; this says which of the five
    /// reasons put the task in one of them.
    pub close_reason: Option<CloseReason>,
    /// What the person or session closing it said about why.
    ///
    /// A column rather than a note, and the reason is enforcement: the storage
    /// layer refuses a close with no message, and it cannot check that a note
    /// was written in some other call. Notes remain the place for everything
    /// learned along the way; this is the one sentence that belongs to the
    /// transition itself.
    pub close_message: Option<String>,
    /// Typed, repeatable proof that the work happened.
    ///
    /// `commit:<sha>`, `pr:<url>`, `test:<command>`, `doc:<entity-id>`,
    /// `url:<url>` or `image:<blob-id>`. Typed rather than free text so that
    /// "what shipped this week, with the commits" is answerable by a query
    /// instead of by reading prose.
    pub evidence: Vec<String>,
    /// Idempotency key, unique within the project.
    pub idempotency_key: String,
    /// The audit block.
    pub audit: Audit,
}

impl Task {
    /// A new task with the required fields.
    ///
    /// `summary` is positional for the same reason `Milestone::new`'s is: the
    /// compiler finds any call site that forgets, which is the check that
    /// cannot itself be forgotten. It was a settable field first, and the cost
    /// showed immediately — the create path refused, and the missing summaries
    /// surfaced one failing test at a time across thirteen files instead of all
    /// at once in a build error.
    ///
    /// Not validated here — see [`Task::validate_summary`], which the store
    /// calls on the way in so the CLI and MCP cannot disagree about what is
    /// acceptable.
    pub fn new(project_id: EntityId, title: impl Into<String>, summary: impl Into<String>) -> Self {
        let title = title.into();
        Task {
            id: EntityId::generate(EntityType::Task),
            idempotency_key: derive_idempotency_key(Some(&project_id), EntityType::Task, &title),
            project_id,
            number: 0,
            milestone_id: None,
            kind: TaskKind::default(),
            title,
            body: None,
            summary: Some(summary.into()),
            status: TaskStatus::default(),
            priority: TaskPriority::default(),
            labels: Vec::new(),
            external_refs: Vec::new(),
            parent_id: None,
            // Zero means "not yet assigned", which is only true between
            // constructing a `Task` and storing it.
            rank: 0.0,
            closed_at: None,
            claimed_by: None,
            claimed_at: None,
            close_reason: None,
            close_message: None,
            evidence: Vec::new(),
            audit: provisional_audit(),
        }
    }
}

/// A prose document's header. The body lives in the `documents` table.
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
    /// The number in this decision's readable identifier — the `12` of `B-12`.
    /// Unique within the project, assigned on creation, and never reused.
    ///
    /// Decisions carry one for a reason tasks do not: `B-12` was already being
    /// written into prose before the column existed, so the identifier was a
    /// convention with nothing behind it. `fsck` could not resolve a single
    /// `B-n` citation and had to skip the family, which is why it misses the
    /// fabricated cross-reference that motivated the check (KEEL-66).
    ///
    /// Zero means "not yet assigned", true only between constructing a
    /// `Decision` and storing it.
    pub number: i32,
    /// Display title.
    pub title: String,
    /// Proposed, accepted, superseded or rejected.
    ///
    /// `accepted` used to make the content un-editable. It no longer does: the
    /// guard refused a title change while the body — the actual reasoning —
    /// stayed writable through `write_revision`, so it prevented the harmless
    /// edit and permitted the harmful one. Every change is an attributed
    /// revision with a diff, which is the guard that was wanted (TQ-27, B-43).
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
            number: 0,
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
    /// The artifact type this word is a spelling of, if it is one.
    ///
    /// This is what lets the glossary drive type aliasing rather than a fixed
    /// list in the source. A project that says "incident" for a task, or
    /// "customer feedback" for feedback, defines the term once and every surface
    /// accepts the word.
    ///
    /// A column rather than something parsed out of `definition`, and the
    /// difference matters: "a phase is a milestone with a demo at the end" and "a
    /// phase is not a milestone" would resolve identically under any rule that
    /// read the prose. A declaration cannot be misread.
    ///
    /// **This must never create a fourteenth type.** Every value is one of the
    /// thirteen, which the type system enforces here — a term declares a
    /// *spelling*, never a concept.
    pub means: Option<EntityType>,
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
            means: None,
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
    /// The stored image in the `blobs` table.
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

    /// The string each constructor hashes into its derived idempotency key.
    ///
    /// Almost always [`Entity::label`], but a project keys on its *slug* — two
    /// projects may legitimately share a display name while the slug is what
    /// must be unique. Comparing a stored key against the key this would
    /// derive is how `create` tells a caller-supplied key from a derived one,
    /// and an explicit key is a claim that two same-titled things are
    /// different.
    pub fn natural_key(&self) -> &str {
        match self {
            Entity::Project(p) => &p.slug,
            other => other.label(),
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
            Milestone::new(p.clone(), "Phase 0", "The spine.").into(),
            Task::new(
                p.clone(),
                "Wire up the schema",
                "A row this test needs in the store.",
            )
            .into(),
            Spec::new(p.clone(), "Storage spec").into(),
            Decision::new(p.clone(), "Use SQLite").into(),
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
        let mut task: Entity =
            Task::new(project(), "t", "A row this test needs in the store.").into();
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
        let a = Task::new(
            p.clone(),
            "Add login page",
            "A row this test needs in the store.",
        );
        let b = Task::new(
            p.clone(),
            "  add   LOGIN   page ",
            "A row this test needs in the store.",
        );
        assert_eq!(
            a.idempotency_key, b.idempotency_key,
            "trivially different titles must collapse to one task (R-6)"
        );

        let c = Task::new(p, "Add logout page", "A row this test needs in the store.");
        assert_ne!(a.idempotency_key, c.idempotency_key);
    }

    #[test]
    fn idempotency_keys_are_scoped_by_project_and_type() {
        let p1 = project();
        let p2 = project();
        assert_ne!(
            Task::new(p1.clone(), "Ship it", "A row this test needs in the store.").idempotency_key,
            Task::new(p2, "Ship it", "A row this test needs in the store.").idempotency_key,
            "the same title in two projects is two tasks"
        );
        assert_ne!(
            Task::new(p1.clone(), "Ship it", "A row this test needs in the store.").idempotency_key,
            Milestone::new(p1, "Ship it", "Get it out of the door.").idempotency_key,
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
        let e: Entity =
            Task::new(project(), "Ship it", "A row this test needs in the store.").into();
        let json = serde_json::to_value(&e).unwrap();
        assert_eq!(json["type"], "task");
        assert_eq!(json["title"], "Ship it");
        let back: Entity = serde_json::from_value(json).unwrap();
        assert_eq!(back, e);
    }

    #[test]
    fn a_milestone_keeps_the_explainer_it_was_built_with() {
        let m = Milestone::new(project(), "Phase 8", "Make the everyday loop work.");
        assert_eq!(m.summary.as_deref(), Some("Make the everyday loop work."));
        assert!(m.validate().is_ok());
    }

    // Failure case, and the one that motivated all of this: a milestone whose
    // roadmap row would be a bare name.
    #[test]
    fn an_empty_explainer_is_refused_with_something_to_send_instead() {
        let m = Milestone::new(project(), "Phase 8", "   ");
        let err = m.validate().unwrap_err();
        let message = err.to_string();
        assert!(message.contains("summary"), "{message}");
        // The point of the three-part error: a model reading only the
        // "Expected" half must still be able to retry successfully.
        assert!(message.contains("one or two sentences"), "{message}");
    }

    #[test]
    fn a_null_explainer_is_refused_too() {
        // Reachable through a read of an older row rather than through `new`,
        // which is exactly why the check is on the value and not on the
        // constructor alone.
        let mut m = Milestone::new(project(), "Phase 8", "Something.");
        m.summary = None;
        assert!(m.validate().is_err());
    }

    // Failure case: the other direction. A paragraph is not a summary, and the
    // roadmap is a list of rows rather than a document.
    #[test]
    fn a_paragraph_is_refused_and_the_error_says_how_long_it_was() {
        let long = "a".repeat(MILESTONE_SUMMARY_MAX + 1);
        let m = Milestone::new(project(), "Phase 8", &long);
        let message = m.validate().unwrap_err().to_string();
        assert!(
            message.contains(&(MILESTONE_SUMMARY_MAX + 1).to_string()),
            "{message}"
        );
        assert!(message.contains("not a paragraph"), "{message}");
    }

    #[test]
    fn the_ceiling_counts_characters_rather_than_bytes() {
        // A summary in a language that is not mostly ASCII should get the same
        // allowance. Byte length would silently halve it.
        let m = Milestone::new(project(), "Phase 8", "é".repeat(MILESTONE_SUMMARY_MAX));
        assert!(m.validate().is_ok());
    }

    #[test]
    fn the_shipped_phases_all_satisfy_the_rule_they_predate() {
        // The eight that already exist were written before there was a rule.
        // If the ceiling were set below what the house style actually produces,
        // this is where that would show up.
        for summary in [
            "Storage, schema, event log, graph, search, backup. No network, no UI.",
            "axum, the nine MCP tools, keel_context, concurrency safety, render-status.",
            "Deployable daemon, auth, mobile client.",
            "Make the everyday loop work: file a bug in seconds, see what's ready to \
             start, and read the board without opening every card.",
        ] {
            let m = Milestone::new(project(), "Phase n", summary);
            assert!(m.validate().is_ok(), "rejected: {summary}");
        }
    }
}
