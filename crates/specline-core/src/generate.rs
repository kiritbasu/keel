//! Generating a project's repository files from Specline.
//!
//! Specline is the source of truth; the markdown in the repository is an output.
//! This module is the one place that turns the former into the latter.
//!
//! # Two kinds of generated file
//!
//! **Adopted files** are prose artifacts that declare a `mirror_path` — a path
//! relative to the repository root where that document belongs. `product/SPEC.md`
//! is a spec artifact whose `mirror_path` is `product/SPEC.md`. Its body *is*
//! the file, written verbatim under a generated banner, because the body
//! already carries its own heading and front matter and injecting more would
//! corrupt a document someone wrote to be read as a whole.
//!
//! **The `.specline/` mirror** ([`crate::mirror`]) covers everything else: one file
//! per spec and decision at a slugged path, plus the aggregated questions and
//! glossary. That is the SPEC §8 export, and it is for artifacts that were born
//! in Specline and have no natural home in the repository.
//!
//! On top of both sits the **tracker** ([`crate::render_status`]), written to
//! the project's `status_path`. It is task-shaped and therefore excluded from
//! the mirror by TQ-5, but it is still a generated repository file, so it
//! belongs to the same command.
//!
//! # Still one-directional
//!
//! Nothing here reads a generated file except to compare it against what would
//! be written, which is what [`Mode::Check`] is for and what keeps an accidental
//! hand edit visible instead of silently overwritten. D-3 is intact: no content
//! ever flows from a file back into the store.

use crate::render_decisions;
use crate::{
    Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result, Store, mirror,
    render_changelog, render_status,
};
use std::path::{Path, PathBuf};

/// Whether to write the files or only report what would change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Write every file that differs.
    Write,
    /// Touch nothing; report what a write would change.
    ///
    /// This is what makes a hand edit to a generated file an error someone
    /// sees rather than work someone silently loses. Run it in CI or a
    /// pre-commit hook and drift cannot survive a commit.
    Check,
}

/// What one generation run did, or would do.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GenerateReport {
    /// Repo-relative paths written, or that would be written.
    pub written: Vec<String>,
    /// Repo-relative paths already correct.
    pub unchanged: Vec<String>,
    /// Artifacts represented in the store that no generated file mentions.
    ///
    /// Not fatal, and not a file operation — a warning that the store holds
    /// something the repository cannot see.
    pub unrepresented: Vec<String>,
    /// Files a previous run produced and this one does not: removed in
    /// [`Mode::Write`], reported in [`Mode::Check`].
    ///
    /// See [`crate::mirror::MirrorReport::orphans`] for why an orphan is worse
    /// than a missing file. Counted by [`GenerateReport::is_current`], so a
    /// repository carrying one fails `--check`.
    pub orphans: Vec<String>,
    /// A mirror directory left from before the rename to Specline, if any.
    ///
    /// Reported, never removed, and deliberately *not* counted by
    /// [`GenerateReport::is_current`]: it is not drift between the store and
    /// the repository, it is a directory this mirror never wrote and cannot
    /// prove it owns. Failing `--check` on it would block every commit until
    /// somebody deleted files that generation is not itself willing to delete.
    pub legacy_mirror: Option<String>,
}

impl GenerateReport {
    /// Whether the repository already matches the store.
    ///
    /// An orphan counts as not matching. It is a file the store no longer
    /// produces, so a repository holding one has content Specline would not write
    /// — which is the same failure as a hand edit, arriving from the other
    /// direction.
    pub fn is_current(&self) -> bool {
        self.written.is_empty() && self.orphans.is_empty()
    }
}

/// Regenerate every repository file for a project.
pub fn all(
    store: &Store,
    project_id: &EntityId,
    repo_root: &Path,
    mode: Mode,
) -> Result<GenerateReport> {
    plan(store, project_id, repo_root)?.apply(mode)
}

/// Everything a generate decided, with nothing written yet.
///
/// The half that reads the store, separated from the half that touches the
/// filesystem, so a caller holding a lock on the store can drop it in between.
/// The daemon does exactly that; `all` is the two halves back to back for
/// callers that have nothing to let go of.
#[derive(Debug)]
pub struct GeneratePlan {
    /// Adopted prose, written before the mirror so that an artifact with an
    /// explicit home is not also emitted into `.specline/`.
    adopted: Vec<PlannedFile>,
    /// The `.specline/` mirror, which has its own manifest and orphan handling.
    mirror: mirror::MirrorPlan,
    /// The tracker and the decision log, rendered from rows.
    rendered: Vec<PlannedFile>,
    /// What could not be represented as a file, decided while reading.
    unrepresented: Vec<String>,
    /// A mirror directory from before the rename, noticed while planning.
    ///
    /// Carried on the plan rather than recomputed in `apply`, so that the
    /// answer comes from the same repository root the plan was built against.
    legacy_mirror: Option<String>,
}

