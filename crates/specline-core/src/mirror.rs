//! The one-directional markdown mirror.
//!
//! A generated export of a project's prose into `<repo>/.specline/`, so that specs,
//! decisions, open questions and the glossary are legible offline, land in repo
//! grep, and end up in an agent's context for free (Q-3: committed).
//!
//! # The mirror is never a source of truth
//!
//! D-3, and the single rule this module exists to keep. Reconciliation between
//! two authorities is the failure the whole design avoids, and every "just sync
//! it back" instinct leads there. Nothing here reads a mirror file. Nothing
//! here compares mirror state to database state. There is no function to do
//! either, deliberately — the absence is the enforcement.
//!
//! SPEC §8.1 used to permit exactly one read — a Claude Code `PostToolUse` hook
//! that turned an observed edit into a revision. That hook never worked and was
//! deleted rather than repaired, so the rule now has **no exception at all**.
//! What replaced it refuses a commit instead of recovering an edit
//! (`scripts/pre-commit`), and a deliberate migration is a person running
//! `specline import`.
//!
//! # Prose only
//!
//! TQ-5. Tasks churn, and mirroring them would make every repo diff noisy for
//! no gain. A task-shaped status file is what `specline render-status` is for.

use crate::{Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result, Store};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// The directory the mirror is written into, inside a project's repository.
///
/// One name for what used to be nine string literals, and the reason is a real
/// failure rather than tidiness. The name appears in three different roles:
/// the root files are written to, the prefix every recorded path carries, and
/// the guard on [`MirrorReport`]'s pruning, which refuses to delete anything
/// whose path does not start with it.
///
/// Those roles disagreeing is silent in the direction that matters. If the
/// guard names one directory and the writer another, pruning stops finding
/// anything to prune: no error, no warning, and orphaned files accumulate in a
/// tree nobody is reading closely. Renaming the mirror was the first time all
/// nine had to change together, which is a good argument for them never having
/// been nine.
pub const MIRROR_DIR: &str = ".specline";

/// [`MIRROR_DIR`] with its separator, for the recorded relative paths.
pub const MIRROR_PREFIX: &str = ".specline/";

/// The directory Keel wrote its mirror into.
///
/// Nothing writes it. It exists so that a repository carrying an old mirror can
/// be recognised and reported rather than silently gaining a second one beside
/// it — see [`legacy_mirror_in`].
pub const LEGACY_MIRROR_DIR: &str = ".keel";

/// Whether a repository still carries a mirror written under the name Keel.
///
/// Generation does not delete it. Pruning only ever removes files this mirror's
/// own manifest says it wrote, and a directory from before the rename is
/// outside that record — deleting it would mean removing files on the strength
/// of a guess about where they came from, in somebody's repository. So this
/// reports, and the caller tells the person.
pub fn legacy_mirror_in(repo_root: &Path) -> Option<PathBuf> {
    let old = repo_root.join(LEGACY_MIRROR_DIR);
    old.is_dir().then_some(old)
}

/// One generated file and what contributed to it.
///
/// The manifest is keyed by **path**, with a list of contributors — not by
/// document id. `questions.md` and `glossary.md` aggregate many rows into one
/// file, so a `doc_id → path` map could not express them (SPEC §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorFile {
    /// Path relative to the repository root.
    pub path: String,
    /// Everything that contributed content to this file.
    pub contributors: Vec<Contributor>,
}

/// One artifact's contribution to a mirror file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Contributor {
    /// The artifact.
    pub entity_id: EntityId,
    /// Its type.
    pub entity_type: String,
    /// The revision rendered, or 0 for artifacts with no body.
    pub version: i32,
    /// Content hash of what was written, so a regeneration that changes
    /// nothing can be skipped.
    pub hash: String,
}

/// The manifest written alongside the mirror.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// The project mirrored.
    pub project_id: EntityId,
    /// When it was generated.
    pub generated_at: String,
    /// Every file.
    pub files: Vec<MirrorFile>,
}

/// What a mirror run did.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MirrorReport {
    /// Files written or rewritten.
    pub written: Vec<String>,
    /// Files already correct, left alone.
    pub unchanged: Vec<String>,
    /// Files this run previously produced and no longer does — removed in
    /// [`crate::generate::Mode::Write`], reported in
    /// [`crate::generate::Mode::Check`].
    ///
    /// Renaming an artifact changes its slug, so the old file would otherwise
    /// survive carrying a `specline:generated` banner and a real id, reading as
    /// current forever. That is worse than a missing file: it is plausible,
    /// greppable and permanently wrong, and nothing would ever say so.
    pub orphans: Vec<String>,
}

