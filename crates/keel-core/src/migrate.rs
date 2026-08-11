//! Read the DuckDB-and-Lance store, write a SQLite one, and prove they match.
//!
//! # It is a row copy, not a re-import
//!
//! The obvious way to write this is to read each artifact and call
//! [`crate::EntityStore::create`] on the new store. That would be wrong, and
//! wrong in a way that only shows up later:
//!
//! - it re-derives idempotency keys, so a row's key changes and the next
//!   `keel_create` that should have been a no-op creates a duplicate;
//! - it assigns fresh task and decision numbers, and `KEEL-42` is cited in prose
//!   that nothing rewrites;
//! - it emits a creation event per row, so the event log — the thing being
//!   migrated — doubles, with today's timestamp on history from months ago;
//! - it re-runs validation, and around a hundred legacy rows would fail it: a
//!   task closed before the reason-and-evidence rule existed, a task with no
//!   summary, a decision whose body predates the prose rules.
//!
//! A migration copies rows verbatim. It does not re-litigate them.
//!
//! # Fidelity is inherited, not hand-rolled
//!
//! The thirteen entity types go out through [`crate::store::rows`] and in
//! through [`crate::store::sqlite::rows`]. Both mapping layers already exist and
//! are tested against their own engine, and both are generated from the same
//! [`crate::store::rows::TableSpec`] — so the column order cannot disagree, and
//! nothing here restates thirteen column lists a fourteenth time.
//!
//! `links`, `notes` and `events` are not entities, so they are copied column by
//! column. They are read as raw values rather than parsed into their structs on
//! purpose: a `rel` or a `surface` that no longer parses is still a row that has
//! to survive the move, and a migration is the worst possible moment to discover
//! that an enum lost a variant.
//!
//! # What the old store never sees
//!
//! Nothing here writes to it. Every statement against `from` is a `SELECT`, the
//! whole copy runs in one SQLite transaction, and a target that already holds
//! rows is refused rather than appended to. A failed migration therefore costs
//! nothing and can be retried against the same source.

use crate::store::rows::{from_row as duck_from_row, spec_for};
use crate::store::sqlite::rows::{
    insert_params as sqlite_insert_params, insert_stmt as sqlite_insert_stmt,
};
use crate::store::{DuckStore, SqliteStore};
use crate::{EntityType, Error, Result, body_hash, sha256_hex};
use chrono::{DateTime, Utc};
use duckdb::types::Value as DuckValue;
use rusqlite::types::Value as SqlValue;
use std::collections::BTreeMap;

/// How a timestamp is spelled in the new store's TEXT columns.
///
/// The same format `store::sqlite::rows` writes, and the fixed six fractional
/// digits are the whole point: every `ORDER BY created_at` is a string
/// comparison once the column is TEXT, and it is only correct while
/// lexicographic order matches chronological order.
///
/// It is spelled out again here rather than shared because the store's copy is
/// `pub(super)`, and reaching into another module's privacy for a migration
/// would be a worse coupling than a constant with a test holding it in place.
/// `timestamps_match_what_the_store_itself_writes` below writes a row through
/// the store's own mapping and compares the text, so the two cannot drift
/// without a failure that names them.
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6fZ";

/// What a migration copied.
///
/// There is no "what it could not" beside this, and that is a claim rather than
/// an omission: every column of every table has somewhere to go, so a table
/// missing from `rows` is a bug and not a known gap.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MigrationReport {
    /// Rows written, per table in the new store.
    pub rows: BTreeMap<String, usize>,
}

impl MigrationReport {
    /// Total rows written across every table.
    pub fn total_rows(&self) -> usize {
        self.rows.values().sum()
    }
}

impl std::fmt::Display for MigrationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "copied {} rows across {} tables",
            self.total_rows(),
            self.rows.len()
        )?;
        for (table, n) in &self.rows {
            writeln!(f, "  {table}: {n}")?;
        }
        Ok(())
    }
}

/// One thing the two stores disagree about.
///
/// A bool would say the migration failed; this says which table, which row, and
/// what each side held — which is the difference between a verification you can
/// act on and one you can only rerun.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Difference {
    /// The table it is in.
    pub table: String,
    /// Which row, or `None` when the whole table disagrees.
    pub id: Option<String>,
    /// What was being compared — a row count, a hash, an embedding.
    pub what: String,
    /// What the old store held.
    pub in_old: String,
    /// What the new store holds.
    pub in_new: String,
}

impl std::fmt::Display for Difference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.id {
            Some(id) => write!(f, "{}/{id}: {}", self.table, self.what)?,
            None => write!(f, "{}: {}", self.table, self.what)?,
        }
        write!(f, " — old store {}, new store {}", self.in_old, self.in_new)
    }
}

/// Both sides' row count for one table.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TableCount {
    /// The table's name in the new store.
    pub table: String,
    /// Rows in the old store.
    pub in_old: i64,
    /// Rows in the new store.
    pub in_new: i64,
}

/// What the comparison found.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VerificationReport {
    /// Every table, both sides, whether it matched or not — so a clean run is
    /// visibly a comparison that happened rather than a check that was skipped.
    pub counts: Vec<TableCount>,
    /// Documents whose content hash was compared.
    pub documents_hashed: usize,
    /// Blobs whose bytes were re-hashed.
    pub blobs_hashed: usize,
    /// Everything that differed. Empty means the two stores agree.
    pub differences: Vec<Difference>,
}

impl VerificationReport {
    /// Whether the two stores hold the same data.
    pub fn is_clean(&self) -> bool {
        self.differences.is_empty()
    }

    /// Total rows compared, counted on the old store's side.
    pub fn total_rows(&self) -> i64 {
        self.counts.iter().map(|c| c.in_old).sum()
    }
}

impl std::fmt::Display for VerificationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.is_clean() {
            return write!(
                f,
                "identical: {} rows across {} tables, {} document hashes and {} blob hashes match",
                self.total_rows(),
                self.counts.len(),
                self.documents_hashed,
                self.blobs_hashed
            );
        }
        writeln!(f, "{} differences:", self.differences.len())?;
        for d in &self.differences {
            writeln!(f, "  {d}")?;
        }
        Ok(())
    }
}

/// Every table the migration copies, as (its name in the new store, its name in
/// the old).
///
/// The entity tables come from [`spec_for`] rather than a second list, so there
/// is nothing to keep in step. Documents and blobs are the only rename: they
/// were Lance datasets reached through the attached `lancedb` namespace and are
/// ordinary tables now.
fn tables() -> Vec<(String, String)> {
    let mut v: Vec<(String, String)> = EntityType::ALL
        .iter()
        .map(|t| {
            let name = spec_for(*t).table.to_owned();
            (name.clone(), name)
        })
        .collect();
    for name in ["links", "notes", "events"] {
        v.push((name.to_owned(), name.to_owned()));
    }
    v.push(("documents".to_owned(), "lancedb.documents".to_owned()));
    v.push(("blobs".to_owned(), "lancedb.blobs".to_owned()));
    v
}