impl GeneratePlan {
    /// Write what the plan decided, or say what writing it would do.
    ///
    /// Touches the filesystem and nothing else. No store, no lock.
    pub fn apply(self, mode: Mode) -> Result<GenerateReport> {
        let mut report = GenerateReport {
            unrepresented: self.unrepresented,
            legacy_mirror: self.legacy_mirror,
            ..GenerateReport::default()
        };
        write_planned(
            &self.adopted,
            mode,
            &mut report.written,
            &mut report.unchanged,
        )?;

        let mirror_report = self.mirror.apply(mode)?;
        report.written.extend(mirror_report.written);
        report.unchanged.extend(mirror_report.unchanged);
        report.orphans.extend(mirror_report.orphans);

        write_planned(
            &self.rendered,
            mode,
            &mut report.written,
            &mut report.unchanged,
        )?;
        Ok(report)
    }
}

/// Decide every file a project's repository should contain.
///
/// Reads the store and nothing else — no file is opened, created or compared.
pub fn plan(store: &Store, project_id: &EntityId, repo_root: &Path) -> Result<GeneratePlan> {
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Err(Error::NotFound {
            entity_type: EntityType::Project,
            id: project_id.to_string(),
        });
    };

    let mut report = GenerateReport::default();
    let legacy_mirror = crate::mirror::legacy_mirror_in(repo_root).map(|p| p.display().to_string());
    let mut adopted_files: Vec<PlannedFile> = Vec::new();
    let mut rendered: Vec<PlannedFile> = Vec::new();

    // --- Adopted prose files ---------------------------------------------
    //
    // Done before the mirror so that an artifact with an explicit home is
    // written there and *not* also into `.specline/`, which would give one
    // document two files and no answer to which is authoritative.
    let mut adopted: Vec<EntityId> = Vec::new();
    let mut adopted_paths: Vec<String> = Vec::new();
    for entity_type in [
        EntityType::Spec,
        EntityType::Decision,
        EntityType::Question,
        EntityType::Design,
    ] {
        let page = store.list(
            &EntityQuery::in_project(project_id.clone())
                .of_type(entity_type)
                .limited(5_000),
        )?;
        for entity in &page.items {
            let Some(relative) = entity.mirror_path() else {
                continue;
            };
            if !is_adopted(relative) {
                continue;
            }
            let Some(doc) = store.revision(entity.id(), None)? else {
                // An artifact can name a path before anything has been
                // written into it. Generating an empty file there would
                // destroy whatever the repository still has.
                report
                    .unrepresented
                    .push(format!("{relative} — no revision to write yet"));
                continue;
            };
            // Second layer. The value was checked on the way into the store,
            // but a row written before that check existed — or by `specline import`
            // — reaches this line all the same, and this is where it turns into
            // a write.
            adopted_files.push(PlannedFile {
                absolute: crate::safe_path::confine(repo_root, relative)?,
                relative: relative.to_owned(),
                content: adopted_file(entity, &doc.body),
                banner_counts: true,
            });
            adopted.push(entity.id().clone());
            adopted_paths.push(relative.to_owned());
        }
    }

    // --- The `.specline/` mirror ---------------------------------------------
    let mirror = mirror::plan_except(store, project_id, repo_root, &adopted)?;

    // --- The tracker ------------------------------------------------------
    if let Some(status_path) = project.status_path.as_deref() {
        // A document may already have adopted this path. Writing both would
        // make the last one to run the winner, which is how a file silently
        // loses half its content — so neither wins and the conflict is
        // reported instead. The tracker is derived and can be regenerated;
        // the prose is not, so it is the one that must not be clobbered.
        if adopted_paths.iter().any(|p| p == status_path) {
            report.unrepresented.push(format!(
                "{status_path} — the tracker was not written: a document has already adopted \
                 this path. Point the project's status_path somewhere else, or archive the \
                 document, so one thing owns the file"
            ));
        } else {
            rendered.push(PlannedFile {
                absolute: crate::safe_path::confine(repo_root, status_path)?,
                relative: status_path.to_owned(),
                content: render_status::render(store, project_id)?,
                banner_counts: true,
            });

            // The other half of the tracker. Closed work used to be rendered
            // into the status file and grew without bound there — 87% of it, at
            // the point somebody measured — so it has its own file now, beside
            // the tracker at a derived path. See `render_changelog` for why the
            // path is derived rather than a column.
            //
            // Same collision rule as everything else here: a document that has
            // adopted the path owns it, and the derived file is skipped with a
            // reason rather than clobbering prose.
            let changelog_path = render_changelog::path_beside(status_path);
            if adopted_paths.contains(&changelog_path) {
                report.unrepresented.push(format!(
                    "{changelog_path} — the changelog was not written: a document has already \
                     adopted this path. Archive the document, or point the project's status_path \
                     at another directory, so one thing owns the file"
                ));
            } else {
                rendered.push(PlannedFile {
                    absolute: crate::safe_path::confine(repo_root, &changelog_path)?,
                    relative: changelog_path,
                    content: render_changelog::render(store, project_id)?,
                    banner_counts: true,
                });
            }
        }
    }

    // --- The decision log -------------------------------------------------
    //
    // Same shape as the tracker and the same collision rule, for the same
    // reason: a decision log rendered from rows and a prose document claiming
    // the path cannot both own the file, and picking a winner silently is how
    // half a file disappears.
    if let Some(decisions_path) = project.decisions_path.as_deref() {
        if adopted_paths.iter().any(|p| p == decisions_path) {
            report.unrepresented.push(format!(
                "{decisions_path} — the decision log was not written: a document has already \
                 adopted this path. Point the project's decisions_path somewhere else, or \
                 archive the document, so one thing owns the file"
            ));
        } else {
            rendered.push(PlannedFile {
                absolute: crate::safe_path::confine(repo_root, decisions_path)?,
                relative: decisions_path.to_owned(),
                content: render_decisions::render(store, project_id)?,
                banner_counts: true,
            });
        }
    }

    Ok(GeneratePlan {
        adopted: adopted_files,
        mirror,
        rendered,
        unrepresented: report.unrepresented,
        legacy_mirror,
    })
}

