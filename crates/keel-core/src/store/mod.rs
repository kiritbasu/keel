//! Storage: three traits and one DuckDB-plus-Lance implementation.
//!
//! The split is the one `product/CLAUDE.md` mandates:
//!
//! - [`EntityStore`] — DuckDB entity CRUD, links and events.
//! - [`DocumentStore`] — Lance revisions, blobs, embeddings and search.
//! - [`GraphStore`] — link traversal, and nothing else.
//!
//! No raw SQL exists outside these implementations. That is not tidiness: the
//! graph queries are wrong in a way that returns *plausible empty results*,
//! which is the worst failure mode available, so centralising them means
//! getting the direction right once instead of at every call site.

pub mod docs;
pub mod duck;
pub mod patch;
pub mod rows;
pub mod schema;
pub mod sqlite;

pub use duck::DuckStore;
pub use patch::{FieldChange, apply_changes};
pub use sqlite::{SqliteStore, store_path};

use crate::{
    Cursor, Direction, Document, DocumentDiff, Entity, EntityId, EntityType, Event, Link, NewEvent,
    NewLink, NewNote, Note, Provenance, Relation, Result,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The outcome of a create.
///
/// `created: false` means an entity with this idempotency key already existed
/// and is being returned unchanged. A retrying agent gets a sane result rather
/// than a duplicate or an error it has to reason about (SPEC §7.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Created {
    /// The entity, new or pre-existing.
    pub entity: Entity,
    /// Whether this call is what brought it into being.
    pub created: bool,
}

/// A page of results that always tells the truth about what it left out.
///
/// Hard constraint 4: every list that can be cut reports that it was cut, with
/// a total. An agent that receives 10 of 40 open questions with no indication
/// will confidently re-litigate settled decisions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page<T> {
    /// The results.
    pub items: Vec<T>,
    /// How many matched in total, before any limit.
    pub total: usize,
    /// Whether `items` is shorter than `total`.
    pub truncated: bool,
}

impl<T> Page<T> {
    /// A page that was cut from a known total.
    pub fn new(items: Vec<T>, total: usize) -> Self {
        Page {
            truncated: items.len() < total,
            items,
            total,
        }
    }

    /// A page containing everything that matched.
    pub fn complete(items: Vec<T>) -> Self {
        Page {
            total: items.len(),
            truncated: false,
            items,
        }
    }

    /// Map the items, preserving the truncation report.
    pub fn map<U>(self, f: impl FnMut(T) -> U) -> Page<U> {
        Page {
            items: self.items.into_iter().map(f).collect(),
            total: self.total,
            truncated: self.truncated,
        }
    }
}

/// Filters for listing entities.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EntityQuery {
    /// Restrict to one project. `None` means all projects.
    pub project_id: Option<EntityId>,
    /// Restrict to these types. Empty means all.
    pub entity_types: Vec<EntityType>,
    /// Restrict to these status values. Empty means all.
    pub statuses: Vec<String>,
    /// Include soft-deleted rows. Defaults to false — archived rows exist for
    /// recovery and audit, not for everyday lists.
    pub include_archived: bool,
    /// Created at or after this instant.
    pub since: Option<DateTime<Utc>>,
    /// Created strictly before this instant.
    pub until: Option<DateTime<Utc>>,
    /// Maximum rows to return. `None` means the store's default cap.
    pub limit: Option<usize>,
    /// Rows to skip.
    pub offset: usize,
}

impl EntityQuery {
    /// Everything in one project.
    pub fn in_project(project_id: EntityId) -> Self {
        EntityQuery {
            project_id: Some(project_id),
            ..Default::default()
        }
    }

    /// Restrict to one type.
    pub fn of_type(mut self, entity_type: EntityType) -> Self {
        self.entity_types = vec![entity_type];
        self
    }

    /// Restrict to a set of statuses.
    pub fn with_status(mut self, statuses: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.statuses = statuses.into_iter().map(Into::into).collect();
        self
    }

    /// Cap the result count.
    pub fn limited(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }
}

