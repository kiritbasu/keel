//! Storage: three traits and one SQLite implementation.
//!
//! The split is the one `product/CLAUDE.md` mandates:
//!
//! - [`EntityStore`] — entity CRUD, links and events.
//! - [`DocumentStore`] — revisions, blobs, embeddings and search.
//! - [`GraphStore`] — link traversal, and nothing else.
//!
//! The traits predate the engine underneath them and outlived it. Keel ran on
//! DuckDB for rows and Lance for documents until Phase 9 moved everything into
//! one SQLite file; the three traits did not change, which is what that boundary
//! was insisted on in Phase 0 to buy.
//!
//! No raw SQL exists outside these implementations. That is not tidiness: the
//! graph queries are wrong in a way that returns *plausible empty results*,
//! which is the worst failure mode available, so centralising them means
//! getting the direction right once instead of at every call site.

pub mod docs;
pub mod entity;
pub mod graph;
pub mod patch;
pub mod rows;
pub mod schema;
pub mod search;
pub mod vector;

pub use patch::{FieldChange, apply_changes};

use crate::{
    Cursor, Direction, Document, DocumentDiff, Entity, EntityId, EntityType, Error, Event, Link,
    NewEvent, NewLink, NewNote, Note, Provenance, Relation, Result,
};
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The store file's name inside a Keel home directory.
pub const STORE_FILE: &str = "keel.sqlite";

/// Where the store lives inside `home`.
///
/// A one-line function so that no two surfaces can disagree about it. They used
/// to be handed the home directory itself, because the store *was* a directory;
/// it is now a file inside one, and a surface that appends the wrong name
/// silently opens an empty store rather than failing — which is the failure mode
/// worth spending a function on.
pub fn store_path(home: impl AsRef<Path>) -> PathBuf {
    home.as_ref().join(STORE_FILE)
}

/// A Keel store backed by one SQLite file.
///
/// Owns its connection. The daemon still owns the single write path, per the
/// first hard constraint — but the reason it still does is worth stating,
/// because SQLite would now permit otherwise. In WAL mode a second process can
/// read this file while a write is open, and a reader measured at 12 µs during
/// an open ten-thousand-row transaction. The single write path survives because
/// six of the seven steps in a Keel write have nothing to do with locking:
/// validation, provenance, the event, the revision, the embedding and the index
/// all still need one place that knows how to do them.
pub struct Store {
    conn: Connection,
    path: PathBuf,
    embedder: Option<std::sync::Arc<dyn crate::Embedder>>,
}

/// Hand-written because a connection and an embedder have nothing worth
/// printing, and because `expect_err` on a failed open needs *something* — a
/// store that cannot be formatted makes the error path harder to assert on than
/// the success path.
impl std::fmt::Debug for Store {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Store")
            .field("path", &self.path)
            .field("embedder", &self.embedder.is_some())
            .finish()
    }
}