/// Whether a recorded `mirror_path` means "adopt this file" or "put it in the
/// mirror".
///
/// A path under `.specline/` is the mirror's own bookkeeping, written by the mirror
/// and pointing at a slugged file it owns. Anything else is a real repository
/// path the document has adopted.
fn is_adopted(relative: &str) -> bool {
    !relative.starts_with(crate::mirror::MIRROR_PREFIX) && !relative.is_empty()
}

/// Render an adopted file: the body, verbatim, under a banner.
///
/// The banner deliberately carries no revision number. It is excluded from the
/// change comparison — otherwise a version bump alone would rewrite every file
/// on every run — which would leave a number on disk that stops matching the
/// store the first time anything else about the document changes. A stale
/// number is worse than no number.
///
/// The banner is an HTML comment so it is invisible in every markdown renderer
/// and harmless to the one reader that cannot be given a choice — Claude Code
/// loads `product/CLAUDE.md` from disk on every session and will read whatever
/// is at the top of it.
fn adopted_file(entity: &Entity, body: &str) -> String {
    format!(
        "<!-- specline:generated {} {}\n     \
         Specline is the source of truth for this file. Edit it there — in the app, or by asking \
         Claude — and regenerate.\n     \
         An edit made here is overwritten on the next `specline generate`. -->\n\n{}\n",
        entity.entity_type().as_str(),
        entity.id(),
        body.trim_end()
    )
}

/// A file the generator has decided on, with nothing written yet.
///
/// Deciding and writing are separate phases so that the daemon can let go of
/// the store between them. Rendering a project's files reads the whole store
/// and then writes a few dozen small files, and holding the write lock across
/// the second half made a `specline generate` block every other request — including
/// the health probe the CLI uses to decide whether a daemon is even there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    /// Where it goes, already confined to the repository root.
    pub absolute: PathBuf,
    /// Its path relative to the root, which is what reports name.
    pub relative: String,
    /// What it should contain.
    pub content: String,
    /// Whether the presence of a generated banner is part of "has this
    /// changed".
    ///
    /// True for adopted prose and the rendered documents: a file that has never
    /// been generated has no banner, and leaving it alone would mean the first
    /// switch-over never marks it as generated. False for the mirror, whose
    /// files always carry a header and whose comparison is only about the body.
    pub banner_counts: bool,
}

