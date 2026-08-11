//! The SQLite schema: one migration, and it is the whole schema.
//!
//! # Why the numbering starts again at 1 with everything in it
//!
//! The DuckDB store this replaced carried sixteen numbered migrations because
//! it *evolved* — each one moved a database that already held rows into a shape
//! it did not have before. There was no such history to replay here. Every
//! SQLite store was created empty, at the current shape, and the rows were
//! copied in afterwards; the intermediate states those sixteen passed through
//! never existed in this file and never will.
//!
//! Replaying them would have meant maintaining sixteen translations of DDL
//! whose only purpose was to arrive somewhere this file states directly, each
//! one untestable against the state it was written for. Migration 11 of that
//! sequence backfilled decision numbers from a table of legacy `B-n` values;
//! there is no SQLite store in the world that needs that to happen.
//!
//! So migration 1 is everything. The mechanism is unchanged from the store
//! before it — forward-only, recorded in `_keel_migrations`, no `down` — and
//! migration 2 is the first change made *after* the move. The old sequence is
//! in git if anyone ever needs to know what a column used to be.
//!
//! # Why the columns are the types they are
//!
//! SQLite has five storage classes, so most of the mapping is forced. Three
//! choices are not:
//!
//! - **List columns are `TEXT` holding JSON.** There is no array type, and
//!   `rows.rs` sends and receives these columns as JSON in both directions, so
//!   the conversion lives in Rust where it can be tested without a database.
//! - **Timestamps and dates are `TEXT`, ISO 8601.** Once the column is text,
//!   every `ORDER BY created_at` in the codebase is a string comparison, and
//!   UTC ISO 8601 is the rendering that sorts lexicographically in the same
//!   order it sorts chronologically. `rows.rs` pins the exact format down to
//!   the width of the fraction, which is not cosmetic.
//! - **Booleans are `INTEGER`.** There is no boolean type; 0 and 1 is what the
//!   engine's own `true` and `false` literals compile to.
//!
//! `STRICT` is on every table. Without it SQLite accepts a string into an
//! integer column and stores it as a string, and the read that eventually fails
//! is nowhere near the write that caused it.

/// One forward-only migration.
///
/// Recorded in `_keel_migrations` once applied, and never re-run. There is no
/// `down`: undoing a shape change would have to decide what becomes of the rows
/// already written against the newer one, and no generic answer to that is safe.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Applied in ascending order. Never reused, never reordered.
    pub id: i32,
    /// What it does, for `_keel_migrations` and for `fsck` output.
    pub name: &'static str,
    /// The statements, separated by `;`. Executed as one batch.
    pub sql: &'static str,
}

/// The audit columns every entity table carries.
///
/// SPEC §3.1 writes this as `<audit>` rather than repeating it thirteen times;
/// this is that expansion. `events` deliberately does not use it — append-only
/// means there is no `updated_at`, no `version` and no `archived_at`, and a
/// table that carried them would invite an update that must never happen.
const AUDIT: &str = "
  created_at   TEXT    NOT NULL,
  updated_at   TEXT    NOT NULL,
  version      INTEGER NOT NULL DEFAULT 1,
  created_by   TEXT    NOT NULL,
  updated_by   TEXT    NOT NULL,
  session_id   TEXT,
  surface      TEXT,
  archived_at  TEXT";