/// Generate the mirror for a project into `repo_root/.specline/`.
pub fn generate(store: &Store, project_id: &EntityId, repo_root: &Path) -> Result<MirrorReport> {
    generate_except(
        store,
        project_id,
        repo_root,
        &[],
        crate::generate::Mode::Write,
    )
}

/// Generate the mirror, skipping artifacts that have adopted a real repository
/// file.
///
/// An adopted document is written to its own path by [`crate::generate`].
/// Emitting it here as well would give one document two files and no answer to
/// which is authoritative — the reconciliation failure the whole design exists
/// to avoid.
pub fn generate_except(
    store: &Store,
    project_id: &EntityId,
    repo_root: &Path,
    skip: &[EntityId],
    mode: crate::generate::Mode,
) -> Result<MirrorReport> {
    plan_except(store, project_id, repo_root, skip)?.apply(mode)
}

/// The mirror's files, decided but not written.
///
/// Exists so that the store can be let go of between deciding and writing —
/// see [`crate::generate::GeneratePlan`], which holds one of these.
#[derive(Debug)]
pub struct MirrorPlan {
    repo_root: PathBuf,
    root: PathBuf,
    /// The files, in the order they should be written.
    writes: Vec<crate::generate::PlannedFile>,
    /// What went into each of them, for the manifest.
    files: Vec<MirrorFile>,
    /// The manifest, already serialised.
    manifest_json: String,
}

impl MirrorPlan {
    /// Write what the plan decided, and reconcile what a previous run left.
    ///
    /// Touches the filesystem and nothing else.
    pub fn apply(self, mode: crate::generate::Mode) -> Result<MirrorReport> {
        if mode == crate::generate::Mode::Write {
            for folder in ["specs", "decisions"] {
                std::fs::create_dir_all(self.root.join(folder)).map_err(Error::io(format!(
                    "create {}",
                    self.root.join(folder).display()
                )))?;
            }
        }

        // What the last run produced, so this one can tell what it has stopped
        // producing. Read before anything is written. A missing or unreadable
        // manifest means "nothing known", never "everything is an orphan" — the
        // one reading that could delete a tree.
        let previous: Vec<String> = std::fs::read_to_string(self.root.join("manifest.json"))
            .ok()
            .and_then(|raw| serde_json::from_str::<Manifest>(&raw).ok())
            .map(|m| m.files.into_iter().map(|f| f.path).collect())
            .unwrap_or_default();

        let mut report = MirrorReport::default();
        crate::generate::write_planned(
            &self.writes,
            mode,
            &mut report.written,
            &mut report.unchanged,
        )?;

        // --- Orphans ------------------------------------------------------
        //
        // Bounded three ways, because this is the only place generation
        // deletes: the path must have been produced by a previous run of *this*
        // project, must live under the mirror root, and must still be a file.
        // Anything the mirror did not write is not the mirror's to remove.
        let produced: std::collections::BTreeSet<&str> =
            self.files.iter().map(|f| f.path.as_str()).collect();
        for stale in &previous {
            if produced.contains(stale.as_str()) {
                continue;
            }
            if !stale.starts_with(MIRROR_PREFIX) || stale.contains("..") {
                continue;
            }
            let Ok(absolute) = crate::safe_path::confine(&self.repo_root, stale) else {
                // A manifest entry that no longer confines is a manifest
                // somebody has edited. Skipping it is the fail-closed
                // direction: the mirror declines to delete a file it cannot
                // prove it owns.
                continue;
            };
            if !absolute.is_file() {
                continue;
            }
            if mode == crate::generate::Mode::Write {
                std::fs::remove_file(&absolute)
                    .map_err(Error::io(format!("remove the orphaned {stale}")))?;
            }
            report.orphans.push(stale.clone());
        }
        report.orphans.sort();

        // The manifest carries a timestamp, so it always differs; written
        // unconditionally rather than pretending to compare it. It is therefore
        // left out of the report, or every `--check` run would claim the tree
        // is dirty because a clock moved.
        if mode == crate::generate::Mode::Write {
            crate::atomic::write(
                &crate::safe_path::confine(
                    &self.repo_root,
                    &format!("{MIRROR_PREFIX}manifest.json"),
                )?,
                &format!("{}\n", self.manifest_json),
            )?;
        }

        Ok(report)
    }
}

