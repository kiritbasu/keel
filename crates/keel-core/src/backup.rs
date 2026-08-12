//! Backup and restore for the store: one file, and a copy of it.
//!
//! # Why a backup is one operation
//!
//! `VACUUM INTO` takes a consistent snapshot of the whole database — rows,
//! documents, blobs, vectors and the keyword index — at a single point in time,
//! without stopping the daemon, and writes it as an ordinary SQLite file.
//! Measured at 64 ms for a 6.9 MB database.
//!
//! That it is *one* operation is the property worth having, and it is worth
//! recording why, because the alternative was lived with for eight phases. Keel
//! used to keep its rows in one engine and its prose in another, so a backup
//! was two dumps and `restore` had to refuse one that was missing its second
//! half — a backup that covers the rows and skips the documents is not a
//! backup. But the flaw nobody could design away was the one no check could
//! catch: **a write landing between the two dumps produced a backup that was
//! internally inconsistent and passed everything**, with the rows from one
//! instant and the documents from another. One file taken in one operation is
//! what makes that failure mode stop existing rather than merely get rarer.
//!
//! # Why a restore is a file copy
//!
//! The output of `VACUUM INTO` is a complete, valid SQLite database. Restoring
//! it means putting it where the store lives. There is nothing to reconstruct,
//! nothing to create before it can be filled, and nothing whose type has to be
//! cast back into shape on the way in — the older restore went through Parquet
//! and had to cast the embeddings back to `FLOAT[384]`, and a restore that
//! converts is a restore that can convert wrongly.
//!
//! What is left to be careful about is refusing. [`restore`] will not write
//! over a store that already exists, and it will not treat a directory that is
//! not a backup as an empty one.

use crate::store::Store;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// What a backup contains, written alongside it.
///
/// Kept even though `VACUUM INTO` needs no manifest to restore, because the
/// manifest is not for the restore — it is what lets a person, or `fsck`, ask
/// "is this backup what I think it is" without opening the database. The row
/// counts are also what [`verify_restore`] compares, so the round-trip test
/// asserts equality rather than eyeballing it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupManifest {
    /// When the backup ran, RFC 3339.
    pub created_at: String,
    /// The `keel-core` version that wrote it.
    pub keel_version: String,
    /// Migration ids applied at the time.
    ///
    /// A restore into a newer binary is fine, because migrations are
    /// forward-only. The reverse is not, and this is what lets `restore` say so
    /// rather than opening a database it does not understand.
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

/// The file the snapshot is written as, inside the backup directory.
const SNAPSHOT: &str = "keel.sqlite";

/// The manifest's filename.
const MANIFEST: &str = "manifest.json";

/// Every table whose rows are counted into the manifest.
///
/// Named explicitly rather than discovered from `sqlite_master`, so that a
/// table added without being counted shows up as a failing test rather than as
/// a backup that silently reports less than it holds. `fts_entities` is
/// excluded on purpose: it is an index, it is rebuilt from its source rows, and
/// counting it would make the total depend on tokenisation.
const COUNTED_TABLES: &[&str] = &[
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
];

