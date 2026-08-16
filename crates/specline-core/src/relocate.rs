//! Moving a store out of the directory Keel used and into Specline's.
//!
//! # Why this exists rather than a note in the release notes
//!
//! Everything else the rename touched fails loudly. A renamed environment
//! variable falls back to a default or the script exits; a renamed binary is
//! not on `PATH`; a renamed tool is absent from the model's namespace. Every
//! one of those is visible in the first second.
//!
//! The store is the exception, and it is the exception in the worst direction.
//! A binary that looks for `~/.specline`, finds nothing, and creates it does
//! not fail at all: it comes up clean, serves an empty digest, and reports
//! itself healthy while five projects and eighteen hundred events sit
//! untouched one directory over. That is this project's defining failure shape
//! — a plausible answer, no error, and nobody notices — arriving at the one
//! artifact that cannot be rebuilt from anything else.
//!
//! So the store moves itself, once, and says so.
//!
//! # What makes it safe to do automatically
//!
//! Three things, and the third is the one that took the thinking.
//!
//! 1. **It refuses while anything holds the store.** The advisory lock from
//!    B-60 is acquired before a byte moves. A daemon serving the old store is
//!    exactly the case where a rename underneath it would be silent corruption.
//! 2. **It is a rename, not a copy.** There is never a moment with two stores
//!    that could drift apart, and no window where a crash leaves half of one.
//! 3. **The write-ahead log moves with the database.** SQLite keeps committed
//!    transactions in `<db>-wal` until a checkpoint folds them in, so renaming
//!    the database and leaving the log behind discards every transaction since
//!    the last checkpoint — silently, because the store that arrives is
//!    internally consistent and merely older. The three files move together and
//!    SQLite recovers from the log on the next open.

use crate::{Error, Result, lock::StoreLock, store::STORE_FILE};
use chrono::Utc;
use std::path::{Path, PathBuf};

/// The home directory Keel used, relative to the user's home.
pub const LEGACY_HOME_DIR: &str = ".keel";

/// The home directory Specline uses, relative to the user's home.
pub const HOME_DIR: &str = ".specline";

/// The store file's name under Keel.
pub const LEGACY_STORE_FILE: &str = "keel.sqlite";

/// The marker left in a relocated home, naming where it came from.
pub const MARKER_FILE: &str = "relocated-from-keel.json";

/// What a relocation moved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relocated {
    /// The directory the store was in.
    pub from: PathBuf,
    /// The directory it is in now.
    pub to: PathBuf,
    /// Whether the database file inside it was renamed too.
    ///
    /// False when the old home held no store — an empty directory left behind
    /// by an install that never ran, which is worth moving and worth not
    /// claiming a database for.
    pub store_renamed: bool,
}

impl Relocated {
    /// One line for a person, saying what happened to their data.
    ///
    /// Printed rather than logged. Somebody whose store just moved should not
    /// have to find out from a log file that it did.
    pub fn describe(&self) -> String {
        format!(
            "moved your store from {} to {}",
            self.from.display(),
            self.to.display()
        )
    }
}