/// One node reached by a graph traversal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Neighbour {
    /// The entity reached.
    pub id: EntityId,
    /// Its type, denormalised on the edge so reaching it costs no join.
    pub entity_type: EntityType,
    /// What it is called, resolved through `v_entities`.
    ///
    /// Carried because a traversal result that is only an id cannot be
    /// rendered or reasoned about without a second round of lookups, and every
    /// caller was doing that round differently — the document reader showed
    /// bare ULIDs where a title belonged, and an agent walking the graph had to
    /// follow every hop with a `keel_get` to learn what it had found. Empty
    /// only if the edge points at a row that no longer resolves, which `fsck`
    /// reports as a dangling link.
    pub label: String,
    /// The relation on the edge that reached it.
    pub rel: Relation,
    /// The anchor on that edge, e.g. `REQ-4`. Empty means whole-entity.
    pub anchor: String,
    /// How many hops from the root. 1 is a direct neighbour.
    pub depth: u8,
    /// The full path from the root, inclusive of both ends. Carried so a
    /// caller can explain *why* something is reachable, which is most of the
    /// value of a traceability query.
    pub path: Vec<EntityId>,
}

/// A search request spanning both indexes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// The natural-language or keyword query.
    pub text: String,
    /// Restrict to one project.
    pub project_id: Option<EntityId>,
    /// Restrict to these types. Empty means every searchable type.
    pub entity_types: Vec<EntityType>,
    /// Created at or after.
    pub since: Option<DateTime<Utc>>,
    /// Created strictly before.
    pub until: Option<DateTime<Utc>>,
    /// How many results to return.
    pub limit: usize,
}

impl SearchQuery {
    /// A query with the default result count.
    pub fn new(text: impl Into<String>) -> Self {
        SearchQuery {
            text: text.into(),
            project_id: None,
            entity_types: Vec::new(),
            since: None,
            until: None,
            limit: 20,
        }
    }

    /// The inner retrieval depth.
    ///
    /// `k_inner = k_outer * 4` per SPEC §5. Retrieving exactly `k` from the
    /// index and *then* filtering by project and date is a classic way to
    /// return three results when forty exist.
    pub fn inner_limit(&self) -> usize {
        self.limit.saturating_mul(4).max(20)
    }
}

/// One search hit, from either index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// The entity found.
    pub entity_id: EntityId,
    /// Its type.
    pub entity_type: EntityType,
    /// The project it belongs to.
    pub project_id: Option<EntityId>,
    /// Its title or label.
    pub title: String,
    /// A short excerpt of the matching text.
    pub excerpt: String,
    /// The fused relevance score. Higher is better.
    pub score: f64,
    /// Which index produced this hit, kept so retrieval quality (R-3) can be
    /// evaluated per index rather than in aggregate — "is the semantic half
    /// earning its keep" is otherwise unanswerable.
    pub source: SearchSource,
}

/// Which index a hit came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSource {
    /// The DuckDB BM25 index, which covers every searchable artifact.
    Keyword,
    /// The Lance vector index, over prose embeddings.
    Semantic,
    /// Both found it. The strongest signal available here: an independent
    /// keyword match and an independent semantic match agreeing.
    Both,
}

/// A stored binary blob.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// `blb_…`
    pub blob_id: crate::BlobId,
    /// The entity it belongs to.
    pub entity_id: Option<EntityId>,
    /// The project it belongs to.
    pub project_id: Option<EntityId>,
    /// MIME type.
    pub media_type: String,
    /// The bytes.
    pub bytes: Vec<u8>,
    /// Content address.
    pub sha256: String,
    /// When it was stored.
    pub created_at: DateTime<Utc>,
}

impl Blob {
    /// A blob from raw bytes, content-addressed.
    ///
    /// `sha256` is computed here rather than accepted from the caller: it is
    /// what makes a re-upload of the same image detectable, and a hash the
    /// caller supplies is a hash nobody has checked.
    pub fn new(bytes: Vec<u8>, media_type: impl Into<String>, at: DateTime<Utc>) -> Self {
        let sha256 = crate::sha256_hex(&bytes);
        Blob {
            blob_id: crate::BlobId::generate(),
            entity_id: None,
            project_id: None,
            media_type: media_type.into(),
            bytes,
            sha256,
            created_at: at,
        }
    }

    /// Attach this blob to an entity and its project.
    pub fn owned_by(mut self, entity_id: EntityId, project_id: EntityId) -> Self {
        self.entity_id = Some(entity_id);
        self.project_id = Some(project_id);
        self
    }
}

/// DuckDB entity CRUD, links and events.
pub trait EntityStore {
    /// Create an entity, or return the existing one with the same idempotency
    /// key.
    ///
    /// Takes `&mut self` to express the single-writer rule at the type level
    /// (D-5), not because DuckDB requires it.
    fn create(&mut self, entity: Entity, provenance: &Provenance) -> Result<Created>;