impl Store {
    /// Open or create a store at `path`, applying any migrations it is missing.
    ///
    /// Creating the parent directory is deliberate: the daemon's first run has
    /// no `~/.keel`, and failing with "unable to open database file" for a
    /// missing directory is a worse first experience than making it.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|source| Error::Io {
                context: format!("create the store directory at {}", parent.display()),
                source,
            })?;
        }

        vector::register();

        let conn = Connection::open(&path).map_err(Error::storage(format!(
            "open the store at {}",
            path.display()
        )))?;

        let mut store = Store {
            conn,
            path,
            embedder: None,
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    /// An in-memory store, for tests.
    ///
    /// Its own constructor rather than `open(":memory:")` so that a test cannot
    /// accidentally get a temporary file, and so the leak KEEL-119 describes —
    /// a killed test run leaving a store behind in `TMPDIR` — has a
    /// zero-footprint alternative for the tests that do not need a path.
    pub fn in_memory() -> Result<Self> {
        vector::register();
        let conn =
            Connection::open_in_memory().map_err(Error::storage("open an in-memory store"))?;
        let mut store = Store {
            conn,
            path: PathBuf::from(":memory:"),
            embedder: None,
        };
        store.configure()?;
        store.migrate()?;
        Ok(store)
    }

    /// Attach an embedder, enabling the semantic half of hybrid search.
    ///
    /// Optional on purpose: a store with no embedder is still fully usable and
    /// still searchable by keyword, so search degrades rather than failing. Passing it in rather than building
    /// it here is what keeps `keel-core` free of decisions about model files
    /// and network access.
    ///
    /// **Attaching it is not optional in practice, though.** Without it,
    /// `search` returns keyword hits only — and that failure is silent, since
    /// results keep arriving and are merely worse. Every caller that opens a
    /// store for a human or a model should attach one.
    pub fn with_embedder(mut self, embedder: std::sync::Arc<dyn crate::Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// The attached embedder, if any.
    pub fn embedder(&self) -> Option<&dyn crate::Embedder> {
        self.embedder.as_deref()
    }

    /// Where this store lives. `:memory:` for an in-memory one.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The connection.
    ///
    /// Public because `fsck`, `backup` and the store's own tests need to ask the
    /// engine questions that no trait method should grow a signature for —
    /// table row counts, `PRAGMA integrity_check`, the migration ledger. The rule it does not weaken is that no *call site*
    /// writes SQL; these are the store's own tools reading their own store.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Fold the write-ahead log back into the database file.
    ///
    /// Called on shutdown, and it is a convenience rather than a safeguard: a
    /// killed process leaves a `-wal` beside the store which the next open
    /// replays, so nothing is lost either way. What it buys is that the file on disk is the
    /// whole store — which is what a person copying it, or a backup taken by
    /// something that does not know about SQLite, would otherwise get wrong.
    ///
    /// `TRUNCATE` rather than `PASSIVE` so the log is actually emptied instead
    /// of merely checkpointed; a reader mid-query blocks it, and that is fine,
    /// because failing to checkpoint costs nothing.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(Error::storage("checkpoint the store before shutting down"))
    }

    /// Connection settings, each of which is load-bearing.
    fn configure(&mut self) -> Result<()> {
        // WAL is the whole reason the app stops stalling behind a write. In the
        // default rollback journal a reader blocks for the duration of a write
        // transaction; in WAL it takes a consistent pre-transaction snapshot
        // and does not wait at all — 12 µs against an open ten-thousand-row
        // write, measured before this was written.
        //
        // `query_row` rather than `execute_batch`: setting journal_mode returns
        // the mode it ended up in, and a statement that returns a row is an
        // error when run as a batch. It is also worth reading, because the
        // request can silently fail — an in-memory database cannot use WAL.
        let mode: String = self
            .conn
            .query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))
            .map_err(Error::storage("put the store into WAL mode"))?;
        tracing::debug!(journal_mode = %mode, "store opened");

        self.conn
            .execute_batch(
                // NORMAL rather than FULL: in WAL mode this fsyncs at
                // checkpoints instead of at every commit. The exposure is that
                // a power cut can lose the last few transactions; it cannot
                // corrupt the database, which is the property that matters.
                // FULL would put an fsync in the path of every note.
                "PRAGMA synchronous = NORMAL;
                 -- Enforced, not decorative. Nothing in Keel is ever DELETEd,
                 -- so these never cascade; what they catch is a link or a note
                 -- written against a row that does not exist, which used to be
                 -- something only `fsck` could find, after the fact.
                 PRAGMA foreign_keys = ON;
                 -- Five seconds before giving up on a locked database. The
                 -- daemon is the only writer, so this should never be reached;
                 -- if it is, waiting beats failing, because the alternative is
                 -- a tool call that returns an error for a store that was
                 -- merely busy.
                 PRAGMA busy_timeout = 5000;",
            )
            .map_err(Error::storage("configure the store connection"))
    }

    /// Apply every migration this store has not seen, in order.
    ///
    /// Forward-only. There is no `down`: rolling a schema backwards on a
    /// single-user store is a fiction that costs more to maintain than it
    /// repays, and SPEC §11 runs a backup before every migration anyway —
    /// restoring is the rollback.
    fn migrate(&mut self) -> Result<()> {
        self.conn
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS _keel_migrations (
                   id         INTEGER PRIMARY KEY,
                   name       TEXT NOT NULL,
                   applied_at TEXT NOT NULL
                 ) STRICT;",
            )
            .map_err(Error::storage("create the migration ledger"))?;

        // Refuse to run against a store newer than this binary understands.
        //
        // Written after the failure it prevents actually happened, in the store
        // this one replaced: a migration added a column, a daemon
        // built before that migration kept running, found every migration it
        // knew about already applied, concluded it was up to date, and went on
        // inserting rows with the new column left NULL. The corruption surfaced
        // two days later as an unrelated-looking read error.
        //
        // An older binary is not merely missing features — it writes rows that
        // are wrong in ways the schema cannot express. Refusing to open turns a
        // silent corruption into a startup error, which is the whole trade.
        let shipped = schema::migrations().iter().map(|m| m.id).max().unwrap_or(0);
        let newest: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(max(id), 0) FROM _keel_migrations",
                [],
                |r| r.get(0),
            )
            .map_err(Error::storage("read the migration ledger"))?;
        if newest > i64::from(shipped) {
            return Err(Error::Invariant {
                operation: "open the store".to_owned(),
                problem: format!(
                    "this store is at schema {newest}; this binary only understands {shipped}, \
                     so it is older than the store.\n\n\
                     It would write rows the newer schema expects to be populated and leave them \
                     empty, which does not fail until something else reads them.\n\n\
                     Rebuild and reinstall: ./plugin/install.sh\n\
                     To run an old binary deliberately, point it at another store with --home."
                ),
            });
        }

        for migration in schema::migrations() {
            let seen: i64 = self
                .conn
                .query_row(
                    "SELECT count(*) FROM _keel_migrations WHERE id = ?1",
                    [migration.id],
                    |r| r.get(0),
                )
                .map_err(Error::storage("read the migration ledger"))?;
            if seen > 0 {
                continue;
            }

            // The DDL and the ledger entry go in together. A migration that
            // ran but was not recorded is a migration that runs again on the
            // next open, against a schema that already has its tables.
            let tx = self
                .conn
                .transaction()
                .map_err(Error::storage("begin a migration"))?;
            tx.execute_batch(migration.sql)
                .map_err(Error::storage(format!(
                    "apply migration {} ({})",
                    migration.id, migration.name
                )))?;
            tx.execute(
                "INSERT INTO _keel_migrations (id, name, applied_at) VALUES (?1, ?2, ?3)",
                rusqlite::params![
                    migration.id,
                    migration.name,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .map_err(Error::storage("record a migration"))?;
            tx.commit()
                .map_err(Error::storage(format!("commit migration {}", migration.id)))?;

            tracing::info!(
                id = migration.id,
                name = migration.name,
                "migration applied"
            );
        }
        Ok(())
    }
}

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
    /// The FTS5 keyword index, which covers every searchable artifact.
    Keyword,
    /// The `sqlite-vec` index, over prose embeddings.
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