/// The thirteen entity tables, plus links, events, notes, documents and blobs.
///
/// Built by a function rather than written as a literal so the audit block
/// appears once. The output is deterministic and it is only ever applied to an
/// empty database, so composing it at runtime costs nothing and removes
/// thirteen chances to mistype a column.
fn initial_schema() -> String {
    let mut sql = String::new();

    // ---- Structural ---------------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE projects (
  id              TEXT PRIMARY KEY,
  slug            TEXT UNIQUE NOT NULL,
  key             TEXT,
  name            TEXT NOT NULL,
  description     TEXT,
  status          TEXT NOT NULL DEFAULT 'active',
  repo_urls       TEXT,
  root_path       TEXT,
  status_path     TEXT,
  decisions_path  TEXT,
  milestone_noun  TEXT,
  aliases         TEXT,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX projects_idem ON projects(idempotency_key);
CREATE UNIQUE INDEX projects_key ON projects(key);

CREATE TABLE milestones (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  kind            TEXT NOT NULL DEFAULT 'milestone',
  name            TEXT NOT NULL,
  summary         TEXT,
  status          TEXT NOT NULL DEFAULT 'planned',
  target_date     TEXT,
  shipped_at      TEXT,
  version_string  TEXT,
  sort_order      INTEGER,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX milestones_idem ON milestones(project_id, idempotency_key);
CREATE INDEX milestones_project ON milestones(project_id);

CREATE TABLE tasks (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  milestone_id    TEXT,
  parent_id       TEXT,
  number          INTEGER,
  rank            REAL,
  kind            TEXT NOT NULL DEFAULT 'task',
  title           TEXT NOT NULL,
  summary         TEXT,
  body            TEXT,
  status          TEXT NOT NULL DEFAULT 'todo',
  priority        TEXT DEFAULT 'p2',
  labels          TEXT,
  external_refs   TEXT,
  claimed_by      TEXT,
  claimed_at      TEXT,
  close_reason    TEXT,
  close_message   TEXT,
  evidence        TEXT,
  closed_at       TEXT,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX tasks_idem ON tasks(project_id, idempotency_key);
CREATE UNIQUE INDEX tasks_number ON tasks(project_id, number);
CREATE INDEX tasks_project ON tasks(project_id);
CREATE INDEX tasks_milestone ON tasks(milestone_id);
CREATE INDEX tasks_parent ON tasks(parent_id);
"
    ));

    // ---- Knowledge ----------------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE specs (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'spec',
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'draft',
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX specs_idem ON specs(project_id, idempotency_key);
CREATE INDEX specs_project ON specs(project_id);

CREATE TABLE decisions (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  number              INTEGER,
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'proposed',
  decided_at          TEXT,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX decisions_idem ON decisions(project_id, idempotency_key);
CREATE UNIQUE INDEX decisions_number ON decisions(project_id, number);
CREATE INDEX decisions_project ON decisions(project_id);

CREATE TABLE questions (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'question',
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'open',
  severity            TEXT,
  resolved_at         TEXT,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX questions_idem ON questions(project_id, idempotency_key);
CREATE INDEX questions_project ON questions(project_id);

CREATE TABLE terms (
  id              TEXT PRIMARY KEY,
  project_id      TEXT,
  term            TEXT NOT NULL,
  definition      TEXT NOT NULL,
  means           TEXT,
  aliases         TEXT,
  mirror_path     TEXT,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
-- COALESCE rather than a bare column: a nullable project_id in a unique index
-- lets duplicate globals through, because SQL says NULL is distinct from NULL.
-- That would make 'the override' ambiguous (PRD Q-4).
CREATE UNIQUE INDEX terms_uniq ON terms(COALESCE(project_id, ''), term);
CREATE UNIQUE INDEX terms_idem ON terms(COALESCE(project_id, ''), idempotency_key);
"
    ));

    // ---- Inputs and surfaces ------------------------------------------
    sql.push_str(&format!(
        // `summary` rather than `title`, and it is the label `v_entities`
        // resolves for this type — a piece of feedback is what someone said,
        // and giving it a title would mean inventing one.
        "CREATE TABLE feedback (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'observation',
  summary             TEXT NOT NULL,
  source              TEXT,
  contact             TEXT,
  sentiment           TEXT,
  occurred_at         TEXT,
  triaged             INTEGER DEFAULT 0,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  idempotency_key     TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX feedback_idem ON feedback(project_id, idempotency_key);
CREATE INDEX feedback_project ON feedback(project_id);

-- `design_artifacts`, not `designs`. It has been called that since the first
-- schema this project ever had, every `TableSpec` names it, and renaming it at
-- the move would have meant translating a table name mid-migration for no gain.
CREATE TABLE design_artifacts (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  name                TEXT NOT NULL,
  state               TEXT NOT NULL DEFAULT 'concept',
  figma_ref           TEXT,
  blob_id             TEXT,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  idempotency_key     TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX design_artifacts_idem
  ON design_artifacts(project_id, idempotency_key);
CREATE INDEX design_artifacts_project ON design_artifacts(project_id);

CREATE TABLE environments (
  id               TEXT PRIMARY KEY,
  project_id       TEXT NOT NULL,
  name             TEXT NOT NULL,
  url              TEXT,
  deployed_version TEXT,
  deployed_commit  TEXT,
  status           TEXT NOT NULL DEFAULT 'unknown',
  last_deployed_at TEXT,
  idempotency_key  TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX environments_idem ON environments(project_id, idempotency_key);
CREATE INDEX environments_project ON environments(project_id);

CREATE TABLE artifacts (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  kind            TEXT NOT NULL DEFAULT 'link',
  name            TEXT NOT NULL,
  url             TEXT,
  blob_id         TEXT,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX artifacts_idem ON artifacts(project_id, idempotency_key);
CREATE INDEX artifacts_project ON artifacts(project_id);
"
    ));

    // ---- Measurement ---------------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE metrics (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  name            TEXT NOT NULL,
  unit            TEXT,
  direction       TEXT,
  target_value    REAL,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX metrics_idem ON metrics(project_id, idempotency_key);
CREATE INDEX metrics_project ON metrics(project_id);

CREATE TABLE metric_observations (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  metric_id       TEXT NOT NULL,
  value           REAL NOT NULL,
  observed_at     TEXT NOT NULL,
  note            TEXT,
  idempotency_key TEXT NOT NULL,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX metric_observations_idem
  ON metric_observations(project_id, idempotency_key);
CREATE INDEX metric_observations_metric ON metric_observations(metric_id);
"
    ));

    // ---- Edges, log and commentary --------------------------------------
    //
    // `links` stores only `blocks`; `depends_on` is swapped on the way in by
    // keel-core, and storing both is the bug the contract warns about at
    // length. Nothing here enforces that — it cannot, since both are legal
    // values — which is why it is enforced on write and audited by `fsck`.
    sql.push_str(&format!(
        "CREATE TABLE links (
  id          TEXT PRIMARY KEY,
  project_id  TEXT,
  from_id     TEXT NOT NULL,
  from_type   TEXT NOT NULL,
  to_id       TEXT NOT NULL,
  to_type     TEXT NOT NULL,
  rel         TEXT NOT NULL,
  anchor      TEXT NOT NULL DEFAULT '',
  note        TEXT,
  {AUDIT}
) STRICT;
CREATE UNIQUE INDEX links_uniq ON links(from_id, rel, to_id, anchor);
CREATE INDEX links_from ON links(from_id);
CREATE INDEX links_to ON links(to_id);

CREATE TABLE notes (
  id          TEXT PRIMARY KEY,
  entity_id   TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  project_id  TEXT,
  body        TEXT NOT NULL,
  author      TEXT NOT NULL,
  session_id  TEXT,
  surface     TEXT,
  created_at  TEXT NOT NULL,
  archived_at TEXT
) STRICT;
CREATE INDEX notes_entity ON notes(entity_id);
CREATE INDEX notes_project ON notes(project_id);
"
    ));

    // The event log is append-only and its ordering is a contract: `seq` is
    // what `keel_activity` pages from, and a cursor that could see the same row
    // twice or skip one would make "catch me up" quietly wrong.
    //
    // `AUTOINCREMENT` rather than a bare `INTEGER PRIMARY KEY`, and the
    // difference matters here. A bare rowid reuses the highest deleted value;
    // `AUTOINCREMENT` never reuses one. Nothing is ever deleted from this table,
    // so in principle they behave identically — but "in principle nothing
    // deletes from here" is exactly the kind of assumption a cursor should not
    // rest on.
    sql.push_str(
        "CREATE TABLE events (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          TEXT NOT NULL UNIQUE,
  project_id  TEXT,
  entity_id   TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  field       TEXT,
  op          TEXT NOT NULL,
  before      TEXT,
  after       TEXT,
  -- The sentence a person reads. Not derivable from the columns beside it:
  -- \"created task “Fix the board filter”\" is written at the point the write
  -- happens, and nothing downstream can reconstruct it once the row it
  -- described has moved on. `render_status` puts it in the changelog and the
  -- activity feed returns it, so an events table without this column makes
  -- both go blank — plausibly, and only for history written after the move.
  summary     TEXT NOT NULL DEFAULT '',
  -- Whatever else the write wanted to say about itself. `unlink` marks its
  -- event `{\"removed\": true}`, and without that an unlink is indistinguishable
  -- from a link in the log.
  meta        TEXT,
  actor       TEXT NOT NULL,
  session_id  TEXT,
  surface     TEXT,
  at          TEXT NOT NULL
) STRICT;
CREATE INDEX events_entity ON events(entity_id);
CREATE INDEX events_project ON events(project_id, seq);
",
    );

    // ---- Documents and blobs, which used to live in Lance ---------------
    //
    // Ordinary tables. The gain is not tidiness: a document and the row it
    // belongs to are written in one transaction, so the two can no longer
    // half-land. `fsck` used to audit that pairing across two engines, which
    // was the only way to find it broken.
    //
    // `embedding` is a raw little-endian f32 blob rather than a typed column,
    // and that is deliberate. `sqlite-vec` is 0.1.9 and its author says to
    // expect breaking changes; keeping the vectors as bytes we own means
    // replacing the vector index is a new virtual table populated from this
    // column, not a re-embedding run over the whole corpus.
    sql.push_str(
        "CREATE TABLE documents (
  doc_id            TEXT PRIMARY KEY,
  entity_type       TEXT NOT NULL,
  entity_id         TEXT NOT NULL,
  project_id        TEXT,
  version           INTEGER NOT NULL,
  parent_version    INTEGER,
  title             TEXT NOT NULL,
  body              TEXT NOT NULL,
  body_hash         TEXT NOT NULL,
  media_ref         TEXT,
  status            TEXT NOT NULL,
  author            TEXT NOT NULL,
  session_id        TEXT,
  surface           TEXT,
  created_at        TEXT NOT NULL,
  embedding         BLOB,
  embedding_model   TEXT,
  embedding_version INTEGER
) STRICT;
CREATE UNIQUE INDEX documents_version ON documents(entity_id, version);
CREATE INDEX documents_current ON documents(entity_id, status);
CREATE INDEX documents_project ON documents(project_id);

CREATE TABLE blobs (
  blob_id     TEXT PRIMARY KEY,
  entity_id   TEXT,
  project_id  TEXT,
  media_type  TEXT NOT NULL,
  byte_length INTEGER NOT NULL,
  sha256      TEXT NOT NULL,
  bytes       BLOB NOT NULL,
  created_at  TEXT NOT NULL
) STRICT;
CREATE INDEX blobs_entity ON blobs(entity_id);
CREATE INDEX blobs_sha ON blobs(sha256);
",
    );

    sql.push_str(&vertex_view());
    sql.push_str(&fts_schema());
    sql
}

