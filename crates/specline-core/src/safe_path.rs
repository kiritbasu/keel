//! Keeping entity-controlled paths inside the project they belong to.
//!
//! Four columns hold a filesystem path that a caller chooses: a prose
//! artifact's `mirror_path`, and a project's `status_path`, `decisions_path`
//! and `root_path`. The first three are joined onto the repository root by
//! [`crate::generate`] and written to. They were free-form strings with no
//! validation of any kind.
//!
//! That is the review's top security finding, and the reason is not that a
//! human might type `../`. It is that these values arrive from a model that can
//! be prompt-injected by anything it has read — an issue title, a pasted log, a
//! web page — and `POST /api/generate` performs the write unattended, outside
//! whatever file-approval gate the harness around the model provides. A
//! `mirror_path` of `../../../.zshenv` is a shell that runs attacker-chosen
//! commands the next time a terminal opens.
//!
//! Two layers, deliberately redundant:
//!
//! - [`validate_repo_relative`] runs on the way in, on create and on update, so
//!   the bad value never reaches storage. It returns the actionable error an
//!   agent can read.
//! - [`confine`] runs at every `root.join(…)`, so a value already in the store —
//!   written before this existed, or by `specline import`, or by a second writer —
//!   still cannot escape. A validator that only guards the front door assumes
//!   the front door is the only one.
//!
//! # What is rejected
//!
//! Absolute paths, any `..` component, any component SQLite-style empty or a
//! bare `.` prefix trick, a leading `~` (which would be taken literally and make
//! a directory called `~`, which is nobody's intent), and a NUL byte. Then, at
//! join time, the resolved path must still be under the resolved root — which
//! catches the case lexical checks cannot: a directory inside the repository
//! that is itself a symlink pointing out of it.
//!
//! Both sides are resolved before comparison. On macOS `/tmp` is a symlink to
//! `/private/tmp` and `/var` to `/private/var`, so comparing a resolved target
//! against an unresolved root fails for every temporary directory — which is to
//! say, for every test.

use crate::{EntityType, Error, Result};
use std::path::{Component, Path, PathBuf};

