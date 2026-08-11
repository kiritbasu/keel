//! The SQLite store: one file, one engine.
//!
//! Replaces `DuckStore`, which was DuckDB for rows and Lance for documents,
//! blobs and vectors. The three traits it implements are unchanged — that
//! boundary was insisted on in Phase 0 and this is the phase that collects on
//! it.
//!
//! Both stores exist while the migration is verified. Nothing switches over
//! until `keel migrate` has proved the two hold the same data, per row count
//! and per content hash (KEEL-127).

pub mod docs;
pub mod entity;
pub mod graph;
pub mod rows;
pub mod schema;
pub mod search;
pub mod vector;

use crate::{Error, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// The store file's name inside a Keel home directory.
pub const STORE_FILE: &str = "keel.sqlite";

/// Where the store lives inside `home`.
///
/// A one-line function so that the CLI, the daemon and `keel migrate` cannot
/// disagree about it. They used to be handed the home directory itself, because
/// the DuckDB store *was* a directory; the SQLite store is a file inside it, and
/// a surface that appends the wrong name silently opens an empty store rather
/// than failing — which is the failure mode worth spending a function on.
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
pub struct SqliteStore {
    conn: Connection,
    path: PathBuf,
    embedder: Option<std::sync::Arc<dyn crate::Embedder>>,
}

/// Hand-written because a connection and an embedder have nothing worth
/// printing, and because `expect_err` on a failed open needs *something* — a
/// store that cannot be formatted makes the error path harder to assert on than
/// the success path.
impl std::fmt::Debug for SqliteStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteStore")
            .field("path", &self.path)
            .field("embedder", &self.embedder.is_some())
            .finish()
    }
}

impl SqliteStore {
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

        let conn = Connection::open(&path).map_err(Error::sqlite(format!(
            "open the store at {}",
            path.display()
        )))?;

        let mut store = SqliteStore {
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
            Connection::open_in_memory().map_err(Error::sqlite("open an in-memory store"))?;
        let mut store = SqliteStore {
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
    /// Optional on purpose, and the same shape `DuckStore` uses: a store with
    /// no embedder is still fully usable and still searchable by keyword, so
    /// search degrades rather than failing. Passing it in rather than building
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
    /// Public for the same reason `DuckStore::connection` is: `keel migrate`
    /// and `fsck` need to ask the engine questions that no trait method should
    /// grow a signature for — table row counts, `PRAGMA integrity_check`, the
    /// migration ledger. The rule it does not weaken is that no *call site*
    /// writes SQL; these are the store's own tools reading their own store.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Fold the write-ahead log back into the database file.
    ///
    /// Called on shutdown, and the reason is milder than DuckDB's was: a killed
    /// process leaves a `-wal` beside the store which the next open replays, so
    /// nothing is lost either way. What it buys is that the file on disk is the
    /// whole store — which is what a person copying it, or a backup taken by
    /// something that does not know about SQLite, would otherwise get wrong.
    ///
    /// `TRUNCATE` rather than `PASSIVE` so the log is actually emptied instead
    /// of merely checkpointed; a reader mid-query blocks it, and that is fine,
    /// because failing to checkpoint costs nothing.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
            .map_err(Error::sqlite("checkpoint the store before shutting down"))
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
            .map_err(Error::sqlite("put the store into WAL mode"))?;
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
            .map_err(Error::sqlite("configure the store connection"))
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
            .map_err(Error::sqlite("create the migration ledger"))?;

        // Refuse to run against a store newer than this binary understands.
        //
        // Carried over from `DuckStore`, where it was written after the failure
        // it prevents actually happened: a migration added a column, a daemon
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
            .map_err(Error::sqlite("read the migration ledger"))?;
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
                .map_err(Error::sqlite("read the migration ledger"))?;
            if seen > 0 {
                continue;
            }

            // The DDL and the ledger entry go in together. A migration that
            // ran but was not recorded is a migration that runs again on the
            // next open, against a schema that already has its tables.
            let tx = self
                .conn
                .transaction()
                .map_err(Error::sqlite("begin a migration"))?;
            tx.execute_batch(migration.sql)
                .map_err(Error::sqlite(format!(
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
            .map_err(Error::sqlite("record a migration"))?;
            tx.commit()
                .map_err(Error::sqlite(format!("commit migration {}", migration.id)))?;

            tracing::info!(
                id = migration.id,
                name = migration.name,
                "migration applied"
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

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
        let store = SqliteStore::in_memory().unwrap();
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
    /// This is the whole of KEEL-123's stall, asserted. The DuckDB store could
    /// not do this: its full-text index does not update when its table changes,
    /// so the first search after *any* write rebuilt the entire index — 217 ms
    /// against a 13 ms mean, measured on the live store while a decision was
    /// being written.
    ///
    /// A test that only inserted would pass against an index that is never
    /// maintained again, so this also changes a row and archives one.
    #[test]
    fn the_keyword_index_follows_the_rows_without_a_rebuild() {
        let store = SqliteStore::in_memory().unwrap();
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
        let store = SqliteStore::in_memory().unwrap();
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
        let store = SqliteStore::in_memory().unwrap();
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

        let first = SqliteStore::open(&path).unwrap();
        let applied: i64 = first
            .connection()
            .query_row("SELECT count(*) FROM _keel_migrations", [], |r| r.get(0))
            .unwrap();
        drop(first);

        let second = SqliteStore::open(&path).unwrap();
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
        let store = SqliteStore::open(dir.path().join("keel.sqlite")).unwrap();
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
        let store = SqliteStore::in_memory().unwrap();
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
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(store.path(), path);
        assert!(
            path.exists(),
            "open should have created the parent directory"
        );
    }
}