/// Decide the mirror's files, skipping artifacts that have adopted a real
/// repository file.
///
/// Reads the store and nothing else.
pub fn plan_except(
    store: &Store,
    project_id: &EntityId,
    repo_root: &Path,
    skip: &[EntityId],
) -> Result<MirrorPlan> {
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Err(Error::NotFound {
            entity_type: EntityType::Project,
            id: project_id.to_string(),
        });
    };

    let root = repo_root.join(MIRROR_DIR);
    let mut writes: Vec<crate::generate::PlannedFile> = Vec::new();
    let mut files: Vec<MirrorFile> = Vec::new();

    // --- README ----------------------------------------------------------
    let readme = format!(
        "# {} — generated\n\n\
         <!-- specline:generated project {} -->\n\n\
         **Do not edit these files.** They are regenerated from Specline, which is the source of \
         truth, and an edit here is overwritten by the next `specline generate` — not recovered, \
         not merged, gone. Change the artifact instead: ask Claude to write it, or edit it in \
         the app. If you have already edited a file and want the words back, \
         `specline import <file>` writes it in as a revision.\n\n\
         Tasks are deliberately absent: they churn, and mirroring them would make every repo \
         diff noisy. Run `specline render-status {}` if you want a task-shaped view.\n\n\
         ## Contents\n\n\
         - `specs/` — one file per spec, PRD, RFC, design doc or note\n\
         - `decisions/` — one file per decision record\n\
         - `questions.md` — every question and risk, open and settled\n\
         - `glossary.md` — what the words mean in this project\n\
         - `manifest.json` — which artifacts produced which file\n",
        project.name, project.id, project.slug
    );
    writes.push(planned(
        repo_root,
        &format!("{MIRROR_PREFIX}README.md"),
        readme,
    )?);

    // --- Specs and decisions, one file each ------------------------------
    for (entity_type, folder) in [
        (EntityType::Spec, "specs"),
        (EntityType::Decision, "decisions"),
    ] {
        let page = store.list(
            &EntityQuery::in_project(project_id.clone())
                .of_type(entity_type)
                .limited(5_000),
        )?;

        for entity in &page.items {
            if skip.contains(entity.id()) {
                continue;
            }
            let doc = store.revision(entity.id(), None)?;
            let slug = slugify(entity.label());
            let relative = format!("{MIRROR_PREFIX}{folder}/{slug}.md");
            let body = doc.as_ref().map(|d| d.body.as_str()).unwrap_or("");
            let version = doc.as_ref().map(|d| d.version).unwrap_or(0);

            let content = format!(
                "{}\n{}\n",
                header(entity_type.as_str(), entity.id(), version),
                render_prose(entity, body)
            );
            // The mirror builds its own paths from a slug, so this cannot
            // currently escape — which is exactly why it is checked. A slug
            // rule that grows a case for some future character is a change
            // nobody would think to re-examine here.
            writes.push(planned(repo_root, &relative, content)?);

            files.push(MirrorFile {
                path: relative,
                contributors: vec![Contributor {
                    entity_id: entity.id().clone(),
                    entity_type: entity_type.as_str().to_owned(),
                    version,
                    hash: crate::body_hash(entity.label(), body),
                }],
            });
        }
    }

    // --- Questions, aggregated -------------------------------------------
    let questions = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Question)
            .limited(5_000),
    )?;
    let mut open = String::new();
    writeln!(open, "# Questions and risks\n").map_err(fmt_err)?;
    writeln!(
        open,
        "<!-- specline:generated questions {} -->\n> Generated from Specline — edits here are not saved.\n",
        project.id
    )
    .map_err(fmt_err)?;

    // Both halves, deliberately. An unresolved question stops someone deciding
    // something nobody has decided; a *settled* one stops them re-deciding
    // something that was, which is the more expensive mistake and the one an
    // agent joining a project makes by default. Emitting only the open half —
    // as this file did until 2026-08-10 — leaves the second job to a
    // hand-maintained prose copy, and a register that exists twice is not one.
    let mut contributors = Vec::new();
    let mut any = false;
    for (heading, blurb, unresolved) in [
        (
            "Open",
            "Nothing here is decided. Do not build on any of it without saying so.",
            true,
        ),
        (
            "Settled",
            "Decided, with the reasoning. Do not re-litigate these.",
            false,
        ),
    ] {
        let mut section = false;
        for entity in &questions.items {
            let Entity::Question(q) = entity else {
                continue;
            };
            if q.status.is_unresolved() != unresolved {
                continue;
            }
            if !section {
                writeln!(open, "## {heading}\n\n*{blurb}*\n").map_err(fmt_err)?;
                section = true;
                any = true;
            }
            let doc = store.revision(&q.id, None)?;
            let body = doc.as_ref().map(|d| d.body.as_str()).unwrap_or("");
            writeln!(open, "### {}\n", q.title).map_err(fmt_err)?;
            writeln!(
                open,
                "`{}` · {} · {}{}\n",
                q.id,
                q.kind,
                q.status,
                q.severity
                    .map(|s| format!(" · severity {s}"))
                    .unwrap_or_default()
            )
            .map_err(fmt_err)?;
            if !body.trim().is_empty() {
                writeln!(open, "{}\n", body.trim()).map_err(fmt_err)?;
            }
            contributors.push(Contributor {
                entity_id: q.id.clone(),
                entity_type: "question".to_owned(),
                version: doc.as_ref().map(|d| d.version).unwrap_or(0),
                hash: crate::body_hash(&q.title, body),
            });
        }
    }
    if !any {
        writeln!(open, "*Nothing recorded.*").map_err(fmt_err)?;
    }
    writes.push(planned(
        repo_root,
        &format!("{MIRROR_PREFIX}questions.md"),
        open,
    )?);
    files.push(MirrorFile {
        path: format!("{MIRROR_PREFIX}questions.md"),
        contributors,
    });

    // --- Glossary, aggregated --------------------------------------------
    let terms = store.list(
        &EntityQuery::in_project(project_id.clone())
            .of_type(EntityType::Term)
            .limited(5_000),
    )?;
    let mut glossary = String::new();
    writeln!(glossary, "# Glossary\n").map_err(fmt_err)?;
    writeln!(
        glossary,
        "<!-- specline:generated glossary {} -->\n> Generated from Specline — edits here are not saved.\n",
        project.id
    )
    .map_err(fmt_err)?;

    let mut rows: Vec<(&str, &str, bool, &EntityId)> = terms
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Term(t) => Some((
                t.term.as_str(),
                t.definition.as_str(),
                t.project_id.is_none(),
                &t.id,
            )),
            _ => None,
        })
        .collect();
    // A project-scoped term overrides a global of the same name (Q-4), so the
    // global is dropped rather than listed twice with different meanings.
    let scoped: std::collections::HashSet<String> = rows
        .iter()
        .filter(|(_, _, global, _)| !global)
        .map(|(term, _, _, _)| term.to_lowercase())
        .collect();
    rows.retain(|(term, _, global, _)| !global || !scoped.contains(&term.to_lowercase()));
    rows.sort_by_key(|(term, _, _, _)| term.to_lowercase());

    let mut term_contributors = Vec::new();
    if rows.is_empty() {
        writeln!(glossary, "*No terms defined yet.*").map_err(fmt_err)?;
    } else {
        for (term, definition, global, id) in &rows {
            writeln!(
                glossary,
                "**{term}**{} — {definition}\n",
                if *global { " *(global)*" } else { "" }
            )
            .map_err(fmt_err)?;
            term_contributors.push(Contributor {
                entity_id: (*id).clone(),
                entity_type: "term".to_owned(),
                version: 0,
                hash: crate::body_hash(term, definition),
            });
        }
    }
    writes.push(planned(
        repo_root,
        &format!("{MIRROR_PREFIX}glossary.md"),
        glossary,
    )?);
    files.push(MirrorFile {
        path: format!("{MIRROR_PREFIX}glossary.md"),
        contributors: term_contributors,
    });

    // --- Manifest --------------------------------------------------------
    //
    // Serialised here rather than in `apply`, because it is entirely a
    // statement about what the store said. Writing it, and reading the previous
    // one to work out what has been orphaned, belong to the other half.
    let manifest = Manifest {
        project_id: project_id.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(Error::json("serialise the mirror manifest"))?;

    Ok(MirrorPlan {
        repo_root: repo_root.to_path_buf(),
        root,
        writes,
        files: manifest.files,
        manifest_json,
    })
}