/// Move a Keel home to Specline's, if there is one and Specline has none.
///
/// Returns `None` when there is nothing to do, which is the common case: a
/// fresh install, or any run after the first. Returns an error only when a
/// relocation was called for and could not be completed — never when it was
/// simply unnecessary, because a new user with no old store must not meet a
/// failure about a product they never had.
pub fn relocate(legacy_home: &Path, home: &Path) -> Result<Option<Relocated>> {
    if !legacy_home.is_dir() {
        return Ok(None);
    }

    // An existing Specline home wins, always. Two stores means the user has
    // already been running the new binary, and quietly replacing what they
    // have been writing to with an older copy would be the one outcome worse
    // than not migrating at all.
    if home.exists() {
        if !is_empty_dir(home) {
            return Ok(None);
        }
        // An empty directory is not a store. It is what a `mkdir -p` or a
        // half-finished install leaves, and treating it as "already migrated"
        // is how the old store stays stranded behind something that looks
        // migrated. Clear it and continue.
        std::fs::remove_dir(home)
            .map_err(Error::io(format!("clear the empty {}", home.display())))?;
    }

    // Nothing moves while anything holds the store. The lock is released when
    // this binding drops, which is before the rename below.
    let legacy_store = legacy_home.join(LEGACY_STORE_FILE);
    let has_store = legacy_store.is_file();
    if has_store {
        let _lock = StoreLock::acquire(&legacy_store).map_err(|_| Error::Invariant {
            operation: format!("move {} to {}", legacy_home.display(), home.display()),
            problem: format!(
                "something else has {} open for writing, and a store cannot be moved \
                 out from under a process that is writing to it.\n\n\
                 It is almost always the daemon. Stop it and run this again:\n\n    \
                 specline daemon stop\n\n\
                 Nothing has been moved, so the store is exactly as it was.",
                legacy_store.display()
            ),
        })?;
    }

    if let Some(parent) = home.parent() {
        std::fs::create_dir_all(parent)
            .map_err(Error::io(format!("create {}", parent.display())))?;
    }

    std::fs::rename(legacy_home, home).map_err(Error::io(format!(
        "move {} to {}. Both are normally under your home directory; a rename \
         across two filesystems is refused by the operating system rather than \
         copied, and moving it by hand is the fix",
        legacy_home.display(),
        home.display()
    )))?;

    if has_store {
        rename_store_files(home)?;
    }

    write_marker(home, legacy_home, has_store)?;

    Ok(Some(Relocated {
        from: legacy_home.to_path_buf(),
        to: home.to_path_buf(),
        store_renamed: has_store,
    }))
}

