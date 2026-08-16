//! Writing a generated file so that it is never half-written.
//!
//! Every generated markdown file was written by truncating it and then writing
//! the new content, which is two operations with a window between them. A disk
//! that fills up or a process killed inside that window leaves a *prefix* of
//! the new content on disk — a file that opens, reads as plausible, and stops
//! mid-sentence.
//!
//! One of those files is `product/CLAUDE.md`. Claude Code loads it at the start
//! of every session, so a torn write silently removes the second half of the
//! standing contract, and nothing anywhere says so. The contract simply gets
//! shorter and every session afterwards follows the part that survived.
//!
//! The fix is the ordinary one: write a temporary file in the same directory,
//! flush it to disk, then rename it over the target. `rename` within a
//! directory is atomic on every platform Specline runs on — a reader sees either
//! the old file or the new one, never a mixture — and "in the same directory"
//! is load-bearing, because a rename across filesystems is a copy and a delete
//! and has the window back.

use crate::{Error, Result};
use std::io::Write as _;
use std::path::Path;

/// Write `content` to `path`, atomically.
///
/// Creates the parent directory if it is missing, which is what every caller
/// wanted anyway — a generated tree includes directories that do not exist yet.
///
/// The temporary file is named from the process id so two Keels generating into
/// one repository cannot collide on it, and it is removed on the failure paths
/// so a full disk does not also leave litter.
pub fn write(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)
            .map_err(Error::io(format!("create {}", parent.display())))?;
    }

    let temp = temp_path(path);

    // A block, so the file is closed before the rename. Windows refuses to
    // rename over a handle that is still open, and the failure would be
    // platform-specific and rare — the worst combination.
    {
        let mut file = std::fs::File::create(&temp)
            .map_err(Error::io(format!("create {}", temp.display())))?;

        if let Err(e) = file.write_all(content.as_bytes()) {
            let _ = std::fs::remove_file(&temp);
            return Err(Error::io(format!("write {}", temp.display()))(e));
        }

        // Flush before the rename, not after. Without this the rename can
        // reach the disk before the bytes do, and a power cut leaves a file
        // with the right name and no content — which is the same torn write
        // wearing a different hat.
        if let Err(e) = file.sync_all() {
            let _ = std::fs::remove_file(&temp);
            return Err(Error::io(format!("flush {}", temp.display()))(e));
        }
    }

    if let Err(e) = std::fs::rename(&temp, path) {
        let _ = std::fs::remove_file(&temp);
        return Err(Error::io(format!(
            "move {} into place at {}",
            temp.display(),
            path.display()
        ))(e));
    }

    Ok(())
}

/// A sibling path for the temporary file.
///
/// In the same directory as the target, because a rename across filesystems is
/// a copy and a delete — which is exactly the non-atomic write this replaces.
fn temp_path(path: &Path) -> std::path::PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "specline".to_owned());
    let temp = format!(".{name}.keel-{}.tmp", std::process::id());
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(temp),
        _ => std::path::PathBuf::from(temp),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_written_and_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");

        write(&path, "the first contract").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "the first contract"
        );

        write(&path, "the second contract").unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "the second contract"
        );
    }

    #[test]
    fn missing_directories_are_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deep").join("nested").join("SPEC.md");
        write(&path, "content").unwrap();
        assert!(path.is_file());
    }

    /// No litter left behind, so a repository does not accumulate `.tmp` files
    /// that look like something went wrong.
    #[test]
    fn the_temporary_file_does_not_survive() {
        let dir = tempfile::tempdir().unwrap();
        write(&dir.path().join("STATUS.md"), "content").unwrap();

        let leftovers: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(std::result::Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("keel-") && n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "left behind {leftovers:?}");
    }

    /// The property the whole module exists for: a failed write leaves the
    /// previous content, not a prefix of the new one.
    #[test]
    fn a_write_that_cannot_finish_leaves_the_old_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("CLAUDE.md");
        write(&path, "the standing contract, in full").unwrap();

        // A directory where the temp file wants to be is the cheapest way to
        // make `File::create` fail without a full disk.
        std::fs::create_dir(temp_path(&path)).unwrap();
        let failed = write(&path, "a replacement that will not land");

        assert!(failed.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "the standing contract, in full",
            "a failed write must leave the old content, never a prefix of the new"
        );
    }

    #[test]
    fn the_temporary_path_is_a_sibling_of_the_target() {
        let temp = temp_path(Path::new("/repo/product/CLAUDE.md"));
        assert_eq!(
            temp.parent().unwrap(),
            Path::new("/repo/product"),
            "a rename across filesystems is a copy and a delete, which is the \
             non-atomic write this replaces"
        );
    }
}