/// `v_entities` — the UNION over every table that lets a query resolve an id
/// without knowing its type.
///
/// Built from a list rather than written out so that adding a fourteenth type
/// is one line in one place. Which is not to say a fourteenth type is coming:
/// hard constraint 6 puts the ceiling at thirteen.
fn vertex_view() -> String {
    // (table, entity type, the column that stands in for a label)
    const SOURCES: &[(&str, &str, &str)] = &[
        ("projects", "project", "name"),
        ("milestones", "milestone", "name"),
        ("tasks", "task", "title"),
        ("specs", "spec", "title"),
        ("decisions", "decision", "title"),
        ("questions", "question", "title"),
        ("terms", "term", "term"),
        // `label` unifies the four different column names — `name`, `title`,
        // `term`, `summary` — that all mean "what this is called".
        ("feedback", "feedback", "summary"),
        ("design_artifacts", "design", "name"),
        ("environments", "environment", "name"),
        ("artifacts", "artifact", "name"),
        ("metrics", "metric", "name"),
    ];

    let mut selects: Vec<String> = SOURCES
        .iter()
        .map(|(table, ty, label)| {
            // `projects` has no `project_id` of its own; it is its own project.
            let project = if *table == "projects" {
                "id".to_owned()
            } else if *table == "terms" {
                // A global term belongs to no project, and an empty string is
                // how every other query in the store spells that.
                "COALESCE(project_id, '')".to_owned()
            } else {
                "project_id".to_owned()
            };
            format!(
                "SELECT id, '{ty}' AS entity_type, {project} AS project_id, \
                 {label} AS label, archived_at FROM {table}"
            )
        })
        .collect();

    // Observations have no label column of their own; `note` stands in.
    selects.push(
        "SELECT id, 'metric_observation' AS entity_type, project_id AS project_id, \
         COALESCE(note, 'observation') AS label, archived_at FROM metric_observations"
            .to_owned(),
    );

    format!(
        "CREATE VIEW v_entities AS\n{};\n",
        selects.join("\nUNION ALL\n")
    )
}