    /// Fetch by id, archived or not. `None` means it never existed.
    fn get(&self, id: &EntityId) -> Result<Option<Entity>>;

    /// Apply a set of field changes under optimistic concurrency.
    ///
    /// `expected_version` is the `version` the caller read. A mismatch is
    /// [`crate::Error::StaleVersion`], which the daemon turns into SPEC §7.3's
    /// 409 with the current state attached.
    fn update(
        &mut self,
        id: &EntityId,
        expected_version: i32,
        changes: &serde_json::Map<String, serde_json::Value>,
        provenance: &Provenance,
    ) -> Result<Entity>;

    /// Soft-delete. Nothing is ever `DELETE`d (D-9).
    fn archive(
        &mut self,
        id: &EntityId,
        expected_version: i32,
        provenance: &Provenance,
    ) -> Result<Entity>;

    /// List entities matching a query.
    fn list(&self, query: &EntityQuery) -> Result<Page<Entity>>;

    /// Create an edge, normalising `depends_on` into `blocks` on the way in.
    fn link(&mut self, link: NewLink, provenance: &Provenance) -> Result<Link>;

    /// Archive an edge. Sets `archived_at`; it does not `DELETE`.
    fn unlink(
        &mut self,
        from_id: &EntityId,
        rel: Relation,
        to_id: &EntityId,
        anchor: &str,
        provenance: &Provenance,
    ) -> Result<Link>;

    /// Append to the mutation log.
    fn append_event(&mut self, event: NewEvent, provenance: &Provenance) -> Result<Event>;

    /// Read the mutation log from a cursor.
    fn events(
        &self,
        cursor: &Cursor,
        project_id: Option<&EntityId>,
        limit: usize,
    ) -> Result<Page<Event>>;

    /// Turn whatever a caller wrote into an id: a ULID, or `KEEL-42`.
    ///
    /// The point of a readable identifier is that a human can say it out loud
    /// and type it into a conversation, so every place that takes an id has to
    /// take one. `Ok(None)` means it names nothing — which is a legitimate
    /// answer to "does this exist", and distinct from a malformed reference.
    fn resolve_ref(&self, reference: &str) -> Result<Option<EntityId>>;

    /// The next unused task number in a project. Never reuses one.
    fn next_task_number(&self, project_id: &EntityId) -> Result<i32>;

    /// A rank that puts a new task at the end of the deliberate order.
    fn next_task_rank(&self, project_id: &EntityId) -> Result<f64>;

    /// A rank that sits between two neighbours, either of which may be absent.
    ///
    /// This is what "move it above the auth work" resolves to. Fractional, so
    /// the move touches one row rather than renumbering everything below it.
    fn rank_between(&self, before: Option<f64>, after: Option<f64>) -> Result<f64>;

    /// Reject a parent that does not exist, is in another project, or would
    /// make a cycle. Called on the way in, because a cycle is unrenderable and
    /// the store is the only place that can see the whole chain.
    fn check_task_parent(&self, task: &crate::Task) -> Result<()>;

    /// A project key that no other project holds, starting from `base`.
    fn unique_project_key(&self, base: &str) -> Result<String>;

    /// The id of the most recent event, if there is one.
    ///
    /// One row, not a scan. The daemon reads this twice per tool call to notice
    /// that something changed, and it used to do so by fetching up to 100,000
    /// events and taking the last — twice, per call, while holding the global
    /// write lock. On a store of a few hundred events that was merely wasteful;
    /// it is quadratic in the wrong direction and the lock made it everyone's
    /// problem.
    fn latest_event_id(&self) -> Result<Option<crate::EventId>>;

    /// One entity's history, oldest first.
    ///
    /// Separate from [`EntityStore::events`] rather than another parameter on
    /// it: that one is a cursor-following feed over a whole project, where
    /// paging must visit every event exactly once, and a filter that removes
    /// rows from under a cursor breaks that guarantee. This one answers a
    /// different question — "what has happened to *this*" — and a row's whole
    /// history is small enough to want in one piece.
    fn events_for(&self, entity_id: &EntityId, limit: usize) -> Result<Page<Event>>;

    /// Append a note to a row's running commentary.
    ///
    /// Fails if the subject does not exist. A note pointing at nothing is
    /// unrecoverable in a way an ordinary orphan is not — nothing links to a
    /// note, so there is no traversal that would ever surface it again.
    fn add_note(&mut self, note: NewNote, provenance: &Provenance) -> Result<Note>;