/// Entity CRUD, links and events.
pub trait EntityStore {
    /// Create an entity, or return the existing one with the same idempotency
    /// key.
    ///
    /// Takes `&mut self` to express the single-writer rule at the type level
    /// (D-5), not because the engine requires it. SQLite in WAL mode would
    /// permit a second writer; the rule is Keel's, and the signature is where
    /// it is stated.
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

/// Revisions, blobs, embeddings and search.
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

    /// Every column the row mapping declares must exist in the schema, for all
    /// thirteen types.
    ///
    /// This is the highest-value test in the file. `TableSpec` drives both the
    /// `SELECT` list and the `INSERT` parameter order, so a column the spec
    /// names and the schema lacks is not a compile error and not a nice
    /// message — it is an insert that fails at runtime for one entity type
    /// while the other twelve work, or worse, a select that silently returns
    /// nothing for a field nobody looks at often.
    ///
    /// Asked of a real database via `PRAGMA table_info` rather than by
    /// grepping the DDL string, because the question is what SQLite actually
    /// built, not what the text appears to say.
    #[test]
    fn the_schema_has_every_column_the_row_specs_declare() {
        let store = Store::in_memory().unwrap();
        let mut missing: Vec<String> = Vec::new();

        for ty in crate::EntityType::ALL {
            let spec = crate::store::rows::spec_for(ty);

            let mut stmt = store
                .connection()
                .prepare(&format!("PRAGMA table_info({})", spec.table))
                .unwrap();
            let actual: Vec<String> = stmt
                .query_map([], |r| r.get::<_, String>(1))
                .unwrap()
                .collect::<std::result::Result<_, _>>()
                .unwrap();

            assert!(
                !actual.is_empty(),
                "the schema has no table called {} for {}",
                spec.table,
                ty.as_str()
            );

            // The audit block is shared and is appended to every insert, so it
            // has to be present on every entity table too.
            const AUDIT: &[&str] = &[
                "created_at",
                "updated_at",
                "version",
                "created_by",
                "updated_by",
                "session_id",
                "surface",
                "archived_at",
            ];

            let wanted = spec
                .cols
                .iter()
                .map(|c| match c {
                    crate::store::rows::Col::Plain(n) | crate::store::rows::Col::Array(n) => *n,
                })
                .chain(AUDIT.iter().copied());

            for col in wanted {
                if !actual.iter().any(|a| a == col) {
                    missing.push(format!("{}.{col} (for {})", spec.table, ty.as_str()));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "the row specs name columns the schema does not have:\n  {}",
            missing.join("\n  ")
        );
    }

    /// The keyword index updates as rows change, with no rebuild.
    ///
    /// This is the whole of KEEL-123's stall, asserted. The store this replaced
    /// could not do it: its full-text index did not update when its table
    /// changed, so the first search after *any* write rebuilt the entire index
    /// — 217 ms against a 13 ms mean, measured on the live store while a
    /// decision was being written. FTS5's triggers keep the index in step with
    /// the rows, so there is no rebuild to pay for.
    ///
    /// A test that only inserted would pass against an index that is never
    /// maintained again, so this also changes a row and archives one.
    #[test]
    fn the_keyword_index_follows_the_rows_without_a_rebuild() {
        let store = Store::in_memory().unwrap();
        let conn = store.connection();

        // Quoted, because `MATCH` takes a query *language*, not a string.
        // `local-first` unquoted parses as the term `local` with a column
        // filter, and fails with "no such column: first" — which names a word
        // from the text and sounds like a schema problem. Whatever lands
        // KEEL-126 has to quote caller input for exactly this reason.
        let matches = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH ?1",
                [format!("\"{q}\"")],
                |r| r.get(0),
            )
            .unwrap()
        };

        conn.execute_batch(
            "INSERT INTO projects
               (id, slug, key, name, description, idempotency_key, created_at,
                updated_at, created_by, updated_by, version)
             VALUES ('prj_1','keel','KEEL','Keel','a local-first store','k1',
                     '2026-08-11T00:00:00.000000Z','2026-08-11T00:00:00.000000Z',
                     'claude','claude',1);
             INSERT INTO tasks
               (id, project_id, number, title, body, summary, idempotency_key,
                created_at, updated_at, created_by, updated_by, version)
             VALUES ('tsk_1','prj_1',1,'The board is slow',
                     'the keyword index is rebuilt on every write','a summary','k2',
                     '2026-08-11T00:00:00.000000Z','2026-08-11T00:00:00.000000Z',
                     'claude','claude',1);",
        )
        .unwrap();

        // Findable immediately, in the very next statement, with nothing having
        // asked the index to catch up.
        assert_eq!(
            matches("keyword"),
            1,
            "a row written should be findable at once"
        );
        assert_eq!(
            matches("local-first"),
            1,
            "the project should be indexed too"
        );

        // An update has to move the index with it, or search keeps returning
        // text that is no longer there.
        conn.execute(
            "UPDATE tasks SET body = 'now it says something else entirely' WHERE id = 'tsk_1'",
            [],
        )
        .unwrap();
        assert_eq!(
            matches("keyword"),
            0,
            "the old text should have left the index"
        );
        assert_eq!(matches("entirely"), 1, "the new text should be in it");

        // Archiving takes it out. Search must not offer something a person put
        // away, and doing that here means no query has to remember to filter.
        conn.execute(
            "UPDATE tasks SET archived_at = '2026-08-11T01:00:00.000000Z' WHERE id = 'tsk_1'",
            [],
        )
        .unwrap();
        assert_eq!(
            matches("entirely"),
            0,
            "an archived row should leave the index"
        );
    }

