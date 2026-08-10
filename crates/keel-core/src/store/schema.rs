//! Forward-only schema migrations for DuckDB and Lance.
//!
//! Migrations are numbered, applied in order, and recorded in
//! `_keel_migrations`. There is no `down`: rolling a schema backwards on a
//! single-user store is a fiction that costs more to maintain than it ever
//! repays, and SPEC §11 makes a backup run before every migration anyway —
//! restoring is the rollback.
//!
//! # Two engines, one migration list
//!
//! The Lance datasets are created through DuckDB's `lance` extension, so their
//! DDL lives in the same numbered sequence as the DuckDB tables (DECISIONS
//! B-2). That matters: a store whose DuckDB half is at migration 3 and whose
//! Lance half is at migration 1 is exactly the split-brain the design avoids
//! everywhere else.

/// One forward-only migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Applied in ascending order. Never reused, never reordered.
    pub id: i32,
    /// What it does, for the `_keel_migrations` table and for `fsck` output.
    pub name: &'static str,
    /// The statements, separated by `;`. Executed as one batch.
    pub sql: &'static str,
}

/// The audit columns every entity table carries, as SQL.
///
/// SPEC §3.1 writes this as `<audit>` to avoid repeating it thirteen times;
/// this constant is that expansion. `events` deliberately does not use it —
/// append-only means no `updated_at`, no `version`, no `archived_at`.
const AUDIT: &str = "
  created_at   TIMESTAMP NOT NULL,
  updated_at   TIMESTAMP NOT NULL,
  version      INTEGER   NOT NULL DEFAULT 1,
  created_by   VARCHAR   NOT NULL,
  updated_by   VARCHAR   NOT NULL,
  session_id   VARCHAR,
  surface      VARCHAR,
  archived_at  TIMESTAMP";

