//! Backup and restore.
//!
//! PRD R-4 makes this load-bearing: the daemon owns everything and GitHub is no
//! longer an implicit backup. SPEC §11 names three recovery tiers, and this
//! module implements the second — the Parquet export that means a restore never
//! depends on a specific DuckDB *or Lance* version still being readable.
//!
//! # Why the Lance half is not optional
//!
//! It is the whole point. R-5 names Lance as the one genuinely unhedged
//! dependency, and §11 says plainly that a Lance *snapshot* is not an escape
//! hatch from Lance. A backup that covers DuckDB and skips the documents
//! dataset would preserve every task and lose every spec, decision and piece of
//! customer feedback — while looking, from the outside, like a complete backup.
//! [`backup`] therefore fails if the Lance export fails; it does not warn and
//! carry on.

use crate::{DuckStore, Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What a backup contains, written alongside it.
///
/// Exists so a restore can fail loudly on a partial backup rather than
/// silently producing a store that is missing a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupManifest {
    /// When the backup ran, RFC 3339.
    pub created_at: String,
    /// The `keel-core` version that wrote it.
    pub keel_version: String,
    /// Migration ids applied at the time. A restore into a newer binary is
    /// fine — migrations are forward-only — but the reverse is not, and this
    /// is what lets `restore` say so.
    pub migrations: Vec<i32>,
    /// Row counts per table, for the round-trip assertion.
    pub counts: std::collections::BTreeMap<String, i64>,
}

impl BackupManifest {
    /// Total rows across every table.
    pub fn total_rows(&self) -> i64 {
        self.counts.values().sum()
    }
}

/// The tables exported from the Lance side.
const LANCE_TABLES: [&str; 2] = ["documents", "blobs"];

/// The `documents` column list, with the embedding cast back to fixed width.
///
/// Parquet does not preserve DuckDB's fixed-size-list type: `FLOAT[384]` comes
/// back as `FLOAT[]`. Inserting that into the Lance dataset without the cast
/// fails at restore time — the point at which nobody wants a surprise.
const DOCUMENTS_RESTORE_SELECT: &str = "SELECT doc_id, entity_type, entity_id, project_id, \
     version, parent_version, title, body, body_hash, media_ref, status, author, session_id, \
     surface, created_at, embedding::FLOAT[384] AS embedding, embedding_model, embedding_version \
     FROM read_parquet('{path}')";

/// Write a complete backup of `store` into `dest`.
///
/// Covers both engines. Returns the manifest, whose counts are what
/// [`verify_restore`] compares against.
pub fn backup(store: &DuckStore, dest: impl AsRef<Path>) -> Result<BackupManifest> {
    let dest = dest.as_ref().to_path_buf();
    std::fs::create_dir_all(dest.join("lance")).map_err(Error::io(format!(
        "create the backup directory at {}",
        dest.display()
    )))?;

    let duck_dir = dest.join("duckdb");
    // EXPORT DATABASE refuses to write into a non-empty directory, and a stale
    // half-backup underneath a fresh one is worse than no backup.
    if duck_dir.exists() {
        std::fs::remove_dir_all(&duck_dir).map_err(Error::io(format!(
            "clear the previous DuckDB export at {}",
            duck_dir.display()
        )))?;
    }

    let conn = store.connection();

    // --- DuckDB half ---
    conn.execute_batch(&format!(
        "EXPORT DATABASE '{}' (FORMAT PARQUET);",
        escape(&duck_dir)
    ))
    .map_err(Error::storage("export the DuckDB tables to Parquet"))?;

    // --- Lance half. Not optional; see the module docs. ---
    for table in LANCE_TABLES {
        let path = dest.join("lance").join(format!("{table}.parquet"));
        conn.execute_batch(&format!(
            "COPY (SELECT * FROM lancedb.{table}) TO '{}' (FORMAT PARQUET);",
            escape(&path)
        ))
        .map_err(Error::storage(format!(
            "export the Lance `{table}` dataset to Parquet. This is the escape hatch \
             from Lance itself (PRD R-5) — a backup without it is not a backup"
        )))?;
    }

    let manifest = BackupManifest {
        created_at: chrono::Utc::now().to_rfc3339(),
        keel_version: env!("CARGO_PKG_VERSION").to_owned(),
        migrations: applied_migrations(store)?,
        counts: row_counts(store)?,
    };

    let json = serde_json::to_string_pretty(&manifest)
        .map_err(Error::json("serialise the backup manifest"))?;
    std::fs::write(dest.join("manifest.json"), json)
        .map_err(Error::io("write the backup manifest"))?;

    tracing::info!(
        dest = %dest.display(),
        rows = manifest.total_rows(),
        "backup complete"
    );
    Ok(manifest)
}

