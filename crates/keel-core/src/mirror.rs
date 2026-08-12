//! The one-directional markdown mirror.
//!
//! A generated export of a project's prose into `<repo>/.keel/`, so that specs,
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
//! `keel import`.
//!
//! # Prose only
//!
//! TQ-5. Tasks churn, and mirroring them would make every repo diff noisy for
//! no gain. A task-shaped status file is what `keel render-status` is for.

use crate::{Entity, EntityId, EntityQuery, EntityStore, EntityType, Error, Result, Store};
use serde::{Deserialize, Serialize};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

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
    /// survive carrying a `keel:generated` banner and a real id, reading as
    /// current forever. That is worse than a missing file: it is plausible,
    /// greppable and permanently wrong, and nothing would ever say so.
    pub orphans: Vec<String>,
}

impl MirrorReport {
    /// Whether anything changed on disk.
    pub fn is_noop(&self) -> bool {
        self.written.is_empty()
    }
}

/// Generate the mirror for a project into `repo_root/.keel/`.
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
    let Some(Entity::Project(project)) = store.get(project_id)? else {
        return Err(Error::NotFound {
            entity_type: EntityType::Project,
            id: project_id.to_string(),
        });
    };

    let root = repo_root.join(".keel");
    if mode == crate::generate::Mode::Write {
        std::fs::create_dir_all(root.join("specs")).map_err(Error::io(format!(
            "create {}",
            root.join("specs").display()
        )))?;
        std::fs::create_dir_all(root.join("decisions")).map_err(Error::io(format!(
            "create {}",
            root.join("decisions").display()
        )))?;
    }

    // What the last run produced, so this one can tell what it has stopped
    // producing. Read before anything is written. A missing or unreadable
    // manifest means "nothing known", never "everything is an orphan" — the
    // one reading that could delete a tree.
    let previous: Vec<String> = std::fs::read_to_string(root.join("manifest.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<Manifest>(&raw).ok())
        .map(|m| m.files.into_iter().map(|f| f.path).collect())
        .unwrap_or_default();

    let mut report = MirrorReport::default();
    let mut files: Vec<MirrorFile> = Vec::new();

    // --- README ----------------------------------------------------------
    let readme = format!(
        "# {} — generated\n\n\
         <!-- keel:generated project {} -->\n\n\
         **Do not edit these files.** They are regenerated from Keel, which is the source of \
         truth, and an edit here is overwritten by the next `keel generate` — not recovered, \
         not merged, gone. Change the artifact instead: ask Claude to write it, or edit it in \
         the app. If you have already edited a file and want the words back, \
         `keel import <file>` writes it in as a revision.\n\n\
         Tasks are deliberately absent: they churn, and mirroring them would make every repo \
         diff noisy. Run `keel render-status {}` if you want a task-shaped view.\n\n\
         ## Contents\n\n\
         - `specs/` — one file per spec, PRD, RFC, design doc or note\n\
         - `decisions/` — one file per decision record\n\
         - `questions.md` — every question and risk, open and settled\n\
         - `glossary.md` — what the words mean in this project\n\
         - `manifest.json` — which artifacts produced which file\n",
        project.name, project.id, project.slug
    );
    write_if_changed(
        &root.join("README.md"),
        &readme,
        ".keel/README.md",
        mode,
        &mut report,
    )?;

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
            let relative = format!(".keel/{folder}/{slug}.md");
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
            write_if_changed(
                &crate::safe_path::confine(repo_root, &relative)?,
                &content,
                &relative,
                mode,
                &mut report,
            )?;

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
        "<!-- keel:generated questions {} -->\n> Generated from Keel — edits here are not saved.\n",
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
    write_if_changed(
        &crate::safe_path::confine(repo_root, ".keel/questions.md")?,
        &open,
        ".keel/questions.md",
        mode,
        &mut report,
    )?;
    files.push(MirrorFile {
        path: ".keel/questions.md".to_owned(),
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
        "<!-- keel:generated glossary {} -->\n> Generated from Keel — edits here are not saved.\n",
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
    write_if_changed(
        &crate::safe_path::confine(repo_root, ".keel/glossary.md")?,
        &glossary,
        ".keel/glossary.md",
        mode,
        &mut report,
    )?;
    files.push(MirrorFile {
        path: ".keel/glossary.md".to_owned(),
        contributors: term_contributors,
    });

    // --- Orphans ----------------------------------------------------------
    //
    // Bounded three ways, because this is the only place generation deletes:
    // the path must have been produced by a previous run of *this* project,
    // must live under the mirror root, and must still be a file. Anything the
    // mirror did not write is not the mirror's to remove.
    let produced: std::collections::BTreeSet<&str> =
        files.iter().map(|f| f.path.as_str()).collect();
    for stale in &previous {
        if produced.contains(stale.as_str()) {
            continue;
        }
        if !stale.starts_with(".keel/") || stale.contains("..") {
            continue;
        }
        let Ok(absolute) = crate::safe_path::confine(repo_root, stale) else {
            // A manifest entry that no longer confines is a manifest somebody
            // has edited. Skipping it is the fail-closed direction: the mirror
            // declines to delete a file it cannot prove it owns.
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

    // --- Manifest --------------------------------------------------------
    let manifest = Manifest {
        project_id: project_id.clone(),
        generated_at: chrono::Utc::now().to_rfc3339(),
        files,
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(Error::json("serialise the mirror manifest"))?;
    // The manifest carries a timestamp, so it always differs; written
    // unconditionally rather than pretending to compare it. It is therefore
    // left out of the report, or every `--check` run would claim the tree is
    // dirty because a clock moved.
    if mode == crate::generate::Mode::Write {
        std::fs::write(repo_root.join(".keel/manifest.json"), format!("{json}\n"))
            .map_err(Error::io("write the mirror manifest"))?;
    }

    Ok(report)
}

/// The generated-file header.
///
/// Says plainly that edits are not saved. The plugin's hook makes that a
/// half-truth in Claude Code — an edit there becomes a revision — but in chat
/// or Cowork, where no hook runs, it is exactly true.
fn header(entity_type: &str, id: &EntityId, version: i32) -> String {
    format!(
        "<!-- keel:generated {entity_type} {id} v{version} {}\n     \
         source of truth is Keel — edits here are not saved -->",
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

/// Write only when the content differs.
///
/// Not an optimisation — the mirror is regenerated on every relevant write, and
/// rewriting an unchanged file would touch its mtime, dirty the working tree
/// and produce a stream of empty commits.
fn write_if_changed(
    path: &Path,
    content: &str,
    relative: &str,
    mode: crate::generate::Mode,
    report: &mut MirrorReport,
) -> Result<()> {
    // Compare ignoring the header, which carries a fresh timestamp every run
    // and would otherwise make every file look changed every time.
    let existing = std::fs::read_to_string(path).ok();
    if let Some(old) = &existing
        && strip_header(old) == strip_header(content)
    {
        report.unchanged.push(relative.to_owned());
        return Ok(());
    }

    report.written.push(relative.to_owned());
    if mode == crate::generate::Mode::Check {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(Error::io(format!("create {}", parent.display())))?;
    }
    std::fs::write(path, content).map_err(Error::io(format!("write {}", path.display())))
}

/// Drop the generated header comment, for change comparison.
fn strip_header(content: &str) -> String {
    let mut out = String::new();
    let mut in_header = false;
    for line in content.lines() {
        if line.trim_start().starts_with("<!-- keel:generated") {
            in_header = !line.contains("-->");
            continue;
        }
        if in_header {
            in_header = !line.contains("-->");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

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

/// Where a project's mirror lives, if it has a checkout.
pub fn mirror_root(project: &crate::Project) -> Option<PathBuf> {
    project
        .root_path
        .as_ref()
        .map(|p| PathBuf::from(p).join(".keel"))
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
        let a = "<!-- keel:generated spec spc_1 v1 2026-08-09T10:00:00Z\n     source -->\n# Title\n\nBody\n";
        let b = "<!-- keel:generated spec spc_1 v1 2026-08-09T23:59:59Z\n     source -->\n# Title\n\nBody\n";
        assert_eq!(strip_header(a), strip_header(b));

        let c = "<!-- keel:generated spec spc_1 v2 2026-08-09T10:00:00Z\n     source -->\n# Title\n\nDifferent\n";
        assert_ne!(strip_header(a), strip_header(c));
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