/// Write the files a plan decided on, or record what writing them would do.
///
/// The whole filesystem half of a generate, and the only part that needs to run
/// with nothing held.
pub(crate) fn write_planned(
    files: &[PlannedFile],
    mode: Mode,
    written: &mut Vec<String>,
    unchanged: &mut Vec<String>,
) -> Result<()> {
    for file in files {
        // Compare ignoring the banner, which carries a generation timestamp and
        // would otherwise make every file look changed every time.
        let existing = std::fs::read_to_string(&file.absolute).ok();
        if let Some(old) = &existing
            && strip_banner(old) == strip_banner(&file.content)
            && (!file.banner_counts || has_banner(old) == has_banner(&file.content))
        {
            unchanged.push(file.relative.clone());
            continue;
        }

        written.push(file.relative.clone());
        if mode == Mode::Check {
            continue;
        }

        // Atomic, because one of the files this writes is `product/CLAUDE.md` —
        // loaded at the start of every Claude Code session, so a torn write
        // silently removes the second half of the standing contract.
        crate::atomic::write(&file.absolute, &file.content)?;
    }
    Ok(())
}

/// Whether a file already declares itself generated.
fn has_banner(content: &str) -> bool {
    content.trim_start().starts_with("<!-- specline:generated")
}

/// Drop a leading generated banner, so a version bump in the comment does not
/// by itself make a file look changed.
pub(crate) fn strip_banner(content: &str) -> String {
    let mut out = String::new();
    let mut in_banner = false;
    for line in content.lines() {
        if line.trim_start().starts_with("<!-- specline:generated") {
            in_banner = !line.contains("-->");
            continue;
        }
        if in_banner {
            in_banner = !line.contains("-->");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out.trim().to_owned()
}

/// `strip_banner`, exposed for the render-stability test.
///
/// Drop a leading generated banner, for callers outside this module.
///
/// Was `strip_banner_for_test`, which stopped being true: the pre-commit
/// check's "is this file already up to date" comparison needs exactly this
/// rule, because the banner carries a generation timestamp and a byte
/// comparison would never hold. A name that says "test only" on something the
/// product depends on is worse than no name.
pub fn strip_banner_public(content: &str) -> String {
    strip_banner(content)
}

/// Where a project's repository lives, if it has a checkout.
pub fn repo_root(project: &crate::Project) -> Option<PathBuf> {
    project.root_path.as_ref().map(PathBuf::from)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_path_under_the_mirror_dir_belongs_to_the_mirror_not_to_adoption() {
        assert!(is_adopted("product/SPEC.md"));
        assert!(is_adopted("docs/architecture/overview.md"));
        assert!(!is_adopted(".specline/specs/storage.md"));
        assert!(!is_adopted(""));
    }

    #[test]
    fn the_banner_does_not_count_as_a_change() {
        let a = "<!-- specline:generated spec spc_1 v1\n     blah -->\n\n# Title\n\nBody\n";
        let b =
            "<!-- specline:generated spec spc_1 v9\n     different blah -->\n\n# Title\n\nBody\n";
        assert_eq!(strip_banner(a), strip_banner(b));

        let c = "<!-- specline:generated spec spc_1 v1\n     blah -->\n\n# Title\n\nEdited\n";
        assert_ne!(strip_banner(a), strip_banner(c));
    }

    #[test]
    fn a_file_that_has_never_been_generated_needs_writing_once() {
        // Otherwise the switch-over is invisible: the prose already matches,
        // so nothing is written, so nothing ever says the file is generated
        // and the next person edits it by hand.
        assert!(!has_banner("# Title\n\nBody\n"));
        assert!(has_banner(
            "<!-- specline:generated spec spc_1 v1 -->\n\n# Title\n"
        ));
    }

    #[test]
    fn a_file_with_no_banner_still_compares() {
        // The first generation of an adopted file overwrites a hand-written
        // one that has no banner at all. It must compare equal when the prose
        // matches, or the very first run reports every file as changed.
        assert_eq!(
            strip_banner("# Title\n\nBody\n"),
            strip_banner("# Title\n\nBody")
        );
    }
}