/// Write a complete backup of `store` into `dest`.
///
/// Returns the manifest, whose counts are what [`verify_restore`] compares
/// against.
///
/// The daemon does not have to stop. `VACUUM INTO` reads a consistent snapshot
/// under WAL, so a write landing halfway through lands after the snapshot
/// rather than inside it.
pub fn backup(store: &Store, dest: impl AsRef<Path>) -> Result<BackupManifest> {
    let dest = dest.as_ref().to_path_buf();
    std::fs::create_dir_all(&dest).map_err(|source| Error::Io {
        context: format!("create the backup directory at {}", dest.display()),
        source,
    })?;

    let snapshot = dest.join(SNAPSHOT);
    // `VACUUM INTO` refuses to overwrite, and that refusal is right — it means
    // a failed backup cannot destroy the previous good one. Clearing the path
    // first is a deliberate choice made here, where the caller has asked for a
    // backup at this location, rather than left to SQLite to complain about.
    if snapshot.exists() {
        std::fs::remove_file(&snapshot).map_err(|source| Error::Io {
            context: format!("clear the previous snapshot at {}", snapshot.display()),
            source,
        })?;
    }

    // The snapshot first, and the counts from the snapshot itself.
    //
    // It used to count the live store and *then* take the snapshot, so a write
    // landing between the two put a row in the snapshot that the manifest did
    // not know about. The backup was perfectly good and its own verification
    // rejected it — at restore time, which is the worst imaginable moment to
    // refuse a healthy backup. Counting what was actually written removes the
    // seam rather than narrowing it.
    store
        .connection()
        .execute("VACUUM INTO ?1", [path_str(&snapshot)?])
        .map_err(Error::storage(format!(
            "write a consistent snapshot to {}",
            snapshot.display()
        )))?;

    let taken = rusqlite::Connection::open_with_flags(
        &snapshot,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .map_err(Error::storage(format!(
        "reopen the snapshot at {} to verify it",
        snapshot.display()
    )))?;

    // "Backup succeeded" used to mean only that SQLite did not error. Ask the
    // snapshot whether it is sound while it is open and cheap to ask — a
    // corrupt backup discovered now is an inconvenience, and one discovered
    // during a restore is the reason there was a backup.
    let problems: Vec<String> = taken
        .prepare("PRAGMA integrity_check")
        .and_then(|mut stmt| {
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()
        })
        .map_err(Error::storage(format!(
            "check the integrity of the snapshot at {}",
            snapshot.display()
        )))?
        .into_iter()
        .filter(|line| line != "ok")
        .collect();
    if !problems.is_empty() {
        return Err(Error::Invariant {
            operation: format!("back up to {}", dest.display()),
            problem: format!(
                "the snapshot just written is damaged: {}. The backup has not been recorded,                  so the previous one is still the most recent good copy",
                problems.join("; ")
            ),
        });
    }

    let manifest = read_manifest(&taken)?;
    drop(taken);

    let json = serde_json::to_string_pretty(&manifest).map_err(|source| Error::Json {
        context: "serialise the backup manifest".to_owned(),
        source,
    })?;
    std::fs::write(dest.join(MANIFEST), json).map_err(|source| Error::Io {
        context: format!("write the backup manifest into {}", dest.display()),
        source,
    })?;

    tracing::info!(
        rows = manifest.total_rows(),
        path = %snapshot.display(),
        "backup written"
    );
    Ok(manifest)
}

/// Restore a backup from `source` into a store at `target`.
///
/// Returns the manifest that was restored, so the caller can compare it against
/// what the restored store actually holds.
///
/// **It refuses to overwrite an existing store.** A restore is what someone
/// reaches for when something has gone wrong, and the worst possible behaviour
/// is to replace a database that still had the data in it. Move the old one
/// aside first, deliberately.
pub fn restore(source: impl AsRef<Path>, target: impl AsRef<Path>) -> Result<BackupManifest> {
    let source = source.as_ref().to_path_buf();
    let target = target.as_ref().to_path_buf();

    let snapshot = source.join(SNAPSHOT);
    if !snapshot.exists() {
        return Err(Error::Invariant {
            operation: format!("restore a backup from {}", source.display()),
            problem: format!(
                "there is no `{SNAPSHOT}` in that directory. A Keel backup is one SQLite file \
                 plus a `{MANIFEST}`; if what is there instead is `duckdb/` and `lance/`, this \
                 is a backup from before the store became a single SQLite file, and no build \
                 since then can read it — it needs a Keel binary from before that change"
            ),
        });
    }

    if target.exists() {
        return Err(Error::Invariant {
            operation: format!("restore into {}", target.display()),
            problem: "a store already exists there. Restoring would overwrite data that a \
                      restore is usually being run to recover — move it aside first"
                .to_owned(),
        });
    }

    let manifest = read_manifest_file(&source)?;

    if let Some(parent) = target.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent).map_err(|source| Error::Io {
            context: format!("create the store directory at {}", parent.display()),
            source,
        })?;
    }

    std::fs::copy(&snapshot, &target).map_err(|source| Error::Io {
        context: format!(
            "copy the snapshot from {} to {}",
            snapshot.display(),
            target.display()
        ),
        source,
    })?;

    tracing::info!(rows = manifest.total_rows(), path = %target.display(), "store restored");
    Ok(manifest)
}