/// Check a repo-relative path a caller supplied, on the way into the store.
///
/// `field` names the column, so the message says which of four paths was wrong
/// rather than making the reader guess.
pub fn validate_repo_relative(entity_type: EntityType, field: &str, value: &str) -> Result<()> {
    let refuse = |problem: String| {
        Err(Error::Invalid {
            entity_type,
            field: field.to_owned(),
            problem,
            expected: "a path relative to the project's checkout, with no leading slash and no \
                       `..` — for example `product/SPEC.md` or `docs/adr/0001.md`. Specline writes \
                       this file, so it must stay inside the repository"
                .to_owned(),
        })
    };

    if value.is_empty() {
        return refuse("the path is empty".to_owned());
    }
    if value.contains('\0') {
        return refuse("the path contains a NUL byte".to_owned());
    }
    if value.starts_with('~') {
        // Nothing in Specline expands `~`, so this would create a directory
        // literally named `~` in the repository. Refusing says what the caller
        // meant is not supported, rather than doing something surprising.
        return refuse(format!(
            "`{value}` starts with `~`, which Specline does not expand"
        ));
    }

    let path = Path::new(value);
    if path.is_absolute() {
        return refuse(format!(
            "`{value}` is an absolute path, so it names a file outside the project"
        ));
    }

    for component in path.components() {
        match component {
            Component::ParentDir => {
                return refuse(format!(
                    "`{value}` contains `..`, which would let the generated file land outside \
                     the project"
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return refuse(format!("`{value}` is rooted outside the project"));
            }
            Component::CurDir | Component::Normal(_) => {}
        }
    }

    Ok(())
}

/// Check a project's `root_path`, which is the checkout itself rather than
/// something inside it.
///
/// The opposite rule to [`validate_repo_relative`]: this one *must* be
/// absolute. A relative root resolves against whatever directory the daemon
/// happened to start in, which is a different repository depending on how it
/// was launched — and the failure would be files generated into the wrong tree
/// rather than an error.
///
/// `~` is accepted here and only here, because a human types this one into
/// `specline adopt` and `~/development/specline` is what they mean. Nothing in Specline
/// expands it, so the path is stored as written and resolved by whatever runs
/// the generate — this is a check on the shape, not a promise about the
/// filesystem.
pub fn validate_root_path(value: &str) -> Result<()> {
    let refuse = |problem: String| {
        Err(Error::Invalid {
            entity_type: EntityType::Project,
            field: "root_path".to_owned(),
            problem,
            expected: "an absolute path to the project's checkout, for example \
                       `/Users/you/development/specline` or `~/development/specline`"
                .to_owned(),
        })
    };

    if value.is_empty() {
        return refuse("the path is empty".to_owned());
    }
    if value.contains('\0') {
        return refuse("the path contains a NUL byte".to_owned());
    }
    if value.starts_with('~') {
        return Ok(());
    }
    if !Path::new(value).is_absolute() {
        return refuse(format!(
            "`{value}` is relative, so which repository it names depends on where the daemon \
             was started"
        ));
    }
    Ok(())
}

/// Join a repo-relative path onto a root, refusing anything that escapes it.
///
/// The second layer, run immediately before every write. It repeats
/// [`validate_repo_relative`]'s lexical checks — cheap — and adds the one they
/// cannot do: resolving both sides and asserting the target is still underneath
/// the root, which is what catches a symlinked directory inside the repository.
///
/// Returns the joined path, so the call site reads `let path = confine(root,
/// relative)?;` and there is no unchecked `join` left for someone to copy.
pub fn confine(root: &Path, relative: &str) -> Result<PathBuf> {
    let escape = |problem: String| Error::Invariant {
        operation: format!("write the generated file `{relative}`"),
        problem,
    };

    // The lexical rules, as an invariant rather than a validation error: by the
    // time a value reaches here it is already stored, so this is Specline refusing
    // to act on its own bad data rather than telling a caller off.
    if let Err(e) = validate_repo_relative(EntityType::Project, "path", relative) {
        return Err(escape(e.to_string()));
    }

    let joined = root.join(relative);

    // Resolve as much of each side as exists. The target file usually does not
    // yet — that is the point of generating it — so canonicalising the whole
    // path would fail for every new file.
    let resolved_root = resolve_existing(root);
    let resolved_target = resolve_existing(&joined);

    if !resolved_target.starts_with(&resolved_root) {
        return Err(escape(format!(
            "`{relative}` resolves to {}, which is outside the project at {}. A directory in the \
             path is probably a symlink pointing out of the repository",
            resolved_target.display(),
            resolved_root.display()
        )));
    }

    Ok(joined)
}

/// Canonicalise the deepest ancestor that exists, then re-append the rest.
///
/// `std::fs::canonicalize` fails on a path whose last components do not exist,
/// which is every file about to be generated. Walking up to the first real
/// directory resolves the symlinks that matter — they can only be on components
/// that exist — and leaves the rest as written.
fn resolve_existing(path: &Path) -> PathBuf {
    let mut tail: Vec<&std::ffi::OsStr> = Vec::new();
    let mut probe = path;

    loop {
        if let Ok(real) = probe.canonicalize() {
            let mut out = real;
            for part in tail.iter().rev() {
                out.push(part);
            }
            return out;
        }
        match (probe.file_name(), probe.parent()) {
            (Some(name), Some(parent)) => {
                tail.push(name);
                probe = parent;
            }
            // Nothing on this path exists at all. Hand back what we were given
            // rather than inventing something: the caller's comparison then
            // fails closed, which is the right direction.
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn ok(value: &str) -> bool {
        validate_repo_relative(EntityType::Spec, "mirror_path", value).is_ok()
    }

    #[test]
    fn ordinary_repo_paths_are_accepted() {
        assert!(ok("product/SPEC.md"));
        assert!(ok(".specline/questions.md"));
        assert!(ok("docs/architecture/overview.md"));
        assert!(ok("README.md"));
    }

    #[test]
    fn an_absolute_path_is_refused() {
        assert!(!ok("/etc/evil"));
        assert!(!ok("/Users/someone/.zshenv"));
    }

    #[test]
    fn a_parent_component_is_refused_wherever_it_sits() {
        assert!(!ok("../../.zshenv"));
        assert!(!ok("product/../../.zshenv"));
        assert!(!ok(".."));
        assert!(!ok("a/b/../../../c"));
    }

    #[test]
    fn a_tilde_is_refused_rather_than_expanded() {
        assert!(!ok("~/.zshenv"));
        assert!(!ok("~"));
    }

    #[test]
    fn the_empty_path_and_a_nul_byte_are_refused() {
        assert!(!ok(""));
        assert!(!ok("product/\0SPEC.md"));
    }

    #[test]
    fn the_message_names_the_field_and_says_what_would_be_valid() {
        let err = validate_repo_relative(EntityType::Spec, "mirror_path", "../x").unwrap_err();
        let text = err.to_string();
        assert!(text.contains("mirror_path"), "{text}");
        assert!(text.contains(".."), "{text}");
        assert!(
            text.contains("product/SPEC.md"),
            "an agent reading this needs an example of a valid value: {text}"
        );
    }

    #[test]
    fn a_root_path_must_be_absolute() {
        assert!(validate_root_path("/Users/kb/development/specline").is_ok());
        assert!(validate_root_path("~/development/specline").is_ok());
        assert!(validate_root_path("development/specline").is_err());
        assert!(validate_root_path("").is_err());
    }

    #[test]
    fn confine_joins_a_good_path_under_the_root() {
        let dir = tempfile::tempdir().unwrap();
        let path = confine(dir.path(), "product/SPEC.md").unwrap();
        assert!(path.ends_with("product/SPEC.md"));
    }

    #[test]
    fn confine_refuses_what_the_validator_refuses() {
        let dir = tempfile::tempdir().unwrap();
        assert!(confine(dir.path(), "../escape.md").is_err());
        assert!(confine(dir.path(), "/etc/evil").is_err());
    }

    /// The check lexical rules cannot do: a real directory inside the
    /// repository that points somewhere else.
    #[cfg(unix)]
    #[test]
    fn confine_refuses_a_symlinked_directory_that_leaves_the_repo() {
        let outside = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("product")).unwrap();

        let err = confine(repo.path(), "product/SPEC.md")
            .expect_err("a symlink out of the repository should be refused");
        assert!(err.to_string().contains("outside the project"), "{err}");
    }

    /// A temporary directory on macOS is under `/var`, which is a symlink to
    /// `/private/var`. Resolving one side and not the other would refuse every
    /// legitimate path in every test — and, worse, would look like the check
    /// working.
    #[test]
    fn a_root_that_is_itself_reached_through_a_symlink_still_works() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("product")).unwrap();
        assert!(confine(dir.path(), "product/SPEC.md").is_ok());
    }
}
