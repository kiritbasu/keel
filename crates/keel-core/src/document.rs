//! Document revisions — the prose half of Keel.
//!
//! Every prose body in the store, of every type, lives in one Lance dataset
//! (D-2). That is the highest-leverage decision in the spec: one hybrid search
//! covers spec sections, decisions, customer feedback and design captions
//! together, versioning has a single code path, and adding a prose-bearing
//! type costs nothing.
//!
//! Revisions are modelled in **user columns**, not Lance dataset versions
//! (D-2b). Dataset versions are a storage concern that serves snapshot and
//! restore; a document revision is a domain concept that has to survive
//! compaction and re-embedding. Conflating them would mean losing revision
//! history to a maintenance operation.

use crate::{Actor, DocId, EntityId, EntityType, Error, Result, Surface};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// The dimensionality of the embedding vector.
///
/// Fixed by the model choice (`bge-small-en-v1.5`, D-7). Changing models means
/// changing this, which means rewriting the dataset — hence `embedding_model`
/// and `embedding_version` on every row, so the migration can be a background
/// pass over stale rows rather than a rewrite of everything.
pub const EMBEDDING_DIM: usize = 384;

/// The embedding model Keel uses.
pub const EMBEDDING_MODEL: &str = "bge-small-en-v1.5";

/// Bumped by hand to force a re-embed of every document.
pub const EMBEDDING_VERSION: i32 = 1;

/// Where a revision sits in its document's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DocStatus {
    /// Written but not yet the current revision.
    Draft,
    /// The revision the entity's `current_doc_version` points at. Exactly one
    /// per entity, which `fsck` checks.
    Current,
    /// A former current revision.
    Superseded,
    /// The whole document was archived along with its entity.
    Archived,
}

impl DocStatus {
    /// Every status.
    pub const ALL: [DocStatus; 4] = [
        DocStatus::Draft,
        DocStatus::Current,
        DocStatus::Superseded,
        DocStatus::Archived,
    ];

    /// The stored string.
    pub const fn as_str(self) -> &'static str {
        match self {
            DocStatus::Draft => "draft",
            DocStatus::Current => "current",
            DocStatus::Superseded => "superseded",
            DocStatus::Archived => "archived",
        }
    }

    /// Parse a stored string.
    pub fn parse(s: &str) -> Result<Self> {
        DocStatus::ALL
            .into_iter()
            .find(|d| d.as_str() == s)
            .ok_or_else(|| Error::MalformedId {
                supplied: s.to_owned(),
                problem: format!("`{s}` is not a document status"),
                expected: "draft | current | superseded | archived".to_owned(),
            })
    }
}