/// Copy everything from `from` into `to`, and report what was written.
///
/// `to` must be a store with no rows in it. Appending to one that already has
/// some would mean a retry after a partial failure silently doubled the corpus,
/// and there is no key on which the two runs could be reconciled — every row
/// carries the identity it had in the old store, so a second copy is a primary
/// key collision at best and a duplicated event log at worst.
///
/// The whole copy is one transaction. A failure leaves the target exactly as it
/// was found, which is what makes "run it again" a safe instruction.
///
/// It never writes to `from`. Stop the daemon before running it all the same:
/// DuckDB has one write lock, and a store being written while it is read is a
/// store whose two halves come from different instants.
pub fn migrate(from: &DuckStore, to: &mut SqliteStore) -> Result<MigrationReport> {
    refuse_a_populated_target(to)?;
    preflight(from)?;

    let conn = to.connection();
    conn.execute_batch("BEGIN IMMEDIATE")
        .map_err(Error::sqlite("begin the migration transaction"))?;

    match copy_everything(from, conn) {
        Ok(report) => {
            conn.execute_batch("COMMIT")
                .map_err(Error::sqlite("commit the migration"))?;
            tracing::info!(rows = report.total_rows(), "migration written");
            Ok(report)
        }
        Err(e) => {
            // Rolling back is recovery, not a failure to report: the caller
            // still gets the original error. A rollback that itself fails is
            // worth a line, because it means the target is not the empty file
            // the next attempt will expect.
            if let Err(rollback) = conn.execute_batch("ROLLBACK") {
                tracing::warn!(
                    error = %rollback,
                    "the migration failed and the rollback did too; the target may hold partial rows"
                );
            }
            Err(e)
        }
    }
}

/// Compare the two stores and name everything they disagree about.
///
/// Four questions, because each one catches a failure the others cannot see:
///
/// 1. **Row counts, per table.** Catches a whole table that was never copied.
/// 2. **A content hash per document**, recomputed from the body in the *new*
///    store and compared against the hash stored in the old one. Comparing the
///    two stored hashes instead would be vacuous — the column is copied
///    verbatim, so it would agree with itself no matter what happened to the
///    prose beside it.
/// 3. **A byte hash per blob**, likewise recomputed from the bytes that landed.
/// 4. **Embedding presence and width**, so a vector that arrived as NULL is
///    caught. Re-embedding the corpus is slow and pointless if the vectors copy
///    cleanly, which makes "did they copy cleanly" a question worth asking.
///
/// Reads only, on both sides.
pub fn verify(from: &DuckStore, to: &SqliteStore) -> Result<VerificationReport> {
    let mut report = VerificationReport::default();

    for (new_name, old_name) in tables() {
        let in_old = count_duck(from, &old_name)?;
        let in_new = count_sqlite(to, &new_name)?;
        if in_old != in_new {
            report.differences.push(Difference {
                table: new_name.clone(),
                id: None,
                what: "row count".to_owned(),
                in_old: format!("{in_old} rows"),
                in_new: format!("{in_new} rows"),
            });
        }
        report.counts.push(TableCount {
            table: new_name,
            in_old,
            in_new,
        });
    }

    compare_documents(from, to, &mut report)?;
    compare_blobs(from, to, &mut report)?;
    compare_embeddings(from, to, &mut report)?;

    Ok(report)
}

// --- Refusals ------------------------------------------------------------

/// Refuse a target that already holds rows.
fn refuse_a_populated_target(to: &SqliteStore) -> Result<()> {
    let mut populated: Vec<String> = Vec::new();
    for (new_name, _) in tables() {
        let n = count_sqlite(to, &new_name)?;
        if n > 0 {
            populated.push(format!("{new_name} ({n} rows)"));
        }
    }
    if populated.is_empty() {
        return Ok(());
    }
    Err(Error::Invariant {
        operation: format!("migrate into the store at {}", to.path().display()),
        problem: format!(
            "it already holds rows in {}. A migration copies every row with the identity it \
             had in the old store, so a second run into the same file is a duplicated event \
             log, not an update. Migrate into a fresh file and move it into place when it \
             verifies",
            populated.join(", ")
        ),
    })
}

/// Check, before writing anything, for the one legacy shape the new schema
/// cannot hold.
///
/// A task or decision with no `number` reads back as zero — that leniency is
/// deliberate and is explained where it lives, because a single unnumbered row
/// once made every row of its type unreadable (KEEL-95, B-44). SQLite's unique
/// index over `(project_id, number)` treats NULLs as distinct but two zeroes as
/// a collision, so *two* unnumbered rows in one project would fail somewhere in
/// the middle of the copy with `UNIQUE constraint failed`, which names the index
/// and not the cause.
///
/// Asking first costs two queries and turns that into a sentence saying what to
/// fix.
fn preflight(from: &DuckStore) -> Result<()> {
    for table in ["tasks", "decisions"] {
        // `table` comes from this function's own list, never from a caller.
        let sql = format!(
            "SELECT project_id, count(*) FROM {table} WHERE number IS NULL \
             GROUP BY project_id HAVING count(*) > 1"
        );
        let mut stmt = from
            .connection()
            .prepare(&sql)
            .map_err(Error::storage(format!(
                "look for unnumbered rows in {table}"
            )))?;
        let mut rows = stmt.query([]).map_err(Error::storage(format!(
            "look for unnumbered rows in {table}"
        )))?;
        if let Some(row) = rows.next().map_err(Error::storage(format!(
            "read the unnumbered rows of {table}"
        )))? {
            let project: String = row
                .get(0)
                .map_err(Error::storage(format!("read a project id from {table}")))?;
            let n: i64 = row.get(1).map_err(Error::storage(format!(
                "count the unnumbered rows of {table}"
            )))?;
            return Err(Error::Invariant {
                operation: "migrate the old store".to_owned(),
                problem: format!(
                    "{n} rows in {table} for project {project} have no number. They all read \
                     back as 0, and the new store's unique index over (project_id, number) \
                     would refuse the second one. Give them numbers in the old store first"
                ),
            });
        }
    }
    Ok(())
}

// --- The copy ------------------------------------------------------------

/// Every table, in one transaction the caller owns.
fn copy_everything(from: &DuckStore, to: &rusqlite::Connection) -> Result<MigrationReport> {
    let mut report = MigrationReport::default();

    for ty in EntityType::ALL {
        let n = copy_entities(from, to, ty)?;
        report.rows.insert(spec_for(ty).table.to_owned(), n);
    }

    report
        .rows
        .insert("links".to_owned(), copy_links(from, to)?);
    report
        .rows
        .insert("notes".to_owned(), copy_notes(from, to)?);
    report
        .rows
        .insert("events".to_owned(), copy_events(from, to)?);
    report
        .rows
        .insert("documents".to_owned(), copy_documents(from, to)?);
    report
        .rows
        .insert("blobs".to_owned(), copy_blobs(from, to)?);

    Ok(report)
}

