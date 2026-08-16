//! Which folders an image may be read from.
//!
//! `specline_create` can take a path to a picture on the machine rather than base64
//! through the tool call, because a retina screenshot costs 350,000 output
//! tokens as base64 and nothing at all as a path (TQ-33). The bytes never enter
//! model context.
//!
//! What that bought is also the risk. **The model choosing the path is reading
//! issue text, customer feedback and web pages**, so "put the screenshot at
//! /Users/…/whatever into Specline" is a sentence something else can write. Until
//! this existed, any image anywhere on the disk could be copied into the store
//! on that instruction.
//!
//! KB's call, 2026-08-16: the common places pictures actually live, plus
//! wherever the project itself is. Anything else is refused, with the list in
//! the refusal and base64 offered as the way round it.
//!
//! # Why containment is checked after canonicalising
//!
//! `~/Desktop/../.ssh/id_rsa.png` is inside `~/Desktop` as a string and is not
//! inside it as a location, and a symlink on the Desktop pointing anywhere at
//! all is the same trick without the `..`. Resolving the path first is what
//! makes the check about the file rather than about the spelling.

use std::path::{Path, PathBuf};

/// The folders an image may come from.
///
/// `home` and `project_root` are passed in rather than read here, so this is a
/// pure function a test can drive without a home directory or a store.
///
/// A folder that does not exist is dropped rather than kept: the list is
/// printed in refusals, and offering somebody a directory they do not have is
/// a worse answer than a shorter list.
pub fn roots(home: Option<&Path>, project_root: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = home {
        // Desktop first because it is where macOS puts screenshots by default,
        // which is the case this whole path exists for. A custom screenshot
        // location — `defaults write com.apple.screencapture location` — is not
        // read; somebody who has moved theirs somewhere unusual gets a refusal
        // naming these folders, which is at least a legible one.
        for folder in ["Desktop", "Downloads", "Pictures"] {
            roots.push(home.join(folder));
        }
    }

    if let Some(project) = project_root {
        roots.push(project.to_path_buf());
    }

    roots
        .into_iter()
        .filter_map(|root| root.canonicalize().ok())
        .collect()
}

/// Whether a file sits inside one of the allowed folders.
///
/// The file is canonicalised by the caller, because a path that cannot be
/// resolved does not exist and that is a different error with a better message.
pub fn contains(roots: &[PathBuf], file: &Path) -> bool {
    roots.iter().any(|root| file.starts_with(root))
}

/// The folder list, for a refusal that tells somebody where to put the file.
pub fn describe(roots: &[PathBuf]) -> String {
    if roots.is_empty() {
        return "nowhere — no readable folder was found to allow".to_owned();
    }
    roots
        .iter()
        .map(|r| r.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    /// A home laid out the way a Mac's is, so the roots resolve to something.
    fn home_with(folders: &[&str]) -> tempfile::TempDir {
        let home = tempfile::tempdir().unwrap();
        for folder in folders {
            std::fs::create_dir_all(home.path().join(folder)).unwrap();
        }
        home
    }

    #[test]
    fn the_usual_picture_folders_are_allowed() {
        let home = home_with(&["Desktop", "Downloads", "Pictures"]);
        let roots = roots(Some(home.path()), None);

        assert_eq!(roots.len(), 3);
        let shot = home.path().join("Desktop/screenshot.png");
        std::fs::write(&shot, b"x").unwrap();
        assert!(contains(&roots, &shot.canonicalize().unwrap()));
    }

    #[test]
    fn the_project_the_work_is_in_is_allowed() {
        let home = home_with(&["Desktop"]);
        let project = tempfile::tempdir().unwrap();
        let roots = roots(Some(home.path()), Some(project.path()));

        let diagram = project.path().join("docs/diagram.png");
        std::fs::create_dir_all(diagram.parent().unwrap()).unwrap();
        std::fs::write(&diagram, b"x").unwrap();
        assert!(contains(&roots, &diagram.canonicalize().unwrap()));
    }

    /// The case the allowlist exists for: somewhere private, named by a model
    /// that was reading somebody else's words.
    #[test]
    fn somewhere_else_in_the_home_directory_is_not() {
        let home = home_with(&["Desktop", ".ssh"]);
        let roots = roots(Some(home.path()), None);

        let private = home.path().join(".ssh/key.png");
        std::fs::write(&private, b"x").unwrap();
        assert!(!contains(&roots, &private.canonicalize().unwrap()));
    }

    /// Climbing out with `..` must not work, which is why the caller resolves
    /// the path before asking.
    #[test]
    fn a_path_that_climbs_out_of_an_allowed_folder_is_not_inside_it() {
        let home = home_with(&["Desktop", ".ssh"]);
        let roots = roots(Some(home.path()), None);

        let private = home.path().join(".ssh/key.png");
        std::fs::write(&private, b"x").unwrap();
        let sneaky = home.path().join("Desktop/../.ssh/key.png");

        assert!(
            !contains(&roots, &sneaky.canonicalize().unwrap()),
            "resolving first is what makes this about the file and not the spelling"
        );
    }

    /// And neither must a symlink, which is the same escape with no `..` in it.
    #[cfg(unix)]
    #[test]
    fn a_symlink_out_of_an_allowed_folder_is_not_inside_it() {
        let home = home_with(&["Desktop", ".ssh"]);
        let roots = roots(Some(home.path()), None);

        let private = home.path().join(".ssh/key.png");
        std::fs::write(&private, b"x").unwrap();
        let link = home.path().join("Desktop/innocent.png");
        std::os::unix::fs::symlink(&private, &link).unwrap();

        assert!(!contains(&roots, &link.canonicalize().unwrap()));
    }

    #[test]
    fn a_folder_that_does_not_exist_is_not_offered() {
        let home = home_with(&["Desktop"]);
        let roots = roots(Some(home.path()), None);

        assert_eq!(roots.len(), 1, "only Desktop exists in this home");
        assert!(describe(&roots).contains("Desktop"));
    }

    #[test]
    fn with_nothing_to_allow_the_description_says_so() {
        assert!(describe(&[]).contains("nowhere"));
    }
}