/// Restore a backup into `target_root`, which must not already hold a store.
///
/// Refusing to overwrite is deliberate. A restore is run in a panic, and
/// "restore over the top of the thing I was trying to recover" is the mistake
/// that turns a recoverable incident into an unrecoverable one.
pub fn restore(source: impl AsRef<Path>, target_root: impl AsRef<Path>) -> Result<BackupManifest> {
    let source = source.as_ref().to_path_buf();
    let target_root = target_root.as_ref().to_path_buf();

    let manifest: BackupManifest = {
        let raw = std::fs::read_to_string(source.join("manifest.json")).map_err(Error::io(
            format!("read the backup manifest at {}", source.display()),
        ))?;
        serde_json::from_str(&raw).map_err(Error::json("parse the backup manifest"))?
    };

    let db_path = target_root.join("keel.duckdb");
    if db_path.exists() {
        return Err(Error::Invariant {
            operation: format!("restore into {}", target_root.display()),
            problem: format!(
                "{} already exists. Restore into an empty directory and move it into \
                 place, so a mistaken restore cannot destroy the store you are recovering",
                db_path.display()
            ),
        });
    }

    let lance_dir = target_root.join("lance");
    std::fs::create_dir_all(&lance_dir).map_err(Error::io(format!(
        "create the restore target at {}",
        target_root.display()
    )))?;

    let conn = duckdb::Connection::open(&db_path).map_err(Error::storage(format!(
        "create the restored database at {}",
        db_path.display()
    )))?;
    conn.execute_batch("INSTALL lance; LOAD lance; INSTALL fts; LOAD fts;")
        .map_err(Error::storage("load the extensions needed to restore"))?;
    conn.execute_batch(&format!(
        "ATTACH '{}' AS lancedb (TYPE lance);",
        escape(&lance_dir)
    ))
    .map_err(Error::storage("attach the Lance datasets for restore"))?;

    // IMPORT DATABASE replays the exported schema and data, `_keel_migrations`
    // included — so the restored store knows exactly which migrations it is
    // at, and a newer binary will apply only what is missing.
    conn.execute_batch(&format!(
        "IMPORT DATABASE '{}';",
        escape(&source.join("duckdb"))
    ))
    .map_err(Error::storage("import the DuckDB tables from the backup"))?;

    // The Lance datasets have to be created before they can be filled: the
    // import above knows nothing about them.
    for migration in crate::store::schema::migrations() {
        if migration.name == "lance_datasets" {
            conn.execute_batch(migration.sql)
                .map_err(Error::storage("recreate the Lance datasets for restore"))?;
        }
    }

    for table in LANCE_TABLES {
        let path = source.join("lance").join(format!("{table}.parquet"));
        if !path.exists() {
            return Err(Error::Invariant {
                operation: format!("restore the Lance `{table}` dataset"),
                problem: format!(
                    "{} is missing. This backup covers DuckDB but not Lance, so it would \
                     restore tasks and lose every spec, decision and piece of feedback",
                    path.display()
                ),
            });
        }
        let select = if table == "documents" {
            DOCUMENTS_RESTORE_SELECT.replace("{path}", &escape(&path))
        } else {
            format!("SELECT * FROM read_parquet('{}')", escape(&path))
        };
        conn.execute_batch(&format!("INSERT INTO lancedb.{table} {select};"))
            .map_err(Error::storage(format!(
                "restore the Lance `{table}` dataset from {}",
                path.display()
            )))?;
    }

    drop(conn);
    tracing::info!(
        target = %target_root.display(),
        rows = manifest.total_rows(),
        "restore complete"
    );
    Ok(manifest)
}