/// The keyword index, and the triggers that keep it honest.
///
/// This is the part that removes the stall KEEL-123 measured. DuckDB's
/// full-text index did not update when its table changed, so the store before
/// this one rebuilt the entire index on the first search after any write —
/// 217 ms against a 13 ms mean, every time Claude touched anything.
///
/// FTS5 with `content=''` is an ordinary index maintained by triggers, so a row
/// written in one transaction is findable in the next call, measured at 68 µs.
/// There is no watermark, no rebuild, and nothing to invalidate.
///
/// **Why one index over a UNION rather than one per table.** BM25 scores are
/// only comparable within a single index — the term statistics that make up the
/// score are computed per index. Thirteen indexes would give thirteen
/// incomparable scores and the fusion would be ranking noise.
///
/// **Why there is a `fts_source` table in the middle.** FTS5 indexes rows by an
/// integer rowid, and Keel's ids are ULID text. Something has to hold the
/// mapping, and making it a real table rather than hiding it inside the index
/// buys two things: the index becomes external-content, so it stores no second
/// copy of every body, and "what is in the index" is a question answerable with
/// an ordinary `SELECT` rather than only through a `MATCH`.
///
/// It is also where archiving is handled. An archived row leaves `fts_source`,
/// so it leaves the index — searching should not return something a person put
/// away, and doing it here means no query has to remember to filter.
///
/// **Which tables feed it, and why prose types are not among them.** Specs,
/// decisions, questions, feedback and designs carry their text in `documents`,
/// not on their own row, so they are indexed from there instead. The six that
/// appear directly are the ones whose text *is* their row. Nothing is indexed
/// twice, because `fts_source` is keyed by entity id.
fn fts_schema() -> String {
    // (table, entity type, the label expression, the body expression)
    //
    // The same six tables the DuckDB store's UNION covered; the difference is
    // that this index is maintained as rows change rather than rebuilt on every
    // search.
    //
    // Every column reference is `new.`-qualified, and it has to be. Inside a
    // trigger body a bare column name is not resolved against the row being
    // written — SQLite looks it up in the table being *inserted into*, which
    // here is `fts_source`, and fails with "no such column: title". That error
    // names the right column and points at entirely the wrong table.
    const SOURCES: &[(&str, &str, &str, &str)] = &[
        (
            "projects",
            "project",
            "new.name",
            "COALESCE(new.description, '')",
        ),
        (
            "milestones",
            "milestone",
            "new.name",
            "COALESCE(new.summary, '')",
        ),
        ("tasks", "task", "new.title", "COALESCE(new.body, '')"),
        ("terms", "term", "new.term", "new.definition"),
        (
            "environments",
            "environment",
            "new.name",
            "COALESCE(new.url, '') || ' ' || COALESCE(new.deployed_version, '')",
        ),
        ("artifacts", "artifact", "new.name", "COALESCE(new.url, '')"),
    ];

    let mut sql = String::from(
        "CREATE TABLE fts_source (
  rowid       INTEGER PRIMARY KEY,
  entity_id   TEXT NOT NULL UNIQUE,
  entity_type TEXT NOT NULL,
  project_id  TEXT NOT NULL DEFAULT '',
  label       TEXT NOT NULL DEFAULT '',
  body        TEXT NOT NULL DEFAULT ''
) STRICT;
CREATE INDEX fts_source_project ON fts_source(project_id);

CREATE VIRTUAL TABLE fts_entities USING fts5(
  label,
  body,
  content='fts_source',
  content_rowid='rowid',
  tokenize='porter unicode61'
);

-- The three that keep the index in step with the table under it. FTS5's
-- external-content mode does not do this itself; it trusts these to be right,
-- which is why the delete has to carry the *old* text — the index needs the
-- terms it is removing, and `old` is the only place they still exist.
CREATE TRIGGER fts_source_ai AFTER INSERT ON fts_source BEGIN
  INSERT INTO fts_entities(rowid, label, body)
    VALUES (new.rowid, new.label, new.body);
END;
CREATE TRIGGER fts_source_ad AFTER DELETE ON fts_source BEGIN
  INSERT INTO fts_entities(fts_entities, rowid, label, body)
    VALUES ('delete', old.rowid, old.label, old.body);
END;
CREATE TRIGGER fts_source_au AFTER UPDATE ON fts_source BEGIN
  INSERT INTO fts_entities(fts_entities, rowid, label, body)
    VALUES ('delete', old.rowid, old.label, old.body);
  INSERT INTO fts_entities(rowid, label, body)
    VALUES (new.rowid, new.label, new.body);
END;
",
    );

    for (table, ty, label, body) in SOURCES {
        // The upsert is keyed on `entity_id`, so a row that changes twenty
        // times occupies one slot in the index rather than twenty.
        let upsert = format!(
            "INSERT INTO fts_source (entity_id, entity_type, project_id, label, body)
     VALUES (new.id, '{ty}', {project}, {label}, {body})
   ON CONFLICT(entity_id) DO UPDATE SET
     label = excluded.label, body = excluded.body, project_id = excluded.project_id;",
            project = if *table == "projects" {
                "new.id"
            } else if *table == "terms" {
                "COALESCE(new.project_id, '')"
            } else {
                "new.project_id"
            }
        );

        sql.push_str(&format!(
            "
CREATE TRIGGER {table}_fts_ai AFTER INSERT ON {table}
  WHEN new.archived_at IS NULL
BEGIN
  {upsert}
END;
CREATE TRIGGER {table}_fts_au AFTER UPDATE ON {table}
  WHEN new.archived_at IS NULL
BEGIN
  {upsert}
END;
-- Archiving removes it from the index. Doing it here rather than in every
-- query is what stops one forgotten `WHERE archived_at IS NULL` from putting
-- put-away rows back in front of someone.
CREATE TRIGGER {table}_fts_archived AFTER UPDATE ON {table}
  WHEN new.archived_at IS NOT NULL
BEGIN
  DELETE FROM fts_source WHERE entity_id = new.id;
END;
"
        ));
    }

    // Documents carry the text for the five prose types. Keyed by `entity_id`
    // rather than `doc_id`, so a new revision replaces its predecessor in the
    // index instead of accumulating one entry per version — which would make a
    // heavily-edited spec outrank everything by sheer repetition.
    sql.push_str(
        "
CREATE TRIGGER documents_fts_ai AFTER INSERT ON documents
  WHEN new.status = 'current'
BEGIN
  INSERT INTO fts_source (entity_id, entity_type, project_id, label, body)
    VALUES (new.entity_id, new.entity_type, COALESCE(new.project_id, ''),
            new.title, new.body)
  ON CONFLICT(entity_id) DO UPDATE SET
    label = excluded.label, body = excluded.body, project_id = excluded.project_id;
END;
CREATE TRIGGER documents_fts_au AFTER UPDATE ON documents
  WHEN new.status = 'current'
BEGIN
  INSERT INTO fts_source (entity_id, entity_type, project_id, label, body)
    VALUES (new.entity_id, new.entity_type, COALESCE(new.project_id, ''),
            new.title, new.body)
  ON CONFLICT(entity_id) DO UPDATE SET
    label = excluded.label, body = excluded.body, project_id = excluded.project_id;
END;
",
    );

    sql
}