impl fmt::Display for DocStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Content-address a body, so identical revisions can be recognised.
///
/// Used to short-circuit a `keel_write_doc` that would append a revision
/// byte-identical to the current one. That happens more than it sounds: the
/// mirror hook in SPEC §8.1 regenerates a file and re-reads it, and without
/// this every no-op save would grow the history.
pub fn body_hash(title: &str, body: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    hasher.update(b"\x1f");
    hasher.update(body.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// One immutable revision of a prose document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    /// `doc_…`, this revision's own identifier.
    pub doc_id: DocId,
    /// Which of the five prose-bearing types this belongs to.
    pub entity_type: EntityType,
    /// The DuckDB header row this is the body of.
    ///
    /// A logical reference, **not** an enforced foreign key: Lance cannot
    /// enforce it and DuckDB cannot see it. `keel-core` validates it on write
    /// and `keel-cli fsck` audits it. This is the single most important
    /// cross-engine invariant in the system.
    pub entity_id: EntityId,
    /// Denormalised from the header, so search can filter by project without
    /// leaving the dataset.
    pub project_id: Option<EntityId>,
    /// 1-based revision number, unique per `entity_id`.
    pub version: i32,
    /// The revision this one succeeds. `None` for the first.
    pub parent_version: Option<i32>,
    /// The title as of this revision.
    pub title: String,
    /// The markdown body.
    pub body: String,
    /// Content address of `(title, body)`.
    pub body_hash: String,
    /// Pointer into the `blobs` dataset, for design captions with an image.
    pub media_ref: Option<String>,
    /// Where this revision sits in the history.
    pub status: DocStatus,
    /// Who wrote it.
    pub author: Actor,
    /// The conversation that wrote it, if supplied.
    pub session_id: Option<String>,
    /// The surface it came from.
    pub surface: Option<Surface>,
    /// When it was written.
    pub created_at: DateTime<Utc>,
    /// The semantic vector. `None` when embedding has not run yet — a document
    /// is still readable and keyword-searchable without one, so a failed
    /// embed must not block the write.
    pub embedding: Option<Vec<f32>>,
    /// Which model produced `embedding`.
    pub embedding_model: String,
    /// Bumped to trigger a re-embed.
    pub embedding_version: i32,
}

impl Document {
    /// The first revision of a document.
    pub fn first(
        entity_type: EntityType,
        entity_id: EntityId,
        project_id: Option<EntityId>,
        title: impl Into<String>,
        body: impl Into<String>,
        author: Actor,
        now: DateTime<Utc>,
    ) -> Result<Self> {
        Self::revision(
            entity_type,
            entity_id,
            project_id,
            title,
            body,
            author,
            now,
            None,
        )
    }

    /// A revision succeeding `parent_version`, or the first if `None`.
    #[allow(clippy::too_many_arguments)]
    pub fn revision(
        entity_type: EntityType,
        entity_id: EntityId,
        project_id: Option<EntityId>,
        title: impl Into<String>,
        body: impl Into<String>,
        author: Actor,
        now: DateTime<Utc>,
        parent_version: Option<i32>,
    ) -> Result<Self> {
        if !entity_type.has_document() {
            return Err(Error::Invariant {
                operation: format!("write a document revision for {entity_id}"),
                problem: format!(
                    "{entity_type} has no prose body; only spec, decision, question, \
                     feedback and design write to the documents dataset"
                ),
            });
        }
        if entity_id.entity_type() != entity_type {
            return Err(Error::Invariant {
                operation: format!("write a {entity_type} document revision for {entity_id}"),
                problem: format!(
                    "the id belongs to a {}, not a {entity_type}",
                    entity_id.entity_type()
                ),
            });
        }
        let title = title.into();
        let body = body.into();
        Ok(Document {
            doc_id: DocId::generate(),
            entity_type,
            entity_id,
            project_id,
            version: parent_version.unwrap_or(0) + 1,
            parent_version,
            body_hash: body_hash(&title, &body),
            title,
            body,
            media_ref: None,
            status: DocStatus::Current,
            author,
            session_id: None,
            surface: None,
            created_at: now,
            embedding: None,
            embedding_model: EMBEDDING_MODEL.to_owned(),
            embedding_version: EMBEDDING_VERSION,
        })
    }

    /// Attach provenance from the caller.
    pub fn attributed(mut self, session_id: Option<String>, surface: Option<Surface>) -> Self {
        self.session_id = session_id;
        self.surface = surface;
        self
    }

    /// Whether this revision's content is identical to another's.
    pub fn same_content_as(&self, other: &Document) -> bool {
        self.body_hash == other.body_hash
    }

    /// The text that gets embedded and keyword-indexed.
    ///
    /// Title and body together: a spec called "Rate limiting" whose body never
    /// repeats the phrase should still be found by searching for it.
    pub fn searchable_text(&self) -> String {
        format!("{}\n\n{}", self.title, self.body)
    }
}

/// A unified diff between two revisions of the same document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentDiff {
    /// The entity whose document this is.
    pub entity_id: EntityId,
    /// The older revision.
    pub from_version: i32,
    /// The newer revision.
    pub to_version: i32,
    /// Unified-diff text.
    pub unified: String,
    /// Lines added.
    pub added: usize,
    /// Lines removed.
    pub removed: usize,
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn spec_id() -> EntityId {
        EntityId::generate(EntityType::Spec)
    }

    #[test]
    fn a_first_revision_is_version_one_with_no_parent() {
        let d = Document::first(
            EntityType::Spec,
            spec_id(),
            None,
            "Storage",
            "# Storage\n\nDuckDB and Lance.",
            Actor::Claude,
            Utc::now(),
        )
        .unwrap();
        assert_eq!(d.version, 1);
        assert_eq!(d.parent_version, None);
        assert_eq!(d.status, DocStatus::Current);
        assert_eq!(d.embedding_model, EMBEDDING_MODEL);
        assert!(d.embedding.is_none(), "embedding is filled in separately");
    }

    #[test]
    fn revisions_increment_from_their_parent() {
        let id = spec_id();
        let v3 = Document::revision(
            EntityType::Spec,
            id,
            None,
            "Storage",
            "changed",
            Actor::Human,
            Utc::now(),
            Some(2),
        )
        .unwrap();
        assert_eq!(v3.version, 3);
        assert_eq!(v3.parent_version, Some(2));
    }

    #[test]
    fn types_without_prose_cannot_have_documents() {
        let err = Document::first(
            EntityType::Task,
            EntityId::generate(EntityType::Task),
            None,
            "t",
            "b",
            Actor::Claude,
            Utc::now(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("no prose body"), "{err}");
    }

    #[test]
    fn the_id_must_match_the_declared_type() {
        // Catches an entity_id/entity_type mismatch at the point of writing
        // rather than leaving an orphan for fsck to find much later.
        let err = Document::first(
            EntityType::Spec,
            EntityId::generate(EntityType::Decision),
            None,
            "t",
            "b",
            Actor::Claude,
            Utc::now(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("belongs to a decision"), "{err}");
    }

    #[test]
    fn identical_content_hashes_identically_and_different_content_does_not() {
        let h1 = body_hash("Storage", "DuckDB and Lance.");
        let h2 = body_hash("Storage", "DuckDB and Lance.");
        let h3 = body_hash("Storage", "DuckDB and Lance");
        let h4 = body_hash("Storage layer", "DuckDB and Lance.");
        assert_eq!(h1, h2);
        assert_ne!(h1, h3, "a trailing full stop is a real change");
        assert_ne!(h1, h4, "the title is part of the content");
    }

    #[test]
    fn searchable_text_includes_the_title() {
        let d = Document::first(
            EntityType::Decision,
            EntityId::generate(EntityType::Decision),
            None,
            "Rate limiting",
            "We will use a token bucket.",
            Actor::Human,
            Utc::now(),
        )
        .unwrap();
        assert!(d.searchable_text().contains("Rate limiting"));
        assert!(d.searchable_text().contains("token bucket"));
    }

    #[test]
    fn embedding_dimension_matches_the_model() {
        assert_eq!(EMBEDDING_DIM, 384, "bge-small-en-v1.5 is 384-dimensional");
    }
}