    /// A prose type is indexed from its document, not its row, and a new
    /// revision replaces its predecessor rather than piling up beside it.
    ///
    /// Without the replace, a heavily-edited spec would appear once per version
    /// and outrank everything by sheer repetition.
    #[test]
    fn a_document_is_indexed_once_however_many_revisions_it_has() {
        let store = Store::in_memory().unwrap();
        let conn = store.connection();

        conn.execute_batch(
            "INSERT INTO documents
               (doc_id, entity_type, entity_id, project_id, version, title, body,
                body_hash, status, author, created_at)
             VALUES ('doc_1','spec','spc_1','prj_1',1,'A spec','the original wording',
                     'h1','current','claude','2026-08-11T00:00:00.000000Z');",
        )
        .unwrap();

        conn.execute_batch(
            "UPDATE documents SET status = 'superseded' WHERE doc_id = 'doc_1';
             INSERT INTO documents
               (doc_id, entity_type, entity_id, project_id, version, title, body,
                body_hash, status, author, created_at)
             VALUES ('doc_2','spec','spc_1','prj_1',2,'A spec','the replacement wording',
                     'h2','current','claude','2026-08-11T00:00:01.000000Z');",
        )
        .unwrap();

        let count = |q: &str| -> i64 {
            conn.query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH ?1",
                [q],
                |r| r.get(0),
            )
            .unwrap()
        };