/// Copy one entity type, archived rows included.
///
/// **No `WHERE` clause, and that is the point.** Soft delete means nothing is
/// ever gone, so a copy that skipped `archived_at IS NOT NULL` would lose the
/// audit trail while every live-row count still matched — a failure that looks
/// exactly like success.
fn copy_entities(from: &DuckStore, to: &rusqlite::Connection, ty: EntityType) -> Result<usize> {
    let spec = spec_for(ty);
    // Ordered by id so a run is reproducible and a diff of two runs is empty.
    // Ids are ULIDs, so this is also creation order.
    let sql = format!("{} ORDER BY id", spec.select_from());
    let mut read = from
        .connection()
        .prepare(&sql)
        .map_err(Error::storage(format!("read every row of {}", spec.table)))?;
    let mut rows = read
        .query([])
        .map_err(Error::storage(format!("read every row of {}", spec.table)))?;

    let mut write = to
        .prepare(&sqlite_insert_stmt(&spec))
        .map_err(Error::sqlite(format!(
            "prepare an insert into {}",
            spec.table
        )))?;

    let mut n = 0usize;
    while let Some(row) = rows
        .next()
        .map_err(Error::storage(format!("read a row of {}", spec.table)))?
    {
        let entity = duck_from_row(ty, row)?;
        write
            .execute(rusqlite::params_from_iter(sqlite_insert_params(&entity)))
            .map_err(Error::sqlite(format!(
                "copy {} into {}",
                entity.id(),
                spec.table
            )))?;
        n += 1;
    }
    Ok(n)
}

/// Copy the edges, verbatim.
///
/// **Nothing is normalised on the way in.** Only `blocks` is ever stored;
/// `depends_on` is swapped to `blocks` by the domain layer at the moment an edge
/// is created. Re-normalising rows that have already been through that would
/// swap them a second time and invert the graph — and an inverted traversal
/// returns an empty result that is indistinguishable from "nothing is linked
/// here", so it would fail silently, plausibly, and in the direction that makes
/// the product look calm.
fn copy_links(from: &DuckStore, to: &rusqlite::Connection) -> Result<usize> {
    const COLS: &str = "id, project_id, from_id, from_type, to_id, to_type, rel, anchor, note, \
                        created_at, updated_at, version, created_by, updated_by, session_id, \
                        surface, archived_at";

    let mut read = from
        .connection()
        .prepare(&format!("SELECT {COLS} FROM links ORDER BY id"))
        .map_err(Error::storage("read every link"))?;
    let mut rows = read.query([]).map_err(Error::storage("read every link"))?;

    let mut write = to
        .prepare(&format!(
            "INSERT INTO links ({COLS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)"
        ))
        .map_err(Error::sqlite("prepare an insert into links"))?;

    let mut n = 0usize;
    while let Some(row) = rows.next().map_err(Error::storage("read a link row"))? {
        let params = vec![
            text(row, "links", "id")?,
            opt_text(row, "links", "project_id")?,
            text(row, "links", "from_id")?,
            text(row, "links", "from_type")?,
            text(row, "links", "to_id")?,
            text(row, "links", "to_type")?,
            text(row, "links", "rel")?,
            text(row, "links", "anchor")?,
            opt_text(row, "links", "note")?,
            stamp(row, "links", "created_at")?,
            stamp(row, "links", "updated_at")?,
            int(row, "links", "version")?,
            text(row, "links", "created_by")?,
            text(row, "links", "updated_by")?,
            opt_text(row, "links", "session_id")?,
            opt_text(row, "links", "surface")?,
            opt_stamp(row, "links", "archived_at")?,
        ];
        write
            .execute(rusqlite::params_from_iter(params))
            .map_err(Error::sqlite("copy a link"))?;
        n += 1;
    }
    Ok(n)
}