/// Rename the database and everything SQLite keeps beside it.
///
/// `-wal` and `-shm` are not optional extras. The log holds committed
/// transactions until a checkpoint, so a database renamed without it comes up
/// consistent and missing whatever had not been folded in — the quiet kind of
/// data loss. The stale lock file is removed rather than carried across: it is
/// a claim held by a process, and by here we know no process holds it.
fn rename_store_files(home: &Path) -> Result<()> {
    let old = home.join(LEGACY_STORE_FILE);
    let new = home.join(STORE_FILE);

    for suffix in ["", "-wal", "-shm"] {
        let from = with_suffix(&old, suffix);
        if !from.exists() {
            continue;
        }
        let to = with_suffix(&new, suffix);
        std::fs::rename(&from, &to).map_err(Error::io(format!(
            "rename {} to {}",
            from.display(),
            to.display()
        )))?;
    }

    let stale_lock = crate::lock::lock_path(&old);
    if stale_lock.exists() {
        std::fs::remove_file(&stale_lock)
            .map_err(Error::io(format!("remove {}", stale_lock.display())))?;
    }

    Ok(())
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    if suffix.is_empty() {
        return path.to_path_buf();
    }
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Record what was moved, so the next question about it has an answer on disk.
fn write_marker(home: &Path, from: &Path, store_renamed: bool) -> Result<()> {
    let marker = home.join(MARKER_FILE);
    let body = serde_json::json!({
        "from": from.display().to_string(),
        "at": Utc::now().to_rfc3339(),
        "store_file": if store_renamed {
            serde_json::json!({ "from": LEGACY_STORE_FILE, "to": STORE_FILE })
        } else {
            serde_json::Value::Null
        },
        "why": "Keel was renamed to Specline. This directory was ~/.keel.",
    });
    let text = serde_json::to_string_pretty(&body)
        .map_err(|e| Error::io(format!("serialise {}", marker.display()))(e.into()))?;
    std::fs::write(&marker, text).map_err(Error::io(format!("write {}", marker.display())))?;
    Ok(())
}

fn is_empty_dir(path: &Path) -> bool {
    std::fs::read_dir(path).is_ok_and(|mut entries| entries.next().is_none())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Build a plausible old home: a database, a log, a shared-memory file and
    /// a stale lock, which is what an unclean shutdown actually leaves.
    fn legacy_home(root: &Path) -> PathBuf {
        let home = root.join(LEGACY_HOME_DIR);
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(home.join(LEGACY_STORE_FILE), b"the database").unwrap();
        std::fs::write(home.join("keel.sqlite-wal"), b"committed, not checkpointed").unwrap();
        std::fs::write(home.join("keel.sqlite-shm"), b"shared memory").unwrap();
        std::fs::write(home.join("keel.sqlite.lock"), b"").unwrap();
        std::fs::create_dir_all(home.join("models")).unwrap();
        home
    }

    #[test]
    fn a_store_moves_once_and_says_where_it_went() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);

        let moved = relocate(&old, &new).unwrap().expect("a store to move");

        assert_eq!(moved.from, old);
        assert_eq!(moved.to, new);
        assert!(moved.store_renamed);
        assert!(!old.exists(), "the old home should be gone, not copied");
        assert_eq!(
            std::fs::read(new.join(STORE_FILE)).unwrap(),
            b"the database"
        );
        assert!(new.join("models").is_dir(), "everything else moves too");
        assert!(new.join(MARKER_FILE).is_file(), "it should say what it did");
    }

    /// The failure this whole module exists to prevent, asserted directly.
    #[test]
    fn the_write_ahead_log_moves_with_the_database() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);

        relocate(&old, &new).unwrap().unwrap();

        assert_eq!(
            std::fs::read(new.join("specline.sqlite-wal")).unwrap(),
            b"committed, not checkpointed",
            "a log left behind is every transaction since the last checkpoint, lost silently"
        );
        assert!(new.join("specline.sqlite-shm").is_file());
        assert!(
            !new.join("keel.sqlite-wal").exists(),
            "the old log must not be left beside the new database"
        );
        assert!(
            !new.join("keel.sqlite.lock").exists(),
            "a lock is a claim held by a process, not a file worth carrying across"
        );
    }

    #[test]
    fn running_again_does_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);

        relocate(&old, &new).unwrap().unwrap();
        assert_eq!(
            relocate(&old, &new).unwrap(),
            None,
            "a second run is a no-op"
        );
    }

    #[test]
    fn a_fresh_install_with_no_old_store_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join(LEGACY_HOME_DIR);
        let new = dir.path().join(HOME_DIR);

        assert_eq!(relocate(&old, &new).unwrap(), None);
        assert!(!new.exists(), "nothing should be created speculatively");
    }

    /// Failure case: an existing Specline store is never replaced.
    #[test]
    fn an_existing_specline_store_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(new.join(STORE_FILE), b"what I have been writing to").unwrap();

        assert_eq!(relocate(&old, &new).unwrap(), None);
        assert_eq!(
            std::fs::read(new.join(STORE_FILE)).unwrap(),
            b"what I have been writing to",
            "the newer store must survive"
        );
        assert!(old.exists(), "and the old one is left where it is");
    }

    /// An empty directory is not a store, and must not look like one.
    #[test]
    fn an_empty_specline_directory_does_not_strand_the_old_store() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);
        std::fs::create_dir_all(&new).unwrap();

        let moved = relocate(&old, &new).unwrap().expect("it should still move");

        assert!(moved.store_renamed);
        assert_eq!(
            std::fs::read(new.join(STORE_FILE)).unwrap(),
            b"the database"
        );
    }

    /// Failure case: a store somebody is writing to is refused, not moved.
    #[test]
    fn a_store_held_by_another_process_is_refused_and_nothing_moves() {
        let dir = tempfile::tempdir().unwrap();
        let old = legacy_home(dir.path());
        let new = dir.path().join(HOME_DIR);

        let held = StoreLock::acquire(&old.join(LEGACY_STORE_FILE)).unwrap();

        let err = relocate(&old, &new).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("open for writing"),
            "the refusal should say why: {message}"
        );
        assert!(old.is_dir(), "nothing should have moved");
        assert!(!new.exists());

        drop(held);
        assert!(
            relocate(&old, &new).unwrap().is_some(),
            "and it should work once the holder lets go"
        );
    }

    /// An old home with no database in it still moves, and says it had none.
    #[test]
    fn an_old_home_with_no_database_moves_without_claiming_one() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join(LEGACY_HOME_DIR);
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("daemon.json"), b"{}").unwrap();
        let new = dir.path().join(HOME_DIR);

        let moved = relocate(&old, &new).unwrap().expect("it should move");

        assert!(!moved.store_renamed);
        assert!(new.join("daemon.json").is_file());
        assert!(!new.join(STORE_FILE).exists());
    }
}
