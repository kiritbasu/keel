//! Document revisions — the prose half of Keel.
//!
//! Every prose body in the store, of every type, lives in one `documents` table
//! (D-2). That is the highest-leverage decision in the spec: one hybrid search
//! covers spec sections, decisions, customer feedback and design captions
//! together, versioning has a single code path, and adding a prose-bearing
//! type costs nothing.
//!
//! Revisions are **ordinary rows**, numbered by `version` and chained by
//! `parent_version` (D-2b). The decision was first taken against a store whose
//! engine versioned its own datasets and invited you to reuse that for
//! revisions; it survived the move to SQLite because the reasoning was never
//! about the engine. Whatever a storage engine versions for you exists to serve
//! snapshot and restore, and it is free to collapse those versions when it
//! compacts. A document revision is a domain concept — it has an author, a
//! session, a status and a diff taken against it — and it has to outlive both
//! compaction and a re-embedding pass. Conflating the two means losing revision
//! history to a maintenance operation.

use crate::{Actor, DocId, EntityId, EntityType, Error, Result, Surface};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// The dimensionality of the embedding vector.
///
/// Fixed by the model choice (`bge-small-en-v1.5`, D-7). Changing models means
/// changing this, which means every stored vector is the wrong width — hence
/// `embedding_model` and `embedding_version` on every row, so the migration can
/// be a background pass over stale rows rather than a rewrite of everything.
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

/// Content-address raw bytes, as lowercase hex.
///
/// Used for blobs, where it is what makes a re-upload of the same image
/// recognisable as the same image.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

/// Content-address a body, so identical revisions can be recognised.
///
/// Used to short-circuit a `keel_write_doc` that would append a revision
/// byte-identical to the current one. That happens more than it sounds: a
/// caller that regenerates a file and re-reads it would otherwise grow the
/// history on every no-op save.
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
    /// The header row this is the body of.
    ///
    /// A logical reference, **not** an enforced foreign key, even though the
    /// header now sits in the same file with `foreign_keys = ON`. There is
    /// nothing for a `REFERENCES` clause to name: `entity_id` is polymorphic,
    /// pointing at whichever of the thirteen entity tables `entity_type` says.
    /// So `keel-core` validates it on write and `keel-cli fsck` audits it. What
    /// one file did buy is that a document and its header are written in one
    /// transaction, so the pair can no longer be left half-written — the case
    /// left to audit is a reference to a row that never existed.
    pub entity_id: EntityId,
    /// Denormalised from the header, so search can filter by project without
    /// joining back to it.
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
    /// Pointer into the `blobs` table, for design captions with an image.
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
                     feedback and design write to the documents table"
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
        Document::searchable_text_of(&self.title, &self.body)
    }

    /// The same text, from the two columns rather than from a whole document.
    ///
    /// The re-embedding pass reads only `title` and `body` — reading every
    /// column of every revision to embed two of them is a lot of bytes for
    /// nothing — and it must produce the identical string the write path does,
    /// or a backfilled vector and a freshly written one are not comparable and
    /// results depend on when a document happened to be embedded.
    pub fn searchable_text_of(title: &str, body: &str) -> String {
        format!("{title}\n\n{body}")
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
            "# Storage\n\nOne SQLite file.",
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
        let h1 = body_hash("Storage", "One SQLite file.");
        let h2 = body_hash("Storage", "One SQLite file.");
        let h3 = body_hash("Storage", "One SQLite file");
        let h4 = body_hash("Storage layer", "One SQLite file.");
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