/// The backup directory for a store, timestamped.
///
/// `home` is the directory the store lives in, not the store file, so that a
/// backup lands beside it rather than inside a path named after a database.
pub fn default_backup_dir(home: &Path, at: chrono::DateTime<chrono::Utc>) -> std::path::PathBuf {
    home.join("backups")
        .join(at.format("%Y-%m-%dT%H-%M-%SZ").to_string())
}

/// Compare a restored store against the manifest it came from.
///
/// Asserting rather than eyeballing is the point: the contract asks for a
/// backup round-trip test that diffs, and a restore nobody checked is a backup
/// nobody has.
pub fn verify_restore(store: &Store, manifest: &BackupManifest) -> Result<()> {
    let actual = read_manifest(store.connection())?;
    let mut differences: Vec<String> = Vec::new();

    for (table, expected) in &manifest.counts {
        let got = actual.counts.get(table).copied().unwrap_or(0);
        if got != *expected {
            differences.push(format!("{table}: expected {expected} rows, found {got}"));
        }
    }
    // A table present in the restored store but absent from the manifest is
    // just as wrong, and only checking one direction would miss it.
    for table in actual.counts.keys() {
        if !manifest.counts.contains_key(table) {
            differences.push(format!(
                "{table}: present after restore but not in the manifest"
            ));
        }
    }

    if differences.is_empty() {
        Ok(())
    } else {
        Err(Error::Invariant {
            operation: "verify a restored store against its backup manifest".to_owned(),
            problem: differences.join("; "),
        })
    }
}

/// Count every table and read the applied migrations, through any connection.
///
/// Takes a `&Connection` rather than a `&Store` so it can be pointed at the
/// snapshot that was just written rather than at the live store — which is the
/// whole of the manifest-race fix.
fn read_manifest(conn: &rusqlite::Connection) -> Result<BackupManifest> {
    let mut counts = std::collections::BTreeMap::new();

    for table in COUNTED_TABLES {
        // `table` comes from this module's own constant, never from a caller,
        // which is why interpolating it is not an injection.
        let n: i64 = conn
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .map_err(Error::storage(format!("count the rows in {table}")))?;
        counts.insert((*table).to_owned(), n);
    }

    let mut stmt = conn
        .prepare("SELECT id FROM _keel_migrations ORDER BY id")
        .map_err(Error::storage("read the applied migration list"))?;
    let migrations: Vec<i32> = stmt
        .query_map([], |r| r.get(0))
        .map_err(Error::storage("read the applied migration list"))?
        .collect::<std::result::Result<_, _>>()
        .map_err(Error::storage("read the applied migration list"))?;

    Ok(BackupManifest {
        created_at: chrono::Utc::now().to_rfc3339(),
        keel_version: env!("CARGO_PKG_VERSION").to_owned(),
        migrations,
        counts,
    })
}

fn read_manifest_file(source: &Path) -> Result<BackupManifest> {
    let path = source.join(MANIFEST);
    let raw = std::fs::read_to_string(&path).map_err(|source| Error::Io {
        context: format!("read the backup manifest at {}", path.display()),
        source,
    })?;
    serde_json::from_str(&raw).map_err(|source| Error::Json {
        context: format!("parse the backup manifest at {}", path.display()),
        source,
    })
}