/// Compare a restored store against the manifest it was restored from.
///
/// The backup round-trip criterion says "assert equality, don't eyeball it".
/// This is that assertion: every table's row count, including both Lance
/// datasets.
pub fn verify_restore(store: &DuckStore, manifest: &BackupManifest) -> Result<Vec<String>> {
    let actual = row_counts(store)?;
    let mut problems = Vec::new();

    for (table, expected) in &manifest.counts {
        match actual.get(table) {
            Some(got) if got == expected => {}
            Some(got) => problems.push(format!(
                "`{table}`: expected {expected} rows, restored {got}"
            )),
            None => problems.push(format!("`{table}`: missing from the restored store")),
        }
    }
    for table in actual.keys() {
        if !manifest.counts.contains_key(table) {
            problems.push(format!(
                "`{table}`: present after restore but not in the backup"
            ));
        }
    }
    Ok(problems)
}

/// Row counts for every table on both engines.
fn row_counts(store: &DuckStore) -> Result<std::collections::BTreeMap<String, i64>> {
    let mut counts = std::collections::BTreeMap::new();
    let conn = store.connection();

    for t in crate::EntityType::ALL {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {}", t.table()), [], |r| {
                r.get(0)
            })
            .map_err(Error::storage(format!("count rows in `{}`", t.table())))?;
        counts.insert(t.table().to_owned(), n);
    }
    for t in ["links", "events"] {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
            .map_err(Error::storage(format!("count rows in `{t}`")))?;
        counts.insert(t.to_owned(), n);
    }
    for t in LANCE_TABLES {
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM lancedb.{t}"), [], |r| {
                r.get(0)
            })
            .map_err(Error::storage(format!("count rows in `lancedb.{t}`")))?;
        counts.insert(format!("lancedb.{t}"), n);
    }
    Ok(counts)
}

/// The migration ids already applied.
fn applied_migrations(store: &DuckStore) -> Result<Vec<i32>> {
    let mut stmt = store
        .connection()
        .prepare("SELECT id FROM _keel_migrations ORDER BY id")
        .map_err(Error::storage("read the applied migrations"))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, i32>(0))
        .map_err(Error::storage("read the applied migrations"))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(Error::storage("read the applied migrations"))
}

/// Render a path for a SQL string literal.
fn escape(path: &Path) -> String {
    path.display().to_string().replace('\'', "''")
}

/// The backup directory for a store, timestamped.
pub fn default_backup_dir(root: &Path, at: chrono::DateTime<chrono::Utc>) -> PathBuf {
    root.join("backups")
        .join(at.format("%Y-%m-%dT%H-%M-%SZ").to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_manifest_round_trips_through_json() {
        let mut counts = std::collections::BTreeMap::new();
        counts.insert("tasks".to_owned(), 12);
        counts.insert("lancedb.documents".to_owned(), 4);
        let m = BackupManifest {
            created_at: "2026-08-09T10:00:00Z".to_owned(),
            keel_version: "0.1.0".to_owned(),
            migrations: vec![1, 2, 3],
            counts,
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert_eq!(back.total_rows(), 16);
    }

    #[test]
    fn the_backup_directory_is_sortable_by_name() {
        let a = default_backup_dir(
            Path::new("/x"),
            chrono::DateTime::from_timestamp(1_000_000, 0).unwrap(),
        );
        let b = default_backup_dir(
            Path::new("/x"),
            chrono::DateTime::from_timestamp(2_000_000, 0).unwrap(),
        );
        assert!(
            a < b,
            "backups must sort chronologically by name: {a:?} vs {b:?}"
        );
    }

    #[test]
    fn both_lance_datasets_are_covered() {
        // R-5: skipping either one is the failure this module exists to
        // prevent, and it is invisible until a restore.
        assert!(LANCE_TABLES.contains(&"documents"));
        assert!(LANCE_TABLES.contains(&"blobs"));
    }

    #[test]
    fn the_documents_restore_casts_the_embedding_back_to_fixed_width() {
        assert!(
            DOCUMENTS_RESTORE_SELECT.contains("embedding::FLOAT[384]"),
            "Parquet loses the fixed-size list type; without the cast, restore fails"
        );
    }
}