/// Build migration 1's DDL.
///
/// A function rather than a literal so the audit block appears once. The
/// output is deterministic, and the migration is only ever applied to an empty
/// database, so composing it at runtime costs nothing and removes thirteen
/// opportunities to fat-finger a column.
fn initial_schema() -> String {
    let mut sql = String::new();

    // ---- Structural -----------------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE projects (
  id              VARCHAR PRIMARY KEY,
  slug            VARCHAR UNIQUE NOT NULL,
  name            VARCHAR NOT NULL,
  description     VARCHAR,
  status          VARCHAR NOT NULL DEFAULT 'active',
  repo_urls       VARCHAR[],
  root_path       VARCHAR,
  status_path     VARCHAR,
  aliases         VARCHAR[],
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX projects_idem ON projects(idempotency_key);

CREATE TABLE milestones (
  id              VARCHAR PRIMARY KEY,
  project_id      VARCHAR NOT NULL,
  kind            VARCHAR NOT NULL DEFAULT 'milestone',
  name            VARCHAR NOT NULL,
  summary         VARCHAR,
  status          VARCHAR NOT NULL DEFAULT 'planned',
  target_date     DATE,
  shipped_at      TIMESTAMP,
  version_string  VARCHAR,
  sort_order      INTEGER,
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX milestones_idem ON milestones(project_id, idempotency_key);
CREATE INDEX milestones_project ON milestones(project_id);

CREATE TABLE tasks (
  id              VARCHAR PRIMARY KEY,
  project_id      VARCHAR NOT NULL,
  milestone_id    VARCHAR,
  kind            VARCHAR NOT NULL DEFAULT 'task',
  title           VARCHAR NOT NULL,
  body            VARCHAR,
  status          VARCHAR NOT NULL DEFAULT 'todo',
  priority        VARCHAR DEFAULT 'p2',
  labels          VARCHAR[],
  external_ref    VARCHAR,
  closed_at       TIMESTAMP,
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX tasks_idem ON tasks(project_id, idempotency_key);
CREATE INDEX tasks_project ON tasks(project_id);
CREATE INDEX tasks_milestone ON tasks(milestone_id);
"
    ));

    // ---- Knowledge ------------------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE specs (
  id                  VARCHAR PRIMARY KEY,
  project_id          VARCHAR NOT NULL,
  kind                VARCHAR NOT NULL DEFAULT 'spec',
  title               VARCHAR NOT NULL,
  status              VARCHAR NOT NULL DEFAULT 'draft',
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         VARCHAR,
  idempotency_key     VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX specs_idem ON specs(project_id, idempotency_key);
CREATE INDEX specs_project ON specs(project_id);

CREATE TABLE decisions (
  id                  VARCHAR PRIMARY KEY,
  project_id          VARCHAR NOT NULL,
  title               VARCHAR NOT NULL,
  status              VARCHAR NOT NULL DEFAULT 'proposed',
  decided_at          TIMESTAMP,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         VARCHAR,
  idempotency_key     VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX decisions_idem ON decisions(project_id, idempotency_key);
CREATE INDEX decisions_project ON decisions(project_id);

CREATE TABLE questions (
  id                  VARCHAR PRIMARY KEY,
  project_id          VARCHAR NOT NULL,
  kind                VARCHAR NOT NULL DEFAULT 'question',
  title               VARCHAR NOT NULL,
  status              VARCHAR NOT NULL DEFAULT 'open',
  severity            VARCHAR,
  resolved_at         TIMESTAMP,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         VARCHAR,
  idempotency_key     VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX questions_idem ON questions(project_id, idempotency_key);
CREATE INDEX questions_project ON questions(project_id);

CREATE TABLE terms (
  id              VARCHAR PRIMARY KEY,
  project_id      VARCHAR,
  term            VARCHAR NOT NULL,
  definition      VARCHAR NOT NULL,
  aliases         VARCHAR[],
  mirror_path     VARCHAR,
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
-- COALESCE, not a bare column: a nullable column would let duplicate globals
-- through and make 'the override' ambiguous (PRD Q-4).
CREATE UNIQUE INDEX terms_uniq ON terms(COALESCE(project_id, ''), term);
CREATE UNIQUE INDEX terms_idem ON terms(COALESCE(project_id, ''), idempotency_key);
"
    ));

    // ---- Inputs and surfaces --------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE feedback (
  id                  VARCHAR PRIMARY KEY,
  project_id          VARCHAR NOT NULL,
  kind                VARCHAR NOT NULL DEFAULT 'observation',
  source              VARCHAR,
  contact             VARCHAR,
  sentiment           VARCHAR,
  occurred_at         TIMESTAMP,
  triaged             BOOLEAN DEFAULT FALSE,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  summary             VARCHAR NOT NULL,
  idempotency_key     VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX feedback_idem ON feedback(project_id, idempotency_key);
CREATE INDEX feedback_project ON feedback(project_id);

CREATE TABLE design_artifacts (
  id                  VARCHAR PRIMARY KEY,
  project_id          VARCHAR NOT NULL,
  name                VARCHAR NOT NULL,
  state               VARCHAR NOT NULL DEFAULT 'proposed',
  figma_ref           VARCHAR,
  blob_id             VARCHAR,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  idempotency_key     VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX design_idem ON design_artifacts(project_id, idempotency_key);
CREATE INDEX design_project ON design_artifacts(project_id);

CREATE TABLE environments (
  id               VARCHAR PRIMARY KEY,
  project_id       VARCHAR NOT NULL,
  name             VARCHAR NOT NULL,
  url              VARCHAR,
  deployed_version VARCHAR,
  deployed_commit  VARCHAR,
  status           VARCHAR DEFAULT 'unknown',
  last_deployed_at TIMESTAMP,
  idempotency_key  VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX environments_idem ON environments(project_id, idempotency_key);
CREATE INDEX environments_project ON environments(project_id);

CREATE TABLE metrics (
  id              VARCHAR PRIMARY KEY,
  project_id      VARCHAR NOT NULL,
  name            VARCHAR NOT NULL,
  unit            VARCHAR,
  target_value    DOUBLE,
  direction       VARCHAR DEFAULT 'up',
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX metrics_idem ON metrics(project_id, idempotency_key);
CREATE INDEX metrics_project ON metrics(project_id);

CREATE TABLE metric_observations (
  id              VARCHAR PRIMARY KEY,
  metric_id       VARCHAR NOT NULL,
  project_id      VARCHAR NOT NULL,
  value           DOUBLE NOT NULL,
  observed_at     TIMESTAMP NOT NULL,
  note            VARCHAR,
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX observations_idem ON metric_observations(project_id, idempotency_key);
CREATE INDEX observations_metric ON metric_observations(metric_id, observed_at);

CREATE TABLE artifacts (
  id              VARCHAR PRIMARY KEY,
  project_id      VARCHAR NOT NULL,
  name            VARCHAR NOT NULL,
  kind            VARCHAR DEFAULT 'link',
  url             VARCHAR,
  blob_id         VARCHAR,
  idempotency_key VARCHAR NOT NULL,
  {AUDIT}
);
CREATE UNIQUE INDEX artifacts_idem ON artifacts(project_id, idempotency_key);
CREATE INDEX artifacts_project ON artifacts(project_id);
"
    ));

    // ---- Connective tissue ----------------------------------------------
    sql.push_str(&format!(
        "CREATE TABLE links (
  id         VARCHAR PRIMARY KEY,
  project_id VARCHAR,
  from_type  VARCHAR NOT NULL,
  from_id    VARCHAR NOT NULL,
  rel        VARCHAR NOT NULL,
  to_type    VARCHAR NOT NULL,
  to_id      VARCHAR NOT NULL,
  -- NOT NULL with an empty-string default so the unique index actually fires.
  -- A nullable anchor would make every ordinary edge distinct from every other.
  anchor     VARCHAR NOT NULL DEFAULT '',
  note       VARCHAR,
  {AUDIT}
);
CREATE UNIQUE INDEX links_uniq ON links(from_id, rel, to_id, anchor);
CREATE INDEX links_from ON links(from_id);
CREATE INDEX links_to   ON links(to_id);

-- Append-only and immutable, so no audit block: no updated_at, no version,
-- no archived_at, because none of them could ever change (SPEC §3.1).
CREATE TABLE events (
  id          VARCHAR PRIMARY KEY,
  project_id  VARCHAR,
  entity_type VARCHAR NOT NULL,
  entity_id   VARCHAR NOT NULL,
  action      VARCHAR NOT NULL,
  field       VARCHAR,
  before      JSON,
  after       JSON,
  actor       VARCHAR NOT NULL,
  session_id  VARCHAR,
  surface     VARCHAR,
  summary     VARCHAR,
  meta        JSON,
  created_at  TIMESTAMP NOT NULL
);
CREATE INDEX events_project_time ON events(project_id, created_at);
CREATE INDEX events_entity ON events(entity_id);
"
    ));

    sql
}

/// The unified vertex view (SPEC §4, TQ-4).
///
/// Built now rather than deferred. It costs one `UNION ALL` today and is
/// annoying to retrofit, it gives `fsck` and cross-type lookups a single place
/// to resolve an id, and it is the shape DuckPGQ would need if it ever ships
/// for a DuckDB line that also carries Lance.
///
/// `label` unifies the four different column names — `name`, `title`, `term`,
/// `summary` — that all mean "what this is called".
fn vertex_view() -> String {
    let parts = [
        ("project", "projects", "name"),
        ("milestone", "milestones", "name"),
        ("task", "tasks", "title"),
        ("spec", "specs", "title"),
        ("decision", "decisions", "title"),
        ("question", "questions", "title"),
        ("term", "terms", "term"),
        ("feedback", "feedback", "summary"),
        ("design", "design_artifacts", "name"),
        ("environment", "environments", "name"),
        ("metric", "metrics", "name"),
        ("artifact", "artifacts", "name"),
    ];
    let mut selects: Vec<String> = parts
        .iter()
        .map(|(ty, table, label)| {
            // `projects` has no project_id column — it *is* the project.
            let project = if *ty == "project" { "id" } else { "project_id" };
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
        "CREATE OR REPLACE VIEW v_entities AS\n{};\n",
        selects.join("\nUNION ALL\n")
    )
}

/// Migration 2: the Lance datasets.
///
/// Written as DDL against the attached `lancedb` namespace. The embedding
/// column is a fixed-width `FLOAT[384]` because the model is fixed (D-7);
/// `embedding_model` and `embedding_version` travel with each row so that
/// changing models is a background pass over stale rows rather than a rewrite.
///
/// **No column constraints.** DuckDB's Lance extension rejects them outright
/// ("Lance CREATE TABLE does not support constraints"), so there is no
/// `NOT NULL` and no primary key here. That is not a gap so much as a
/// restatement of what SPEC §2.1 already says: integrity across the two
/// engines is an application-level invariant, enforced in `keel-core` on write
/// and audited by `keel-cli fsck`. Nothing in Lance was ever going to enforce
/// `entity_id` pointing at a real DuckDB row either.
const LANCE_SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS lancedb.documents (
  doc_id            VARCHAR,
  entity_type       VARCHAR,
  entity_id         VARCHAR,
  project_id        VARCHAR,
  version           INTEGER,
  parent_version    INTEGER,
  title             VARCHAR,
  body              VARCHAR,
  body_hash         VARCHAR,
  media_ref         VARCHAR,
  status            VARCHAR,
  author            VARCHAR,
  session_id        VARCHAR,
  surface           VARCHAR,
  created_at        TIMESTAMP,
  embedding         FLOAT[384],
  embedding_model   VARCHAR,
  embedding_version INTEGER
);

CREATE TABLE IF NOT EXISTS lancedb.blobs (
  blob_id     VARCHAR,
  entity_id   VARCHAR,
  project_id  VARCHAR,
  media_type  VARCHAR,
  byte_length BIGINT,
  sha256      VARCHAR,
  bytes       BLOB,
  created_at  TIMESTAMP
);
";

/// Every migration, in order.
///
/// Composed at call time rather than as a `const` because migration 1 and the
/// vertex view are built by functions. The list is still fixed and ordered —
/// nothing here depends on runtime state.
pub fn migrations() -> Vec<Migration> {
    // Leaked to obtain `&'static str` for a value computed once per process.
    // The alternative is threading lifetimes through `Migration` for the sake
    // of a few kilobytes that live as long as the process anyway.
    let initial: &'static str = Box::leak(initial_schema().into_boxed_str());
    let vertices: &'static str = Box::leak(vertex_view().into_boxed_str());
    vec![
        Migration {
            id: 1,
            name: "initial_schema",
            sql: initial,
        },
        Migration {
            id: 2,
            name: "lance_datasets",
            sql: LANCE_SCHEMA,
        },
        Migration {
            id: 3,
            name: "unified_vertex_view",
            sql: vertices,
        },
        // Additive and nullable, so an existing store picks it up without
        // touching a row. The tracker needs somewhere to be written to, and
        // deriving it from `root_path` would hard-code `product/STATUS.md`
        // into every project that never asked for one.
        Migration {
            id: 4,
            name: "project_status_path",
            sql: "ALTER TABLE projects ADD COLUMN IF NOT EXISTS status_path VARCHAR;",
        },
        // The running commentary that used to live only in the tracker's prose.
        // Append-only, so it carries `created_at` and `archived_at` but neither
        // `updated_at` nor `version` — see `note.rs` for why editing a note is
        // the failure mode rather than the feature.
        Migration {
            id: 5,
            name: "entity_notes",
            sql: "
CREATE TABLE IF NOT EXISTS notes (
  id          VARCHAR PRIMARY KEY,
  project_id  VARCHAR,
  entity_type VARCHAR NOT NULL,
  entity_id   VARCHAR NOT NULL,
  body        VARCHAR NOT NULL,
  author      VARCHAR NOT NULL,
  session_id  VARCHAR,
  surface     VARCHAR,
  created_at  TIMESTAMP NOT NULL,
  archived_at TIMESTAMP
);
-- The only query that matters: one row's stream, oldest first. Ordering is by
-- id, which is a ULID, so this index serves the sort as well as the filter.
CREATE INDEX IF NOT EXISTS notes_entity ON notes(entity_id, id);
CREATE INDEX IF NOT EXISTS notes_project ON notes(project_id);
",
        },
    ]
}

/// The bookkeeping table migrations record themselves in.
pub const MIGRATION_TABLE: &str = "
CREATE TABLE IF NOT EXISTS _keel_migrations (
  id         INTEGER PRIMARY KEY,
  name       VARCHAR NOT NULL,
  applied_at TIMESTAMP NOT NULL
);
";

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::EntityType;

    #[test]
    fn migration_ids_are_unique_and_ascending() {
        let m = migrations();
        assert!(!m.is_empty());
        for pair in m.windows(2) {
            assert!(
                pair[0].id < pair[1].id,
                "migration {} is not before {}",
                pair[0].id,
                pair[1].id
            );
        }
    }

    #[test]
    fn every_entity_type_has_a_table_in_the_initial_schema() {
        let sql = initial_schema();
        for t in EntityType::ALL {
            assert!(
                sql.contains(&format!("CREATE TABLE {} (", t.table())),
                "no table for {t} (expected `{}`)",
                t.table()
            );
        }
    }

    /// The `CREATE TABLE` body for one table, whitespace-normalised.
    ///
    /// Matching on normalised text rather than the literal source keeps these
    /// assertions about the *schema* rather than about column alignment — a
    /// test that breaks when someone tidies the indentation teaches people to
    /// stop trusting it.
    fn table_body(sql: &str, table: &str) -> String {
        let start = sql
            .find(&format!("CREATE TABLE {table} ("))
            .unwrap_or_else(|| panic!("no CREATE TABLE for `{table}`"));
        let body = &sql[start..];
        let end = body.find(");").unwrap_or(body.len());
        body[..end].split_whitespace().collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn every_entity_table_carries_the_audit_block_and_an_idempotency_key() {
        let sql = initial_schema();
        for t in EntityType::ALL {
            let body = table_body(&sql, t.table());
            for col in [
                "created_at",
                "updated_at",
                "version",
                "created_by",
                "updated_by",
                "session_id",
                "surface",
                "archived_at",
            ] {
                assert!(body.contains(col), "{} is missing `{col}`", t.table());
            }
            assert!(
                body.contains("idempotency_key VARCHAR NOT NULL"),
                "PROVISIONAL (TQ-9): every create is idempotent per REQ-7, so `{}` \
                 must carry the key — not just `tasks` as SPEC §3.2 shows",
                t.table()
            );
        }
        // Links take the audit block too, but have no idempotency key: their
        // uniqueness is the natural key (from_id, rel, to_id, anchor).
        let links = table_body(&sql, "links");
        assert!(links.contains("archived_at"));
        assert!(!links.contains("idempotency_key"));
    }

    #[test]
    fn events_has_no_audit_block() {
        let sql = initial_schema();
        let events = sql
            .split("CREATE TABLE events (")
            .nth(1)
            .expect("events table must exist");
        let body = events.split(");").next().unwrap();
        for forbidden in ["updated_at", "version", "archived_at"] {
            assert!(
                !body.contains(forbidden),
                "events is append-only and must not carry `{forbidden}`"
            );
        }
    }

    #[test]
    fn the_terms_index_coalesces_so_duplicate_globals_are_impossible() {
        assert!(initial_schema().contains("terms_uniq ON terms(COALESCE(project_id, ''), term)"));
    }

    #[test]
    fn the_link_anchor_is_not_nullable() {
        assert!(initial_schema().contains("anchor     VARCHAR NOT NULL DEFAULT ''"));
    }

    #[test]
    fn the_vertex_view_covers_all_thirteen_types() {
        let view = vertex_view();
        for t in EntityType::ALL {
            assert!(
                view.contains(&format!("'{}' AS entity_type", t.as_str())),
                "v_entities is missing {t}"
            );
        }
        assert_eq!(view.matches("UNION ALL").count(), EntityType::ALL.len() - 1);
    }
}