/// Copy the note streams.
fn copy_notes(from: &DuckStore, to: &rusqlite::Connection) -> Result<usize> {
    const COLS: &str = "id, entity_id, entity_type, project_id, body, author, session_id, \
                        surface, created_at, archived_at";

    let mut read = from
        .connection()
        .prepare(&format!("SELECT {COLS} FROM notes ORDER BY id"))
        .map_err(Error::storage("read every note"))?;
    let mut rows = read.query([]).map_err(Error::storage("read every note"))?;

    let mut write = to
        .prepare(&format!(
            "INSERT INTO notes ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ))
        .map_err(Error::sqlite("prepare an insert into notes"))?;

    let mut n = 0usize;
    while let Some(row) = rows.next().map_err(Error::storage("read a note row"))? {
        let params = vec![
            text(row, "notes", "id")?,
            text(row, "notes", "entity_id")?,
            text(row, "notes", "entity_type")?,
            opt_text(row, "notes", "project_id")?,
            text(row, "notes", "body")?,
            text(row, "notes", "author")?,
            opt_text(row, "notes", "session_id")?,
            opt_text(row, "notes", "surface")?,
            stamp(row, "notes", "created_at")?,
            opt_stamp(row, "notes", "archived_at")?,
        ];
        write
            .execute(rusqlite::params_from_iter(params))
            .map_err(Error::sqlite("copy a note"))?;
        n += 1;
    }
    Ok(n)
}

/// Copy the event log, in the order the old store reads it.
///
/// **The order is the contract.** `seq` in the new store is assigned by
/// insertion, and it is what `keel_activity` pages from; the old store's cursor
/// is the event's ULID, and it reads ascending. Inserting in `id` order is
/// therefore what keeps a cursor monotonic across the move. Any other order
/// would leave the feed showing history out of sequence, with nothing looking
/// broken.
///
/// Three columns are renamed rather than dropped: `action` becomes `op`,
/// `created_at` becomes `at`, and a NULL `summary` becomes the empty string the
/// new column's `NOT NULL DEFAULT ''` asks for. `summary` in particular is not
/// derivable from the columns beside it — it is the sentence written at the
/// moment of the write, and the changelog and the activity feed both read it —
/// so a copy that let it go would make every migrated event render blank.
fn copy_events(from: &DuckStore, to: &rusqlite::Connection) -> Result<usize> {
    let mut read = from
        .connection()
        .prepare(
            "SELECT id, project_id, entity_id, entity_type, field, action, \
             CAST(before AS VARCHAR) AS before, CAST(after AS VARCHAR) AS after, \
             COALESCE(summary, '') AS summary, CAST(meta AS VARCHAR) AS meta, \
             actor, session_id, surface, created_at \
             FROM events ORDER BY id ASC",
        )
        .map_err(Error::storage("read every event"))?;
    let mut rows = read.query([]).map_err(Error::storage("read every event"))?;

    let mut write = to
        .prepare(
            "INSERT INTO events \
             (id, project_id, entity_id, entity_type, field, op, before, after, summary, \
              meta, actor, session_id, surface, at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        )
        .map_err(Error::sqlite("prepare an insert into events"))?;

    let mut n = 0usize;
    while let Some(row) = rows.next().map_err(Error::storage("read an event row"))? {
        let params = vec![
            text(row, "events", "id")?,
            opt_text(row, "events", "project_id")?,
            text(row, "events", "entity_id")?,
            text(row, "events", "entity_type")?,
            opt_text(row, "events", "field")?,
            text(row, "events", "action")?,
            opt_text(row, "events", "before")?,
            opt_text(row, "events", "after")?,
            text(row, "events", "summary")?,
            opt_text(row, "events", "meta")?,
            text(row, "events", "actor")?,
            opt_text(row, "events", "session_id")?,
            opt_text(row, "events", "surface")?,
            stamp(row, "events", "created_at")?,
        ];
        write
            .execute(rusqlite::params_from_iter(params))
            .map_err(Error::sqlite("copy an event"))?;
        n += 1;
    }
    Ok(n)
}

/// Copy the prose revisions out of Lance and into an ordinary table.
///
/// The embedding is converted rather than dropped. It is a `FLOAT[384]` on one
/// side and a raw little-endian f32 blob on the other, which is a change of
/// container and not of value — and re-embedding the whole corpus to recover
/// something that copies exactly would be a slow way to get the same numbers.
fn copy_documents(from: &DuckStore, to: &rusqlite::Connection) -> Result<usize> {
    const COLS: &str = "doc_id, entity_type, entity_id, project_id, version, parent_version, \
                        title, body, body_hash, media_ref, status, author, session_id, surface, \
                        created_at, embedding, embedding_model, embedding_version";

    let mut read = from
        .connection()
        .prepare(&format!(
            "SELECT {COLS} FROM lancedb.documents ORDER BY entity_id, version"
        ))
        .map_err(Error::storage("read every document revision"))?;
    let mut rows = read
        .query([])
        .map_err(Error::storage("read every document revision"))?;

    let mut write = to
        .prepare(&format!(
            "INSERT INTO documents ({COLS}) VALUES \
             (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)"
        ))
        .map_err(Error::sqlite("prepare an insert into documents"))?;

    let mut n = 0usize;
    while let Some(row) = rows.next().map_err(Error::storage("read a document row"))? {
        let embedding = match embedding_bytes(row)? {
            Some(bytes) => SqlValue::Blob(bytes),
            None => SqlValue::Null,
        };
        let params = vec![
            text(row, "documents", "doc_id")?,
            text(row, "documents", "entity_type")?,
            text(row, "documents", "entity_id")?,
            opt_text(row, "documents", "project_id")?,
            int(row, "documents", "version")?,
            opt_int(row, "documents", "parent_version")?,
            text(row, "documents", "title")?,
            text(row, "documents", "body")?,
            text(row, "documents", "body_hash")?,
            opt_text(row, "documents", "media_ref")?,
            text(row, "documents", "status")?,
            text(row, "documents", "author")?,
            opt_text(row, "documents", "session_id")?,
            opt_text(row, "documents", "surface")?,
            stamp(row, "documents", "created_at")?,
            embedding,
            opt_text(row, "documents", "embedding_model")?,
            opt_int(row, "documents", "embedding_version")?,
        ];
        write
            .execute(rusqlite::params_from_iter(params))
            .map_err(Error::sqlite("copy a document revision"))?;
        n += 1;
    }
    Ok(n)
}

/// Copy the blobs, bytes and all.
fn copy_blobs(from: &DuckStore, to: &rusqlite::Connection) -> Result<usize> {
    const COLS: &str = "blob_id, entity_id, project_id, media_type, byte_length, sha256, \
                        bytes, created_at";

    let mut read = from
        .connection()
        .prepare(&format!(
            "SELECT {COLS} FROM lancedb.blobs ORDER BY blob_id"
        ))
        .map_err(Error::storage("read every blob"))?;
    let mut rows = read.query([]).map_err(Error::storage("read every blob"))?;

    let mut write = to
        .prepare(&format!(
            "INSERT INTO blobs ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"
        ))
        .map_err(Error::sqlite("prepare an insert into blobs"))?;

    let mut n = 0usize;
    while let Some(row) = rows.next().map_err(Error::storage("read a blob row"))? {
        let bytes: Vec<u8> = row
            .get("bytes")
            .map_err(Error::storage("read column `bytes` of `blobs`"))?;
        let length: i64 = row
            .get("byte_length")
            .map_err(Error::storage("read column `byte_length` of `blobs`"))?;
        let params = vec![
            text(row, "blobs", "blob_id")?,
            opt_text(row, "blobs", "entity_id")?,
            opt_text(row, "blobs", "project_id")?,
            text(row, "blobs", "media_type")?,
            SqlValue::Integer(length),
            text(row, "blobs", "sha256")?,
            SqlValue::Blob(bytes),
            stamp(row, "blobs", "created_at")?,
        ];
        write
            .execute(rusqlite::params_from_iter(params))
            .map_err(Error::sqlite("copy a blob"))?;
        n += 1;
    }
    Ok(n)
}

// --- The comparison ------------------------------------------------------

/// Compare every revision's content hash, recomputed from what landed.
fn compare_documents(
    from: &DuckStore,
    to: &SqliteStore,
    report: &mut VerificationReport,
) -> Result<()> {
    // (entity_id, version) -> the hash the old store recorded.
    let mut old: BTreeMap<(String, i32), String> = BTreeMap::new();
    let mut stmt = from
        .connection()
        .prepare("SELECT entity_id, version, body_hash FROM lancedb.documents")
        .map_err(Error::storage("read the document hashes"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::storage("read the document hashes"))?;
    while let Some(row) = rows
        .next()
        .map_err(Error::storage("read a document hash row"))?
    {
        let e = |c: &'static str| Error::storage(format!("read column `{c}` of `documents`"));
        let entity_id: String = row.get("entity_id").map_err(e("entity_id"))?;
        let version: i32 = row.get("version").map_err(e("version"))?;
        let hash: String = row.get("body_hash").map_err(e("body_hash"))?;
        old.insert((entity_id, version), hash);
    }

    let mut stmt = to
        .connection()
        .prepare("SELECT entity_id, version, title, body, body_hash FROM documents")
        .map_err(Error::sqlite("read the migrated documents"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::sqlite("read the migrated documents"))?;
    while let Some(row) = rows
        .next()
        .map_err(Error::sqlite("read a migrated document"))?
    {
        let e = |c: &'static str| Error::sqlite(format!("read column `{c}` of `documents`"));
        let entity_id: String = row.get("entity_id").map_err(e("entity_id"))?;
        let version: i32 = row.get("version").map_err(e("version"))?;
        let title: String = row.get("title").map_err(e("title"))?;
        let body: String = row.get("body").map_err(e("body"))?;
        let stored: String = row.get("body_hash").map_err(e("body_hash"))?;
        let label = format!("{entity_id} v{version}");

        // Recomputed from the prose that actually arrived. Comparing the two
        // stored hashes would only prove that one TEXT column copied.
        let recomputed = body_hash(&title, &body);
        report.documents_hashed += 1;

        match old.remove(&(entity_id, version)) {
            Some(before) if before == recomputed => {}
            Some(before) => report.differences.push(Difference {
                table: "documents".to_owned(),
                id: Some(label),
                what: "content hash".to_owned(),
                in_old: before,
                in_new: format!("{recomputed} (recomputed from the migrated title and body)"),
            }),
            None => report.differences.push(Difference {
                table: "documents".to_owned(),
                id: Some(label),
                what: "revision".to_owned(),
                in_old: "no such revision".to_owned(),
                in_new: format!("body_hash {stored}"),
            }),
        }
    }

    for ((entity_id, version), hash) in old {
        report.differences.push(Difference {
            table: "documents".to_owned(),
            id: Some(format!("{entity_id} v{version}")),
            what: "revision".to_owned(),
            in_old: format!("body_hash {hash}"),
            in_new: "missing".to_owned(),
        });
    }
    Ok(())
}

/// Compare every blob's bytes by hashing what landed.
fn compare_blobs(
    from: &DuckStore,
    to: &SqliteStore,
    report: &mut VerificationReport,
) -> Result<()> {
    let mut old: BTreeMap<String, (String, i64)> = BTreeMap::new();
    let mut stmt = from
        .connection()
        .prepare("SELECT blob_id, sha256, byte_length FROM lancedb.blobs")
        .map_err(Error::storage("read the blob hashes"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::storage("read the blob hashes"))?;
    while let Some(row) = rows
        .next()
        .map_err(Error::storage("read a blob hash row"))?
    {
        let e = |c: &'static str| Error::storage(format!("read column `{c}` of `blobs`"));
        let blob_id: String = row.get("blob_id").map_err(e("blob_id"))?;
        let sha: String = row.get("sha256").map_err(e("sha256"))?;
        let length: i64 = row.get("byte_length").map_err(e("byte_length"))?;
        old.insert(blob_id, (sha, length));
    }

    let mut stmt = to
        .connection()
        .prepare("SELECT blob_id, bytes FROM blobs")
        .map_err(Error::sqlite("read the migrated blobs"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::sqlite("read the migrated blobs"))?;
    while let Some(row) = rows.next().map_err(Error::sqlite("read a migrated blob"))? {
        let e = |c: &'static str| Error::sqlite(format!("read column `{c}` of `blobs`"));
        let blob_id: String = row.get("blob_id").map_err(e("blob_id"))?;
        let bytes: Vec<u8> = row.get("bytes").map_err(e("bytes"))?;
        report.blobs_hashed += 1;

        let sha = sha256_hex(&bytes);
        let length = bytes.len() as i64;
        match old.remove(&blob_id) {
            Some((old_sha, old_length)) => {
                if old_sha != sha {
                    report.differences.push(Difference {
                        table: "blobs".to_owned(),
                        id: Some(blob_id.clone()),
                        what: "sha256".to_owned(),
                        in_old: old_sha,
                        in_new: format!("{sha} (recomputed from the migrated bytes)"),
                    });
                }
                if old_length != length {
                    report.differences.push(Difference {
                        table: "blobs".to_owned(),
                        id: Some(blob_id),
                        what: "byte_length".to_owned(),
                        in_old: format!("{old_length} bytes"),
                        in_new: format!("{length} bytes"),
                    });
                }
            }
            None => report.differences.push(Difference {
                table: "blobs".to_owned(),
                id: Some(blob_id),
                what: "blob".to_owned(),
                in_old: "no such blob".to_owned(),
                in_new: format!("{length} bytes"),
            }),
        }
    }

    for (blob_id, (sha, length)) in old {
        report.differences.push(Difference {
            table: "blobs".to_owned(),
            id: Some(blob_id),
            what: "blob".to_owned(),
            in_old: format!("{length} bytes, sha256 {sha}"),
            in_new: "missing".to_owned(),
        });
    }
    Ok(())
}

/// Compare each revision's vector by presence and width.
///
/// A vector that arrived as NULL, or at half its width, is invisible to every
/// other check here: the row is present, the prose hashes, and search merely
/// gets quietly worse.
fn compare_embeddings(
    from: &DuckStore,
    to: &SqliteStore,
    report: &mut VerificationReport,
) -> Result<()> {
    /// How a width reads in a difference. `None` is a row with no vector.
    fn describe(dim: Option<i64>) -> String {
        match dim {
            Some(n) => format!("{n} floats"),
            None => "no vector".to_owned(),
        }
    }

    let mut old: BTreeMap<String, Option<i64>> = BTreeMap::new();
    let mut stmt = from
        .connection()
        .prepare("SELECT doc_id, len(embedding) AS dim FROM lancedb.documents")
        .map_err(Error::storage("read the document embedding widths"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::storage("read the document embedding widths"))?;
    while let Some(row) = rows
        .next()
        .map_err(Error::storage("read an embedding width"))?
    {
        let e = |c: &'static str| Error::storage(format!("read column `{c}` of `documents`"));
        let doc_id: String = row.get("doc_id").map_err(e("doc_id"))?;
        let dim: Option<i64> = row.get("dim").map_err(e("dim"))?;
        old.insert(doc_id, dim);
    }

    // Four bytes to the float: the column is a raw little-endian f32 blob.
    let mut stmt = to
        .connection()
        .prepare("SELECT doc_id, length(embedding) / 4 AS dim FROM documents")
        .map_err(Error::sqlite("read the migrated embedding widths"))?;
    let mut rows = stmt
        .query([])
        .map_err(Error::sqlite("read the migrated embedding widths"))?;
    while let Some(row) = rows
        .next()
        .map_err(Error::sqlite("read a migrated embedding width"))?
    {
        let e = |c: &'static str| Error::sqlite(format!("read column `{c}` of `documents`"));
        let doc_id: String = row.get("doc_id").map_err(e("doc_id"))?;
        let dim: Option<i64> = row.get("dim").map_err(e("dim"))?;
        // A document the row comparison has already reported as missing is not
        // reported a second time here.
        if let Some(before) = old.remove(&doc_id)
            && before != dim
        {
            report.differences.push(Difference {
                table: "documents".to_owned(),
                id: Some(doc_id),
                what: "embedding".to_owned(),
                in_old: describe(before),
                in_new: describe(dim),
            });
        }
    }
    Ok(())
}

// --- Reading one column at a time ----------------------------------------

/// Wrap a DuckDB read failure with the column that caused it.
fn read_err(table: &str, column: &str) -> impl FnOnce(duckdb::Error) -> Error {
    let context = format!("read column `{column}` of `{table}`");
    move |source| Error::Storage { context, source }
}

/// A required text column, as a SQLite parameter.
fn text(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    Ok(SqlValue::Text(
        row.get::<_, String>(col).map_err(read_err(table, col))?,
    ))
}

/// An optional text column.
fn opt_text(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    Ok(row
        .get::<_, Option<String>>(col)
        .map_err(read_err(table, col))?
        .map_or(SqlValue::Null, SqlValue::Text))
}

/// A required integer column.
fn int(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    Ok(SqlValue::Integer(i64::from(
        row.get::<_, i32>(col).map_err(read_err(table, col))?,
    )))
}

/// An optional integer column.
fn opt_int(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    Ok(row
        .get::<_, Option<i32>>(col)
        .map_err(read_err(table, col))?
        .map_or(SqlValue::Null, |v| SqlValue::Integer(i64::from(v))))
}

/// A required timestamp column, in the format the new store sorts correctly.
fn stamp(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    let at: DateTime<Utc> = row
        .get::<_, DateTime<Utc>>(col)
        .map_err(read_err(table, col))?;
    Ok(SqlValue::Text(as_stored_text(at)))
}

/// An optional timestamp column.
fn opt_stamp(row: &duckdb::Row<'_>, table: &str, col: &str) -> Result<SqlValue> {
    Ok(row
        .get::<_, Option<DateTime<Utc>>>(col)
        .map_err(read_err(table, col))?
        .map_or(SqlValue::Null, |at| SqlValue::Text(as_stored_text(at))))
}

/// A timestamp as the new store spells it.
fn as_stored_text(at: DateTime<Utc>) -> String {
    at.format(TIMESTAMP_FORMAT).to_string()
}

/// Turn a Lance `FLOAT[384]` into the raw little-endian bytes SQLite holds.
///
/// A row with no vector stays a row with no vector — this is a copy, and
/// inventing zeroes for it would make an unembedded document look embedded to
/// the re-embed pass that is meant to find it.
fn embedding_bytes(row: &duckdb::Row<'_>) -> Result<Option<Vec<u8>>> {
    let value: DuckValue = row
        .get::<_, DuckValue>("embedding")
        .map_err(read_err("documents", "embedding"))?;

    let items = match value {
        DuckValue::Null => return Ok(None),
        DuckValue::Array(items) | DuckValue::List(items) => items,
        other => {
            return Err(Error::Invariant {
                operation: "copy a document's embedding".to_owned(),
                problem: format!(
                    "`embedding` held {other:?}, which is not a list of floats. The column is a \
                     FLOAT[384] in Lance and the migration copies it as raw f32 bytes"
                ),
            });
        }
    };

    let mut bytes = Vec::with_capacity(items.len() * 4);
    for item in items {
        match item {
            DuckValue::Float(f) => bytes.extend_from_slice(&f.to_le_bytes()),
            other => {
                return Err(Error::Invariant {
                    operation: "copy a document's embedding".to_owned(),
                    problem: format!("an element of `embedding` held {other:?}, not a float"),
                });
            }
        }
    }
    Ok(Some(bytes))
}

/// Count one table in the old store.
fn count_duck(from: &DuckStore, table: &str) -> Result<i64> {
    // `table` comes from `tables()`, which is built from this crate's own enum
    // and three literals. No caller can influence it.
    from.connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .map_err(Error::storage(format!("count the rows in {table}")))
}

/// Count one table in the new store.
fn count_sqlite(to: &SqliteStore, table: &str) -> Result<i64> {
    to.connection()
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
        .map_err(Error::sqlite(format!("count the rows in {table}")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::{
        Actor, Blob, Document, DocumentStore, Entity, EntityId, EntityStore, HashEmbedder, NewLink,
        NewNote, Project, Provenance, Relation, Spec, Surface, Task,
    };
    use std::sync::Arc;

    /// A store, its temporary directory, and the ids a test needs to assert on.
    struct Seeded {
        store: DuckStore,
        _dir: tempfile::TempDir,
        blocker: EntityId,
        blocked: EntityId,
        archived: EntityId,
        spec: EntityId,
    }

    fn provenance() -> Provenance {
        Provenance {
            actor: Actor::Claude,
            session_id: Some("ses_migrate_test".to_owned()),
            surface: Some(Surface::Code),
        }
    }

    /// A small store with one of everything the migration has to carry.
    ///
    /// Deliberately not the full fixture: most of these tests are about one
    /// property each, and paying for two hundred entities to assert that a
    /// number survived would make the suite slow enough to stop being run.
    fn seeded() -> Seeded {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DuckStore::open(dir.path())
            .unwrap()
            .with_embedder(Arc::new(HashEmbedder::new()));
        let p = provenance();

        let project = id(store
            .create(Project::new("keel", "Keel").into(), &p)
            .unwrap());

        let blocker = id(store
            .create(
                Task::new(
                    project.clone(),
                    "Write the migration",
                    "The old store has to be readable into the new one, verbatim.",
                )
                .into(),
                &p,
            )
            .unwrap());
        let blocked = id(store
            .create(
                Task::new(
                    project.clone(),
                    "Switch the daemon over",
                    "Nothing points at the new store until the migration verifies.",
                )
                .into(),
                &p,
            )
            .unwrap());
        let archived = id(store
            .create(
                Task::new(
                    project.clone(),
                    "An abandoned idea",
                    "Archived, and it still has to survive the move.",
                )
                .into(),
                &p,
            )
            .unwrap());

        // The one direction that must not be touched: `blocks` is stored as it
        // is written.
        store
            .link(
                NewLink::new(blocker.clone(), Relation::Blocks, blocked.clone()),
                &p,
            )
            .unwrap();

        store
            .add_note(
                NewNote::new(
                    blocker.clone(),
                    "The copy bypasses the domain layer on purpose.",
                    Actor::Claude,
                )
                .in_session("ses_migrate_test")
                .from_surface(Surface::Code),
                &p,
            )
            .unwrap();

        // Prose lives on a spec, not on a task: only the five prose types write
        // to the documents dataset at all.
        let spec = id(store
            .create(
                Spec::new(project.clone(), "One database, not three").into(),
                &p,
            )
            .unwrap());
        let doc = Document::first(
            EntityType::Spec,
            spec.clone(),
            Some(project.clone()),
            "One database, not three",
            "The migration reads the DuckDB store and writes a SQLite one, and proves \
             the two hold the same rows.",
            Actor::Claude,
            Utc::now(),
        )
        .unwrap();
        store.write_revision(doc).unwrap();

        let bytes: Vec<u8> = (0u32..4096).map(|i| (i % 251) as u8).collect();
        store
            .put_blob(
                Blob::new(bytes, "image/png", Utc::now())
                    .owned_by(blocked.clone(), project.clone()),
            )
            .unwrap();

        // Archived last, so its version is known and the archive succeeds.
        let current = store.get(&archived).unwrap().unwrap();
        store
            .archive(&archived, current.audit().version, &p)
            .unwrap();

        Seeded {
            store,
            _dir: dir,
            blocker,
            blocked,
            archived,
            spec,
        }
    }

    fn id(created: crate::Created) -> EntityId {
        created.entity.id().clone()
    }

    /// Migrate a seeded store into a fresh in-memory one.
    fn migrated(from: &DuckStore) -> (SqliteStore, MigrationReport) {
        let mut to = SqliteStore::in_memory().unwrap();
        let report = migrate(from, &mut to).unwrap();
        (to, report)
    }

    fn one_string(store: &SqliteStore, sql: &str) -> String {
        store.connection().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    fn one_int(store: &SqliteStore, sql: &str) -> i64 {
        store.connection().query_row(sql, [], |r| r.get(0)).unwrap()
    }

    // --- The round trip --------------------------------------------------

    /// The whole claim, against the corpus the rest of the project is measured
    /// on: two hundred entities, every type, every relation, prose revisions
    /// and embeddings.
    #[test]
    fn the_fixture_migrates_and_verifies_clean() {
        let dir = tempfile::tempdir().unwrap();
        let mut from = DuckStore::open(dir.path())
            .unwrap()
            .with_embedder(Arc::new(HashEmbedder::new()));
        let summary = crate::fixture::load(&mut from).unwrap();

        let (to, report) = migrated(&from);
        assert!(report.rows["tasks"] > 0 && report.rows["specs"] > 0);
        assert!(
            report.total_rows() > summary.total_entities(),
            "the copy should carry the links, events and revisions too, not only {} entities",
            summary.total_entities()
        );

        let verification = verify(&from, &to).unwrap();
        assert!(
            verification.is_clean(),
            "the two stores disagree:\n{verification}"
        );
        assert!(
            verification.documents_hashed >= summary.revisions,
            "every revision should have been hashed; {} of {} were",
            verification.documents_hashed,
            summary.revisions
        );
    }

    /// Vectors copy rather than being regenerated, and they arrive at their
    /// full width. A vector that landed as NULL passes every count.
    #[test]
    fn embeddings_arrive_at_their_declared_width() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let width = one_int(
            &to,
            "SELECT length(embedding) / 4 FROM documents WHERE embedding IS NOT NULL LIMIT 1",
        );
        assert_eq!(width as usize, crate::EMBEDDING_DIM);

        let missing = one_int(
            &to,
            "SELECT count(*) FROM documents WHERE embedding IS NULL",
        );
        assert_eq!(missing, 0, "a document lost its vector in the copy");
    }

    // --- The things that fail silently -----------------------------------

    /// Soft delete means nothing is ever gone. A copy that filtered on
    /// `archived_at IS NULL` would lose the audit trail while every live-row
    /// count still matched — which is why this has its own test and not a line
    /// in another one.
    #[test]
    fn an_archived_row_survives_with_its_archived_at() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let total = one_int(&to, "SELECT count(*) FROM tasks");
        assert_eq!(total, 3, "the archived task should have been copied too");

        let archived: i64 = to
            .connection()
            .query_row(
                "SELECT count(*) FROM tasks WHERE id = ?1 AND archived_at IS NOT NULL",
                [seed.archived.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 1, "the row survived but lost its archived_at");
    }

    /// Only `blocks` is ever stored, and the domain layer has already swapped
    /// any `depends_on` into it. Re-normalising on the way in would swap it
    /// again and invert the graph, and an inverted traversal returns an empty
    /// result that reads exactly like "nothing is linked here".
    #[test]
    fn a_blocks_link_keeps_its_direction() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let from_id = one_string(&to, "SELECT from_id FROM links WHERE rel = 'blocks'");
        let to_id = one_string(&to, "SELECT to_id FROM links WHERE rel = 'blocks'");

        assert_eq!(from_id, seed.blocker.as_str(), "the edge was inverted");
        assert_eq!(to_id, seed.blocked.as_str(), "the edge was inverted");

        // And against what the old store literally holds, so the assertion does
        // not depend on this test's own idea of which way round `blocks` goes.
        let old_from: String = seed
            .store
            .connection()
            .query_row("SELECT from_id FROM links WHERE rel = 'blocks'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(from_id, old_from);
    }

    /// `KEEL-42` means the same task forever, and it is cited in prose that
    /// nothing rewrites. A migration that renumbered would break every
    /// reference at once.
    #[test]
    fn task_numbers_are_preserved() {
        let seed = seeded();

        let mut expected: Vec<(String, i64)> = Vec::new();
        let mut stmt = seed
            .store
            .connection()
            .prepare("SELECT id, number FROM tasks ORDER BY id")
            .unwrap();
        let mut rows = stmt.query([]).unwrap();
        while let Some(row) = rows.next().unwrap() {
            expected.push((
                row.get(0).unwrap(),
                i64::from(row.get::<_, i32>(1).unwrap()),
            ));
        }
        assert_eq!(expected.len(), 3);
        assert!(
            expected.iter().any(|(_, n)| *n > 0),
            "the seed has no numbers"
        );

        let (to, _) = migrated(&seed.store);
        let mut stmt = to
            .connection()
            .prepare("SELECT id, number FROM tasks ORDER BY id")
            .unwrap();
        let actual: Vec<(String, i64)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        assert_eq!(actual, expected, "a task was renumbered by the migration");
    }

    /// The cursor has to stay monotonic across the move. `seq` is assigned by
    /// insertion in the new store, and the old store's feed reads by event id
    /// ascending — so inserting in that order is what keeps "catch me up" from
    /// paging over a gap.
    #[test]
    fn events_keep_their_order() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let mut stmt = seed
            .store
            .connection()
            .prepare("SELECT id FROM events ORDER BY id ASC")
            .unwrap();
        let expected: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert!(expected.len() > 5, "the seed produced too few events");

        let mut stmt = to
            .connection()
            .prepare("SELECT id FROM events ORDER BY seq ASC")
            .unwrap();
        let actual: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        assert_eq!(actual, expected, "the event log came out in a new order");
    }

    /// `summary` is the sentence a person reads, written at the moment of the
    /// write and reconstructable from nothing else once the row it described
    /// has moved on. Dropping it would leave the changelog and the activity
    /// feed rendering migrated history as blank lines, plausibly and silently.
    #[test]
    fn events_keep_their_summary_and_meta() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let mut stmt = seed
            .store
            .connection()
            .prepare("SELECT id, COALESCE(summary, '') FROM events ORDER BY id")
            .unwrap();
        let expected: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();
        assert!(
            expected.iter().any(|(_, s)| !s.is_empty()),
            "the seed wrote no event summaries, so this test proves nothing"
        );

        let mut stmt = to
            .connection()
            .prepare("SELECT id, summary FROM events ORDER BY id")
            .unwrap();
        let actual: Vec<(String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap();

        assert_eq!(actual, expected, "an event lost the sentence describing it");
    }

    /// A row copy bypasses the write path, so the question is whether the
    /// keyword index comes with it. It does — the index is maintained by
    /// triggers on the tables themselves — and asserting it here is what keeps
    /// a migrated store from being one where search silently finds nothing.
    #[test]
    fn the_keyword_index_comes_with_the_rows() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        let indexed = one_int(&to, "SELECT count(*) FROM fts_entities");
        assert!(indexed > 0, "the migrated store has an empty keyword index");

        let hits: i64 = to
            .connection()
            .query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH 'migration'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(hits > 0, "a migrated row is not findable by keyword");

        // Archived rows are kept out of the index by the same triggers, which
        // is the store's own behaviour and not something the copy overrides.
        let archived: i64 = to
            .connection()
            .query_row(
                "SELECT count(*) FROM fts_source WHERE entity_id = ?1",
                [seed.archived.as_str()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(archived, 0, "an archived row was put into the search index");
    }

    /// The migration must not touch the store it is reading.
    #[test]
    fn the_old_store_is_left_alone() {
        let seed = seeded();
        let before: Vec<i64> = tables()
            .iter()
            .map(|(_, old)| count_duck(&seed.store, old).unwrap())
            .collect();

        let (_to, _report) = migrated(&seed.store);

        let after: Vec<i64> = tables()
            .iter()
            .map(|(_, old)| count_duck(&seed.store, old).unwrap())
            .collect();
        assert_eq!(before, after, "the migration changed the old store");
    }

    // --- The refusals ----------------------------------------------------

    /// A second run into the same file would duplicate the event log, and there
    /// is no key on which the two runs could be reconciled.
    #[test]
    fn migrating_into_a_populated_target_is_refused() {
        let seed = seeded();
        let mut to = SqliteStore::in_memory().unwrap();
        migrate(&seed.store, &mut to).unwrap();

        let err = migrate(&seed.store, &mut to).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("tasks"),
            "the refusal should name what is already there: {message}"
        );
        assert!(
            message.contains("fresh file"),
            "the refusal should say what to do instead: {message}"
        );
    }

    // --- The verification, failing ---------------------------------------

    /// A check that only ever passes is worse than none. Deleting a row from
    /// the target behind the migration's back has to be caught, and the report
    /// has to say which table and which row.
    #[test]
    fn verification_names_a_row_deleted_from_the_target() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);
        assert!(verify(&seed.store, &to).unwrap().is_clean());

        to.connection()
            .execute("DELETE FROM tasks WHERE id = ?1", [seed.blocked.as_str()])
            .unwrap();

        let report = verify(&seed.store, &to).unwrap();
        assert!(!report.is_clean(), "a deleted row went unnoticed");
        let text = report.to_string();
        assert!(
            text.contains("tasks"),
            "the report should name the table: {text}"
        );
        assert!(
            text.contains("3 rows") && text.contains("2 rows"),
            "the report should say what each side held: {text}"
        );
    }

    /// The document check has to be about the prose, not about a hash column
    /// copying itself. Editing the body in the target — and leaving the stored
    /// hash alone — is the corruption a naive comparison would miss entirely.
    #[test]
    fn verification_catches_a_body_that_changed_but_kept_its_hash() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        to.connection()
            .execute("UPDATE documents SET body = 'something else entirely'", [])
            .unwrap();

        let report = verify(&seed.store, &to).unwrap();
        assert!(!report.is_clean(), "an edited body went unnoticed");
        let text = report.to_string();
        assert!(
            text.contains("content hash"),
            "the report should say what it compared: {text}"
        );
        assert!(
            text.contains(seed.spec.as_str()),
            "the report should name the revision: {text}"
        );
    }

    /// A blob whose bytes were replaced keeps its row, its length and its
    /// stored hash. Only re-hashing what landed finds it.
    #[test]
    fn verification_catches_corrupted_blob_bytes() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        to.connection()
            .execute("UPDATE blobs SET bytes = zeroblob(length(bytes))", [])
            .unwrap();

        let report = verify(&seed.store, &to).unwrap();
        assert!(!report.is_clean(), "corrupted bytes went unnoticed");
        assert!(
            report.to_string().contains("sha256"),
            "the report should name the hash that disagreed: {report}"
        );
    }

    /// A vector that copied as NULL leaves the row present, the prose intact
    /// and the counts equal.
    #[test]
    fn verification_catches_a_dropped_embedding() {
        let seed = seeded();
        let (to, _) = migrated(&seed.store);

        to.connection()
            .execute("UPDATE documents SET embedding = NULL", [])
            .unwrap();

        let report = verify(&seed.store, &to).unwrap();
        assert!(!report.is_clean(), "a dropped vector went unnoticed");
        let text = report.to_string();
        assert!(
            text.contains("no vector"),
            "the report should say the vector is gone: {text}"
        );
    }

    // --- The things that would be silent ---------------------------------

    /// The report has to account for every table, not only the ones that had
    /// rows. A table missing from it is a table nobody would notice was never
    /// copied, because a count of zero and an absence read the same.
    #[test]
    fn the_report_accounts_for_every_table() {
        let seed = seeded();
        let (_to, report) = migrated(&seed.store);

        for (new_name, _) in tables() {
            assert!(
                report.rows.contains_key(&new_name),
                "the report does not mention {new_name}:\n{report}"
            );
        }
        assert_eq!(report.rows["tasks"], 3);
        assert_eq!(report.rows["links"], 1);
        assert_eq!(report.rows["blobs"], 1);
    }

    /// The format constant above is a copy of one that is private to the SQLite
    /// store. This is what stops the two drifting: it writes a row through the
    /// store's own mapping, reads the raw text back, and compares.
    #[test]
    fn timestamps_match_what_the_store_itself_writes() {
        let store = SqliteStore::in_memory().unwrap();
        let at = DateTime::from_timestamp_micros(1_775_000_000_500_000).unwrap();

        let mut project = Project::new("keel", "Keel");
        project.audit.created_at = at;
        project.audit.updated_at = at;
        let entity: Entity = project.into();

        let spec = spec_for(EntityType::Project);
        store
            .connection()
            .execute(
                &sqlite_insert_stmt(&spec),
                rusqlite::params_from_iter(sqlite_insert_params(&entity)),
            )
            .unwrap();

        let stored = one_string(&store, "SELECT created_at FROM projects");
        assert_eq!(
            stored,
            as_stored_text(at),
            "the migration writes timestamps in a different format from the store"
        );
    }

    /// The list the migration works from has to cover every table the new store
    /// has rows in, or a whole table is silently never copied and nothing
    /// compares it.
    #[test]
    fn every_entity_type_is_in_the_table_list() {
        let names: Vec<String> = tables().into_iter().map(|(new, _)| new).collect();
        for ty in EntityType::ALL {
            assert!(
                names.iter().any(|n| n == spec_for(ty).table),
                "{ty} is not in the migration's table list"
            );
        }
        for extra in ["links", "notes", "events", "documents", "blobs"] {
            assert!(names.iter().any(|n| n == extra), "{extra} is missing");
        }
    }
}