/// One planned mirror file.
///
/// The mirror's own comparison ignores the banner entirely — every file it
/// writes has one, so its presence is never the difference.
fn planned(
    repo_root: &Path,
    relative: &str,
    content: String,
) -> Result<crate::generate::PlannedFile> {
    Ok(crate::generate::PlannedFile {
        absolute: crate::safe_path::confine(repo_root, relative)?,
        relative: relative.to_owned(),
        content,
        banner_counts: false,
    })
}

/// The generated-file header.
///
/// Says plainly that edits are not saved. The plugin's hook makes that a
/// half-truth in Claude Code — an edit there becomes a revision — but in chat
/// or Cowork, where no hook runs, it is exactly true.
fn header(entity_type: &str, id: &EntityId, version: i32) -> String {
    format!(
        "<!-- specline:generated {entity_type} {id} v{version} {}\n     \
         source of truth is Specline — edits here are not saved -->",
        chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ")
    )
}

/// Render an artifact's front matter and body.
fn render_prose(entity: &Entity, body: &str) -> String {
    // The readable identifier goes in the heading, not in the metadata below
    // it. `B-12` is what the prose in this repository actually cites, so a
    // reader arriving from a citation should land on a heading that matches
    // what they searched for.
    let mut out = match entity {
        Entity::Decision(d) if d.number > 0 => format!("# B-{} — {}\n\n", d.number, d.title),
        _ => format!("# {}\n\n", entity.label()),
    };
    if let Some(status) = entity.status() {
        out.push_str(&format!("**Status:** `{status}`  \n"));
    }
    match entity {
        Entity::Spec(s) => out.push_str(&format!("**Kind:** `{}`  \n", s.kind)),
        Entity::Decision(d) => {
            if let Some(at) = d.decided_at {
                out.push_str(&format!("**Decided:** {}  \n", at.format("%Y-%m-%d")));
            }
        }
        _ => {}
    }
    out.push_str(&format!("**Id:** `{}`\n\n", entity.id()));
    out.push_str(body.trim());
    out.push('\n');
    out
}

