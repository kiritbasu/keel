//! One writer at a time, enforced by the operating system.
//!
//! Hard constraint 1 says the daemon owns the write path. Under DuckDB that was
//! a rule *and* a mechanism — the engine refused a second writer outright. SQLite
//! in WAL mode does not: a second process opens the same file and writes to it
//! quite happily, and nothing anywhere says so.
//!
//! On 2026-08-13 that stopped being theoretical. A second daemon was started
//! against the live store — `--bind` and `--embeddings` passed, `--home`
//! forgotten — and it applied a schema migration while the first was serving.
//! It was not a rogue writer skipping steps; it went through this crate
//! correctly. It was a legitimate writer that should not have been a second one,
//! and the only thing that noticed was `/api/health` reporting a schema number
//! that disagreed with what the store should have been at. Had the numbers
//! happened to agree, nothing would have looked wrong.
//!
//! # Why a lock file is safe here when it usually is not
//!
//! TQ-36 rejected this for a good reason: "a stale lock after a crash is a store
//! nobody can open, which is worse than the problem." That is exactly right for
//! a PID file or a claimed row in a table, and exactly wrong for an advisory
//! lock, because the kernel holds it against an open file descriptor rather than
//! against anything written down. Close the descriptor and the lock is gone —
//! including when the process is `SIGKILL`ed, panics, or the machine loses
//! power. Measured before B-59 was written rather than assumed:
//!
//! ```text
//! --- while the holder is alive ---
//! REFUSED — still held: "WouldBlock"
//! --- after SIGKILL of the holder ---
//! ACQUIRED — the lock was free
//! ```
//!
//! There is no stale state, so there is no repair command to write and no
//! `--unlock` flag for someone to reach for at the worst possible moment.
//!
//! # What it does not cover
//!
//! Advisory locks are unreliable on network and synchronised filesystems, where
//! the guarantee depends on the server rather than the kernel. `specline doctor`
//! already warns when the store sits in one, and that is the same population —
//! anyone this fails for has a louder problem already.
//!
//! It is also, as the name says, *advisory*. It stops a second `specline-core` from
//! opening the store for writing. It does nothing about `sqlite3` on the command
//! line, which is the right scope: this guards against the accident, not against
//! someone who has decided to edit the database by hand.

use crate::{Error, Result};
use std::fs::File;
use std::path::{Path, PathBuf};

/// Exclusive claim on a store, released when this is dropped.
///
/// Held by whoever opened the store for writing — the daemon for its whole
/// lifetime, a CLI command for the length of the command. Dropping it releases
/// the claim, and so does the process ending by any means.
#[derive(Debug)]
pub struct StoreLock {
    /// The open descriptor *is* the lock. Named with an underscore because
    /// nothing reads it and everything depends on it not being dropped — which
    /// is the kind of field a tidying pass deletes.
    _file: File,
    path: PathBuf,
}

impl StoreLock {
    /// Claim the store, or say who has it.
    ///
    /// Fails immediately rather than waiting. A caller blocked on this would be
    /// a daemon that appears to hang at startup, and the honest answer — "something
    /// else is already writing to this" — is available at once.
    pub fn acquire(store_path: &Path) -> Result<Self> {
        let path = lock_path(store_path);
        let file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(Error::io(format!(
                "open the lock file at {}",
                path.display()
            )))?;

        match file.try_lock() {
            Ok(()) => Ok(StoreLock { _file: file, path }),
            Err(_) => Err(Error::Invariant {
                operation: format!("open {} for writing", store_path.display()),
                problem: format!(
                    "another process already has this store open for writing.\n\n\
                     Only one writer at a time: six of the seven steps in a Specline write — \
                     validation, provenance, the event, the revision, the embedding, the \
                     index — are this crate's job rather than the database's, and two \
                     processes doing them at once agree about none of them.\n\n\
                     It is almost always a daemon. Check with `specline doctor`, and if you meant \
                     to work on a different store, pass `--home`.\n\n\
                     The claim is held by a running process, not by {}, so there is nothing \
                     to delete: stop the other process and it is released.",
                    path.display()
                ),
            }),
        }
    }

    /// The lock file backing this claim.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// The lock file that belongs to a store.
///
/// Beside the database rather than inside it, and a file of its own rather than
/// the database itself: SQLite takes its own locks on that file through a
/// different mechanism, and stacking a second scheme on the same inode is the
/// sort of thing that works until it is one platform away from where it was
/// tested.
pub fn lock_path(store_path: &Path) -> PathBuf {
    let mut name = store_path.as_os_str().to_os_string();
    name.push(".lock");
    PathBuf::from(name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_lock_file_sits_beside_the_store() {
        let p = lock_path(Path::new("/tmp/keel/keel.sqlite"));
        assert_eq!(p, Path::new("/tmp/keel/keel.sqlite.lock"));
    }

    #[test]
    fn a_second_claim_is_refused_and_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("keel.sqlite");

        let first = StoreLock::acquire(&store).unwrap();
        let err = StoreLock::acquire(&store).unwrap_err().to_string();
        assert!(
            err.contains("already has this store open for writing"),
            "{err}"
        );
        // The remedy has to be actionable: someone reading this needs to know it
        // is a process and not a file to delete.
        assert!(err.contains("nothing to delete"), "{err}");
        drop(first);
    }

    #[test]
    fn dropping_the_claim_releases_it() {
        let dir = tempfile::tempdir().unwrap();
        let store = dir.path().join("keel.sqlite");

        let first = StoreLock::acquire(&store).unwrap();
        drop(first);
        // If this fails the lock leaked, and every future open of this store
        // fails until the machine is rebooted.
        StoreLock::acquire(&store).expect("the claim should have been released");
    }

    #[test]
    fn two_different_stores_do_not_contend() {
        let dir = tempfile::tempdir().unwrap();
        let a = StoreLock::acquire(&dir.path().join("a.sqlite")).unwrap();
        let b = StoreLock::acquire(&dir.path().join("b.sqlite")).unwrap();
        drop((a, b));
    }
}