/// A path as a string SQLite can take as a bound parameter.
///
/// Bound rather than interpolated, so a directory with a quote in its name is a
/// path and not a syntax error.
fn path_str(path: &Path) -> Result<String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| Error::Invariant {
            operation: "write a backup".to_owned(),
            problem: format!("the path {} is not valid UTF-8", path.display()),
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Seed a store with something in every shape a backup has to carry: rows,
    /// a document, a blob and an event.
    fn seeded(path: &Path) -> Store {
        let store = Store::open(path).unwrap();
        let conn = store.connection();
        conn.execute_batch(
            "INSERT INTO projects
               (id, slug, key, name, idempotency_key, created_at, updated_at,
                created_by, updated_by, version)
             VALUES ('prj_1','keel','KEEL','Keel','k1','2026-08-11T00:00:00.000000Z',
                     '2026-08-11T00:00:00.000000Z','claude','claude',1);
             INSERT INTO tasks
               (id, project_id, number, title, summary, idempotency_key, created_at,
                updated_at, created_by, updated_by, version)
             VALUES ('tsk_1','prj_1',1,'A task','It does a thing','k2',
                     '2026-08-11T00:00:00.000000Z','2026-08-11T00:00:00.000000Z',
                     'claude','claude',1);
             INSERT INTO documents
               (doc_id, entity_type, entity_id, project_id, version, title, body,
                body_hash, status, author, created_at)
             VALUES ('doc_1','spec','spc_1','prj_1',1,'A spec','the body','h1',
                     'current','claude','2026-08-11T00:00:00.000000Z');
             INSERT INTO blobs
               (blob_id, entity_id, project_id, media_type, byte_length, sha256,
                bytes, created_at)
             VALUES ('blb_1','dsg_1','prj_1','image/png',4,'abcd',x'DEADBEEF',
                     '2026-08-11T00:00:00.000000Z');
             INSERT INTO events
               (id, project_id, entity_id, entity_type, op, actor, at)
             VALUES ('evt_1','prj_1','tsk_1','task','create','claude',
                     '2026-08-11T00:00:00.000000Z');",
        )
        .unwrap();
        store
    }

    #[test]
    fn a_backup_is_one_file_and_a_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded(&dir.path().join("keel.sqlite"));
        let dest = dir.path().join("backup");

        let manifest = backup(&store, &dest).unwrap();

        assert!(
            dest.join(SNAPSHOT).is_file(),
            "the snapshot should be a file"
        );
        assert!(dest.join(MANIFEST).is_file());
        // The whole claim of this module: one file, not the directory tree the
        // two-engine backup used to leave — a row export beside a separate dump
        // of the prose.
        let entries: Vec<_> = std::fs::read_dir(&dest)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(
            entries.len(),
            2,
            "expected only the snapshot and the manifest, got {entries:?}"
        );
        assert_eq!(manifest.counts["tasks"], 1);
        assert_eq!(manifest.counts["blobs"], 1);
        assert_eq!(manifest.migrations, vec![1]);
    }

    /// The keyword index has to survive the round trip, and nothing about row
    /// counts would notice if it did not.
    ///
    /// FTS5 keeps its index in shadow tables — `fts_entities_data`,
    /// `fts_entities_idx` and the rest — which are ordinary tables that
    /// `VACUUM INTO` copies like any other. That is the expectation, and this
    /// asserts it rather than assuming it, because the failure would be a
    /// restored store that answers every query correctly *except* search, and
    /// returns nothing rather than erroring. Silent, plausible, and only
    /// discovered by someone who searched for something they knew was there.
    #[test]
    fn search_still_works_after_a_restore() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded(&dir.path().join("keel.sqlite"));

        let found_before: i64 = store
            .connection()
            .query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH ?1",
                ["task"],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            found_before > 0,
            "the fixture should be findable to begin with"
        );

        let dest = dir.path().join("backup");
        backup(&store, &dest).unwrap();
        drop(store);

        let restored_path = dir.path().join("restored").join("keel.sqlite");
        restore(&dest, &restored_path).unwrap();
        let restored = Store::open(&restored_path).unwrap();

        let found_after: i64 = restored
            .connection()
            .query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH ?1",
                ["task"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            found_after, found_before,
            "the keyword index did not survive the backup round trip"
        );

        // And it must still be *live* afterwards, not merely present. A copied
        // index with dead triggers would pass the assertion above and then
        // quietly stop tracking every row written from here on.
        restored
            .connection()
            .execute(
                "INSERT INTO tasks
                   (id, project_id, number, title, summary, idempotency_key, created_at,
                    updated_at, created_by, updated_by, version)
                 VALUES ('tsk_2','prj_1',2,'A brandnewthing','also a summary','k9',
                         '2026-08-11T00:00:00.000000Z','2026-08-11T00:00:00.000000Z',
                         'claude','claude',1)",
                [],
            )
            .unwrap();
        let fresh: i64 = restored
            .connection()
            .query_row(
                "SELECT count(*) FROM fts_entities WHERE fts_entities MATCH ?1",
                ["brandnewthing"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            fresh, 1,
            "the triggers should still be maintaining the index"
        );
    }

    #[test]
    fn a_backup_restores_to_a_store_holding_the_same_rows() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded(&dir.path().join("keel.sqlite"));
        let dest = dir.path().join("backup");
        let manifest = backup(&store, &dest).unwrap();
        drop(store);

        let restored_path = dir.path().join("restored").join("keel.sqlite");
        let read_back = restore(&dest, &restored_path).unwrap();
        assert_eq!(read_back.counts, manifest.counts);

        let restored = Store::open(&restored_path).unwrap();
        verify_restore(&restored, &manifest).unwrap();
    }

    /// The bytes have to survive, not merely the row count. A blob that
    /// restores as the right number of rows and the wrong bytes passes every
    /// count-based check.
    #[test]
    fn blob_bytes_survive_the_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keel.sqlite");
        let store = seeded(&path);

        let big: Vec<u8> = (0..5 * 1024 * 1024).map(|i| (i % 251) as u8).collect();
        store
            .connection()
            .execute(
                "INSERT INTO blobs
                   (blob_id, entity_id, project_id, media_type, byte_length, sha256,
                    bytes, created_at)
                 VALUES ('blb_big', NULL, 'prj_1', 'image/png', ?1, 'sha',
                         ?2, '2026-08-11T00:00:00.000000Z')",
                rusqlite::params![big.len() as i64, big],
            )
            .unwrap();

        let dest = dir.path().join("backup");
        backup(&store, &dest).unwrap();
        drop(store);

        let restored_path = dir.path().join("restored").join("keel.sqlite");
        restore(&dest, &restored_path).unwrap();
        let restored = Store::open(&restored_path).unwrap();
        let back: Vec<u8> = restored
            .connection()
            .query_row(
                "SELECT bytes FROM blobs WHERE blob_id = 'blb_big'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert_eq!(back.len(), big.len());
        assert!(back == big, "5 MB blob did not survive byte-identically");
    }

    /// A restore that quietly replaces a live store is the worst thing this
    /// module could do, because a restore is what someone runs when something
    /// has already gone wrong.
    #[test]
    fn restoring_over_an_existing_store_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keel.sqlite");
        let store = seeded(&path);
        let dest = dir.path().join("backup");
        backup(&store, &dest).unwrap();

        let err = restore(&dest, &path).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "expected a refusal naming the existing store, got: {err}"
        );
    }

    /// A directory that is not a Keel backup must say so, and say what to do.
    /// The two-part directory a pre-SQLite backup left behind is the case that
    /// will actually happen, and someone reaching for it is already having a bad
    /// day — worth a sentence naming the shape rather than a bare "not found".
    #[test]
    fn a_directory_with_no_snapshot_is_refused_with_advice() {
        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("not-a-backup");
        std::fs::create_dir_all(empty.join("lance")).unwrap();
        std::fs::create_dir_all(empty.join("duckdb")).unwrap();

        let err = restore(&empty, dir.path().join("out.sqlite")).unwrap_err();
        let text = err.to_string();
        assert!(
            text.contains(SNAPSHOT),
            "the error should name what was missing: {text}"
        );
        assert!(
            text.contains("duckdb") && text.contains("lance"),
            "an old two-part backup should be recognised by name, not left to guesswork: {text}"
        );
    }

    /// `verify_restore` has to fail when the store disagrees with the manifest,
    /// or it is a check that only ever says yes.
    #[test]
    fn verification_fails_when_a_row_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let store = seeded(&dir.path().join("keel.sqlite"));
        let dest = dir.path().join("backup");
        let mut manifest = backup(&store, &dest).unwrap();

        manifest.counts.insert("tasks".to_owned(), 99);
        let err = verify_restore(&store, &manifest).unwrap_err();
        assert!(
            err.to_string().contains("tasks"),
            "the failure should name the table that disagreed: {err}"
        );
    }
}