        assert_eq!(
            count("replacement"),
            1,
            "the current revision should be findable"
        );
        assert_eq!(
            count("original"),
            0,
            "the superseded revision should not still be in the index"
        );

        let rows: i64 = conn
            .query_row(
                "SELECT count(*) FROM fts_source WHERE entity_id = 'spc_1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(rows, 1, "two revisions should occupy one slot, not two");
    }

    #[test]
    fn a_new_store_has_every_table() {
        let store = Store::in_memory().unwrap();
        let mut stmt = store
            .connection()
            .prepare("SELECT name FROM sqlite_master WHERE type IN ('table','view') ORDER BY name")
            .unwrap();
        let names: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        for expected in [
            "projects",
            "milestones",
            "tasks",
            "specs",
            "decisions",
            "questions",
            "terms",
            "feedback",
            "design_artifacts",
            "environments",
            "artifacts",
            "metrics",
            "metric_observations",
            "links",
            "notes",
            "events",
            "documents",
            "blobs",
            "v_entities",
            "fts_entities",
        ] {
            assert!(
                names.iter().any(|n| n == expected),
                "a new store is missing {expected}; it has {names:?}"
            );
        }
    }

    /// Opening twice must not re-run migration 1 against tables that exist.
    /// This is the failure the ledger prevents, and it is silent until the
    /// second open.
    #[test]
    fn opening_an_existing_store_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keel.sqlite");

        let first = Store::open(&path).unwrap();
        let applied: i64 = first
            .connection()
            .query_row("SELECT count(*) FROM _keel_migrations", [], |r| r.get(0))
            .unwrap();
        drop(first);

        let second = Store::open(&path).unwrap();
        let again: i64 = second
            .connection()
            .query_row("SELECT count(*) FROM _keel_migrations", [], |r| r.get(0))
            .unwrap();

        assert_eq!(
            applied, again,
            "reopening applied a migration a second time"
        );
    }

    /// A file store must be in WAL, or a reader blocks behind every write and
    /// the board stalls exactly as it did before.
    #[test]
    fn a_file_store_uses_write_ahead_logging() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(dir.path().join("keel.sqlite")).unwrap();
        let mode: String = store
            .connection()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    /// `STRICT` is what makes a bad write fail at the write. Without it SQLite
    /// stores the string in the integer column and the failure surfaces
    /// somewhere else entirely, usually as a deserialisation error on a read
    /// that has nothing to do with whoever wrote it.
    ///
    /// The direction matters and it is easy to get backwards: STRICT converts
    /// where the conversion is lossless, so an integer *into* a TEXT column is
    /// accepted and becomes text. What it refuses is text that is not a number
    /// going into an INTEGER column, which is the mistake worth catching.
    #[test]
    fn text_in_an_integer_column_is_refused() {
        let store = Store::in_memory().unwrap();
        let bad = store.connection().execute(
            "INSERT INTO projects
               (id, slug, name, idempotency_key, created_at, updated_at,
                created_by, updated_by, version)
             VALUES ('prj_1', 'p', 'P', 'k', 'now', 'now', 'claude', 'claude', 'not a number')",
            [],
        );
        assert!(
            bad.is_err(),
            "STRICT should have refused non-numeric text in an INTEGER column"
        );

        // The lossless direction is accepted, and asserting it keeps the test
        // honest about what STRICT does rather than what it is hoped to do.
        let ok = store.connection().execute(
            "INSERT INTO projects
               (id, slug, name, idempotency_key, created_at, updated_at,
                created_by, updated_by, version)
             VALUES ('prj_2', 'q', 'Q', 'k2', 'now', 'now', 'claude', 'claude', 3)",
            [],
        );
        assert!(ok.is_ok(), "a well-typed insert should have been accepted");
    }

    #[test]
    fn the_store_reports_where_it_lives() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("keel.sqlite");
        let store = Store::open(&path).unwrap();
        assert_eq!(store.path(), path);
        assert!(
            path.exists(),
            "open should have created the parent directory"
        );
    }
}