/// Every migration, in order.
///
/// A function rather than a `const` because migration 1 is composed. The list
/// is still fixed and ordered — nothing in it depends on runtime state.
pub fn migrations() -> Vec<Migration> {
    // Leaked to obtain a `&'static str` for a value computed once per process.
    // The alternative is a lifetime parameter on `Migration` for the sake of a
    // few kilobytes that live as long as the process anyway.
    let initial: &'static str = Box::leak(initial_schema().into_boxed_str());
    vec![Migration {
        id: 1,
        name: "initial_schema",
        sql: initial,
    }]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Every type in `EntityType::ALL` has to be reachable through
    /// `v_entities`, or `resolve_ref` returns `None` for a row that exists —
    /// which reads as "no such thing" rather than as the bug it is.
    #[test]
    fn the_vertex_view_covers_every_entity_type() {
        let view = vertex_view();
        for ty in crate::EntityType::ALL {
            let needle = format!("'{}' AS entity_type", ty.as_str());
            assert!(
                view.contains(&needle),
                "v_entities is missing {}",
                ty.as_str()
            );
        }
    }

    #[test]
    fn migration_ids_are_unique_and_ascending() {
        let ids: Vec<i32> = migrations().iter().map(|m| m.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted, "migrations must be unique and in order");
    }

    /// A migration that is not `STRICT` accepts a string into an integer column
    /// and stores it as a string, and the read that fails is nowhere near the
    /// write that caused it. Every entity table opts in.
    #[test]
    fn every_entity_table_is_strict() {
        let sql = initial_schema();
        let creates = sql.matches("CREATE TABLE ").count();
        let stricts = sql.matches(") STRICT;").count();
        assert_eq!(
            creates, stricts,
            "every CREATE TABLE must end in ) STRICT; — found {creates} tables and {stricts} strict"
        );
    }
}