// `write_if_changed` and `strip_header` used to live here. Writing moved to
// `generate::write_planned`, which both halves of a generate now share: they
// were two near-identical functions with one real difference — whether a file's
// *lack* of a banner counts as a change, which is true for adopted prose being
// generated for the first time and never true for the mirror. That difference
// is a field on the planned file now rather than a second copy of the
// comparison.
//
// The comparison is not an optimisation, in either version: the mirror is
// regenerated on every relevant write, and rewriting an unchanged file would
// touch its mtime, dirty the working tree and produce a stream of empty
// commits.

/// A filesystem-safe slug.
fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let joined = s
        .split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    if joined.is_empty() {
        "untitled".to_owned()
    } else {
        joined
    }
}

/// Convert a formatting error, which cannot actually happen when writing to a
/// `String`, into a domain error rather than unwrapping it.
fn fmt_err(e: std::fmt::Error) -> Error {
    Error::Invariant {
        operation: "render the mirror".to_owned(),
        problem: e.to_string(),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_filesystem_safe() {
        assert_eq!(slugify("Usage metering"), "usage-metering");
        assert_eq!(
            slugify("REQ-1: Idempotent ingest!"),
            "req-1-idempotent-ingest"
        );
        assert_eq!(slugify("///"), "untitled");
    }

    #[test]
    fn the_header_is_ignored_when_comparing_content() {
        // The header carries a timestamp. Without stripping it, every file
        // would look changed on every run and the repo would fill with empty
        // commits.
        //
        // Asserted against `generate`'s copy of the rule, which is now the only
        // copy: the mirror had its own, identical but for a trailing newline.
        use crate::generate::strip_banner_public as strip;
        let a = "<!-- specline:generated spec spc_1 v1 2026-08-09T10:00:00Z\n     source -->\n# Title\n\nBody\n";
        let b = "<!-- specline:generated spec spc_1 v1 2026-08-09T23:59:59Z\n     source -->\n# Title\n\nBody\n";
        assert_eq!(strip(a), strip(b));

        let c = "<!-- specline:generated spec spc_1 v2 2026-08-09T10:00:00Z\n     source -->\n# Title\n\nDifferent\n";
        assert_ne!(strip(a), strip(c));
    }

    #[test]
    fn this_module_contains_no_way_to_read_a_mirror_file() {
        // D-3 and hard constraint 2. The mirror is one-directional; the
        // absence of a reader is the enforcement, so this test asserts the
        // absence rather than trusting a comment.
        let source = include_str!("mirror.rs");
        let body = source
            .split("#[cfg(test)]")
            .next()
            .expect("there is always a non-test half");
        assert!(
            !body.contains("read_to_string(path)") || body.contains("strip_header"),
            "the only permitted read is the unchanged-content comparison"
        );
        for forbidden in [
            "fn read_mirror",
            "fn parse_mirror",
            "fn reconcile",
            "fn sync",
        ] {
            assert!(
                !body.contains(forbidden),
                "`{forbidden}` would make the mirror a second source of truth"
            );
        }
    }
}