    /// One row's notes, oldest first.
    ///
    /// Retracted notes are excluded unless `include_retracted`, because the
    /// overwhelmingly common caller is a renderer showing current commentary,
    /// and making that caller filter is how retracted notes end up in output.
    fn notes_for(&self, entity_id: &EntityId, include_retracted: bool) -> Result<Vec<Note>>;

    /// Every live note in a project, oldest first.
    ///
    /// The renderer needs fifty streams at once; asking for them one row at a
    /// time is fifty round trips to answer one question.
    fn notes_in_project(&self, project_id: &EntityId) -> Result<Vec<Note>>;

    /// Retract a note. Soft, like every other removal in the store.
    fn retract_note(&mut self, id: &crate::NoteId, provenance: &Provenance) -> Result<Note>;
}

/// Link traversal. Nobody hand-writes a recursive CTE at a call site.
pub trait GraphStore {
    /// Walk the graph from `root`.
    ///
    /// `direction` is [`Direction::Outbound`] to follow edges away from the
    /// root and [`Direction::Inbound`] to follow edges into it. Getting this
    /// backwards returns an empty set that looks exactly like a legitimate
    /// "nothing is linked here" — read `product/SPEC.md` §3.3 before choosing.
    ///
    /// An empty `rels` means every stored relation. `depth` is clamped to
    /// [`crate::MAX_DEPTH`].
    fn neighbours(
        &self,
        root: &EntityId,
        direction: Direction,
        rels: &[Relation],
        depth: u8,
    ) -> Result<Vec<Neighbour>>;

    /// The edges immediately touching an entity, unwalked.
    fn links_of(&self, id: &EntityId, direction: Direction) -> Result<Vec<Link>>;
}

/// Lance revisions, blobs, embeddings and search.
pub trait DocumentStore {
    /// Append a revision. Returns the stored document, whose `version` is
    /// whatever the store actually assigned.
    fn write_revision(&mut self, document: Document) -> Result<Document>;

    /// Fetch a revision — the current one if `version` is `None`.
    fn revision(&self, entity_id: &EntityId, version: Option<i32>) -> Result<Option<Document>>;

    /// Every revision of a document, oldest first.
    fn revisions(&self, entity_id: &EntityId) -> Result<Vec<Document>>;

    /// A unified diff between two revisions. Satisfies REQ-2 at the API layer,
    /// not only in the UI.
    fn diff(&self, entity_id: &EntityId, from: i32, to: i32) -> Result<DocumentDiff>;

    /// Hybrid search across both indexes, fused.
    fn search(&self, query: &SearchQuery) -> Result<Page<SearchHit>>;

    /// Store bytes.
    fn put_blob(&mut self, blob: Blob) -> Result<crate::BlobId>;

    /// Fetch bytes.
    fn get_blob(&self, blob_id: &crate::BlobId) -> Result<Option<Blob>>;
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_page_reports_truncation_honestly() {
        let cut = Page::new(vec![1, 2, 3], 40);
        assert!(cut.truncated);
        assert_eq!(cut.total, 40);

        let whole = Page::new(vec![1, 2, 3], 3);
        assert!(!whole.truncated);

        let complete = Page::complete(vec![1, 2, 3]);
        assert!(!complete.truncated);
        assert_eq!(complete.total, 3);
    }

    #[test]
    fn mapping_a_page_preserves_the_truncation_report() {
        let mapped = Page::new(vec![1, 2], 9).map(|n| n * 2);
        assert_eq!(mapped.items, vec![2, 4]);
        assert_eq!(mapped.total, 9);
        assert!(mapped.truncated);
    }

    #[test]
    fn inner_search_limit_is_four_times_the_outer() {
        let q = SearchQuery {
            limit: 25,
            ..SearchQuery::new("onboarding")
        };
        assert_eq!(q.inner_limit(), 100, "SPEC §5: k_inner = k_outer * 4");
    }

    #[test]
    fn a_tiny_outer_limit_still_retrieves_enough_to_filter() {
        // k=1 would otherwise retrieve 4 rows, and a project filter could
        // then discard all of them and report "no results" wrongly.
        let q = SearchQuery {
            limit: 1,
            ..SearchQuery::new("x")
        };
        assert!(q.inner_limit() >= 20);
    }
}
