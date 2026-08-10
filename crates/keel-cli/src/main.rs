//! `keel` — the command-line client.
//!
//! Deliberately thin. Everything it does lives in `keel-core`; this crate
//! resolves a store path, parses arguments, and prints. That split is what
//! lets the daemon expose the same operations without either of them growing a
//! dependency on the other.
//!
//! Phase 0 gives it `fsck`, `backup`, `restore` and `fixture`. `render-status`
//! arrives in Phase 1, with the dogfooding switch.

mod bootstrap;
mod gate;
mod generate;
mod import;
mod rubric;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use keel_core::{DuckStore, backup, fixture, fsck};
use std::path::PathBuf;

/// Keel's command-line client.
#[derive(Parser)]
#[command(name = "keel", version, about = "Keel — the project spine", long_about = None)]
struct Cli {
    /// Where the store lives. Defaults to `~/.keel`.
    ///
    /// `keel-core` never reads the environment; resolving this is the CLI's
    /// job, which is why the flag exists here and not there.
    #[arg(long, global = true, env = "KEEL_HOME")]
    home: Option<PathBuf>,

    /// Print machine-readable JSON instead of prose.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Check cross-engine referential integrity.
    ///
    /// Exits non-zero if anything is actually broken, so it can gate a backup
    /// or a deploy.
    Fsck,

    /// Back up both engines to Parquet.
    Backup {
        /// Where to write it. Defaults to `<home>/backups/<timestamp>`.
        #[arg(long)]
        dest: Option<PathBuf>,
    },

    /// Restore a backup into an empty directory.
    Restore {
        /// The backup directory to read.
        source: PathBuf,
        /// Where to restore to. Must not already contain a store.
        target: PathBuf,
    },

    /// Load the realistic fixture corpus into an empty store.
    Fixture,

    /// Import markdown files into Keel as versioned documents.
    ///
    /// Re-importing is safe: the same file lands on the same artifact, and
    /// unchanged content appends no revision. So the repo copy can stay
    /// authoritative for as long as you like with Keel kept in step.
    Import {
        /// Markdown files to import.
        files: Vec<PathBuf>,
        /// Project id, slug or name.
        #[arg(long)]
        project: String,
        /// What to store them as.
        #[arg(long, default_value = "spec")]
        r#as: String,
        /// Override the inferred spec kind: prd, spec, rfc, design-doc, note.
        #[arg(long)]
        kind: Option<String>,
        /// Override the title, which otherwise comes from the first heading.
        #[arg(long)]
        title: Option<String>,
    },

    /// Seed Keel's own project — the dogfooding switch.
    ///
    /// Imports the real state from the product docs: phases as milestones,
    /// the actual task list, the decision log, the open questions and the
    /// glossary. After this, `keel render-status keel` generates
    /// `product/STATUS.md` rather than a human maintaining it.
    Bootstrap {
        /// Repository path to record on the project, for the markdown mirror.
        #[arg(long)]
        repo: Option<String>,
        /// Archive every other project, leaving only Keel visible.
        ///
        /// Soft delete — the rows stay on disk, they just stop appearing.
        #[arg(long)]
        only: bool,
    },

    /// Print a one-line summary of what is in the store.
    Status,

    /// Regenerate a project's repository files from Keel.
    ///
    /// Keel is the source of truth; the markdown in the repo is an output.
    /// This writes the adopted prose files at their recorded paths, the
    /// `.keel/` mirror for everything born in Keel, and the tracker.
    ///
    /// One-directional: nothing here reads a generated file back into the
    /// store. It goes through the running daemon, which owns the store —
    /// falling back to opening the store directly only when no daemon is up.
    Generate {
        /// Project id, slug or name.
        project: String,
        /// Repository root. Defaults to the project's recorded `root_path`.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Report what would change and exit non-zero if anything would.
        ///
        /// For a pre-commit hook or CI: makes a hand edit to a generated file
        /// a failure someone sees rather than work someone silently loses.
        #[arg(long)]
        check: bool,
        /// Daemon base URL. Defaults to the local daemon.
        #[arg(long, default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// Score Phase 2's exit criterion from the event log.
    ///
    /// Does not run the sessions — "unprompted" is the whole claim and a test
    /// that calls the tool has prompted it. This scores what the sessions did.
    Gate {
        /// Restrict to one project.
        #[arg(long)]
        project: Option<String>,
        /// Only count activity after this instant (RFC 3339).
        #[arg(long)]
        since: Option<String>,
        /// Score from an archived run directory instead of the event log.
        ///
        /// Transcript-based: one file per session, so ids cannot collide, and
        /// a session that only *offered* to write is visible. This is the mode
        /// to use for a real run.
        #[arg(long)]
        run: Option<PathBuf>,
        /// How many sessions were run. The denominator.
        ///
        /// Not derived from the log: a session that wrote nothing leaves no
        /// event, so the log cannot tell you it happened.
        #[arg(long, default_value_t = 10)]
        sessions: usize,
        /// Daemon base URL.
        #[arg(long, default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// Print the generated tracker for a project to standard output.
    RenderStatus {
        /// Project id, slug or name.
        project: String,
        /// Write here instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
    },

    /// Append to a row's running commentary, or read it back.
    ///
    /// The commentary is what the tracker's prose used to carry. Having it on
    /// the CLI matters beyond convenience: it is the only write path that does
    /// not go through MCP, so a note can still be recorded when the MCP surface
    /// is unavailable.
    Note {
        #[command(subcommand)]
        action: NoteAction,
    },

    /// Archive a row. Soft delete — it stays on disk and stops appearing.
    ///
    /// Needed for the same reason `task` is: with MCP down there was no way to
    /// retire a row, and a document that has outlived its purpose keeps owning
    /// the file path it adopted.
    Archive {
        /// The entity id.
        id: String,
        /// The version you believe is current, for optimistic concurrency.
        #[arg(long)]
        version: i32,
    },

    /// Create a task row.
    ///
    /// Exists because until now the only ways to bring a row into being were
    /// MCP and the one-shot `bootstrap`/`import` migrations. With MCP down
    /// there was no way at all, which is how four completed pieces of work
    /// ended up as lines in a markdown table and nowhere else.
    Task {
        /// Project id, slug or name.
        #[arg(long)]
        project: String,
        /// The task title.
        title: String,
        /// Longer description.
        #[arg(long)]
        body: Option<String>,
        /// todo, in_progress, blocked, done, dropped.
        #[arg(long, default_value = "todo")]
        status: String,
        /// p0, p1, p2, p3.
        #[arg(long, default_value = "p2")]
        priority: String,
    },
}

#[derive(Subcommand, Debug)]
enum NoteAction {
    /// Append a note to a row.
    Add {
        /// The row to annotate. Any entity id.
        entity: String,
        /// The note. A finding, a decision, an observation.
        body: String,
        /// The conversation responsible, so the note stays traceable.
        #[arg(long)]
        session: Option<String>,
    },
    /// Print a row's notes, oldest first.
    Ls {
        /// The row whose commentary to read.
        entity: String,
        /// Include retracted notes.
        #[arg(long)]
        all: bool,
    },
    /// Retract a note. Soft, like every other removal in the store.
    Retract {
        /// The note id, `nte_…`.
        id: String,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "keel=info".into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let cli = Cli::parse();
    let home = resolve_home(cli.home.clone())?;

    match &cli.command {
        Command::Fsck => run_fsck(&home, cli.json),
        Command::Backup { dest } => run_backup(&home, dest.clone(), cli.json),
        Command::Restore { source, target } => run_restore(source, target, cli.json),
        Command::Fixture => run_fixture(&home, cli.json),
        Command::Status => run_status(&home, cli.json),
        Command::RenderStatus { project, out } => run_render_status(&home, project, out.clone()),
        Command::Note { action } => run_note(&home, action, cli.json),
        Command::Archive { id, version } => {
            use keel_core::{Actor, EntityId, EntityStore, Provenance, Surface};
            let mut store = open(&home)?;
            let prov = Provenance::anonymous(Actor::Human).with_surface(Surface::Cli);
            let archived = store.archive(&EntityId::parse(id)?, *version, &prov)?;
            println!("{} — archived", archived.id());
            Ok(())
        }
        Command::Task {
            project,
            title,
            body,
            status,
            priority,
        } => run_task_add(
            &home,
            project,
            title,
            body.clone(),
            status,
            priority,
            cli.json,
        ),
        Command::Gate {
            project,
            since,
            run,
            sessions,
            daemon,
        } => match run {
            Some(dir) => gate::score_run(dir, cli.json),
            None => gate::run(
                daemon,
                project.as_deref(),
                since.as_deref(),
                *sessions,
                cli.json,
            ),
        },
        Command::Generate {
            project,
            repo,
            check,
            daemon,
        } => generate::run(&home, project, repo.clone(), *check, daemon, cli.json),
        Command::Bootstrap { repo, only } => run_bootstrap(&home, repo.clone(), *only, cli.json),
        Command::Import {
            files,
            project,
            r#as,
            kind,
            title,
        } => run_import(
            &home,
            files,
            project,
            r#as,
            kind.clone(),
            title.clone(),
            cli.json,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_import(
    home: &PathBuf,
    files: &[PathBuf],
    project: &str,
    as_type: &str,
    kind: Option<String>,
    title: Option<String>,
    json: bool,
) -> Result<()> {
    use keel_core::{EntityType, SpecKind};

    if files.is_empty() {
        bail!("no files given. Pass one or more markdown paths");
    }
    if title.is_some() && files.len() > 1 {
        bail!(
            "--title applies to a single file; {} were given",
            files.len()
        );
    }

    let entity_type = EntityType::parse(as_type)?;
    let kind = match kind {
        Some(k) => Some(SpecKind::parse(&k)?),
        None => None,
    };

    let mut store = open(home)?;
    let found = resolve_project(&store, project)?;
    let project_id = found.id().clone();

    let mut rows = Vec::new();
    for path in files {
        let imported = import::file(
            &mut store,
            path,
            &project_id,
            entity_type,
            kind,
            title.clone(),
        )?;
        rows.push((path.clone(), imported));
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!(
                rows.iter()
                    .map(|(path, i)| serde_json::json!({
                        "file": path.display().to_string(),
                        "id": i.entity_id.as_str(),
                        "title": i.title,
                        "version": i.version,
                        "created": i.created,
                        "revised": i.revised,
                        "bytes": i.bytes,
                        "mirror_path": i.mirror_path,
                    }))
                    .collect::<Vec<_>>()
            ))?
        );
    } else {
        for (path, i) in &rows {
            let what = if i.created {
                "created"
            } else if i.revised {
                "revised"
            } else {
                "unchanged"
            };
            // Naming the adopted path is the point of the line: it is what
            // `keel generate` will write back over, and a surprise there is
            // the one that costs someone a file.
            let adopted = match &i.mirror_path {
                Some(p) => format!("  → generates {p}"),
                None => "  → no repo path; goes to the .keel mirror".to_owned(),
            };
            println!(
                "{what:>9}  {}  v{}  {} bytes  {}{adopted}",
                i.title,
                i.version,
                i.bytes,
                path.display()
            );
        }
    }
    Ok(())
}

fn run_bootstrap(home: &PathBuf, repo: Option<String>, only: bool, json: bool) -> Result<()> {
    let mut store = open(home)?;
    let summary = bootstrap::run(&mut store, repo)?;

    let archived = if only {
        bootstrap::archive_other_projects(&mut store, &summary.project_id)?
    } else {
        0
    };

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project_id": summary.project_id.as_str(),
                "entities": summary.entities,
                "links": summary.links,
                "revisions": summary.revisions,
                "archived": archived,
            }))?
        );
    } else {
        println!(
            "seeded Keel: {} entities, {} links, {} document revisions",
            summary.entities, summary.links, summary.revisions
        );
        println!("  project {}", summary.project_id);
        if only {
            println!("  archived {archived} artifact(s) belonging to other projects");
        }
        println!();
        println!("Keel now tracks itself. Regenerate the tracker with:");
        println!("  keel render-status keel --out product/STATUS.md");
    }
    Ok(())
}

/// Resolve a project by id, slug or name.
pub(crate) fn resolve_project(store: &DuckStore, reference: &str) -> Result<keel_core::Entity> {
    use keel_core::{Entity, EntityQuery, EntityStore, EntityType};
    let projects = store.list(&EntityQuery::default().of_type(EntityType::Project))?;
    let needle = reference.to_lowercase();
    projects
        .items
        .into_iter()
        .find(|p| match p {
            Entity::Project(pr) => {
                pr.id.as_str() == reference
                    || pr.slug.eq_ignore_ascii_case(reference)
                    || pr.name.to_lowercase() == needle
            }
            _ => false,
        })
        .with_context(|| format!("no project matches `{reference}`"))
}

fn run_render_status(home: &PathBuf, project: &str, out: Option<PathBuf>) -> Result<()> {
    let store = open(home)?;
    let found = resolve_project(&store, project)?;

    let markdown = keel_core::render_status::render(&store, found.id())?;
    match out {
        Some(path) => {
            std::fs::write(&path, &markdown)
                .with_context(|| format!("write {}", path.display()))?;
            println!("wrote {} ({} bytes)", path.display(), markdown.len());
        }
        None => print!("{markdown}"),
    }
    Ok(())
}

fn run_note(home: &PathBuf, action: &NoteAction, json: bool) -> Result<()> {
    use keel_core::{Actor, EntityId, EntityStore, NewNote, NoteId, Provenance, Surface};

    let mut store = open(home)?;
    // `cli` rather than `code`: this is a person at a terminal, and the whole
    // point of `surface` is telling those apart when reading the history back.
    let prov = Provenance::anonymous(Actor::Human).with_surface(Surface::Cli);

    match action {
        NoteAction::Add {
            entity,
            body,
            session,
        } => {
            let id = EntityId::parse(entity)?;
            let mut note = NewNote::new(id, body.clone(), Actor::Human);
            if let Some(s) = session {
                note = note.in_session(s.clone());
            }
            let written = store.add_note(note, &prov)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&written)?);
            } else {
                println!("{} — noted on {}", written.id, written.entity_id);
            }
        }
        NoteAction::Ls { entity, all } => {
            let id = EntityId::parse(entity)?;
            let notes = store.notes_for(&id, *all)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&notes)?);
            } else if notes.is_empty() {
                println!("no notes on {id}");
            } else {
                for n in &notes {
                    let mark = if n.is_live() { " " } else { "×" };
                    println!(
                        "{mark} {} · {} · {}\n  {}",
                        n.id,
                        n.author,
                        n.created_at.format("%Y-%m-%d %H:%M"),
                        n.body.replace('\n', "\n  ")
                    );
                }
            }
        }
        NoteAction::Retract { id } => {
            let note = store.retract_note(&NoteId::parse(id)?, &prov)?;
            println!("{} — retracted", note.id);
        }
    }
    Ok(())
}

fn run_task_add(
    home: &PathBuf,
    project: &str,
    title: &str,
    body: Option<String>,
    status: &str,
    priority: &str,
    json: bool,
) -> Result<()> {
    use keel_core::{Actor, EntityStore, Provenance, Surface, Task, TaskPriority, TaskStatus};

    let mut store = open(home)?;
    let found = resolve_project(&store, project)?;
    let prov = Provenance::anonymous(Actor::Human).with_surface(Surface::Cli);

    let mut task = Task::new(found.id().clone(), title);
    task.status = TaskStatus::parse(status)?;
    task.priority = TaskPriority::parse(priority)?;
    task.body = body;

    let created = store.create(task.into(), &prov)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&created)?);
    } else {
        println!(
            "{} — {}",
            created.entity.id(),
            if created.created {
                "created"
            } else {
                "already existed, returned unchanged"
            }
        );
    }
    Ok(())
}

/// Resolve the store directory.
fn resolve_home(explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    // Q-2's working assumption: `~/.keel`, local, no remote.
    let home = std::env::var_os("HOME").map(PathBuf::from).context(
        "HOME is not set, so the default store location cannot be resolved. Pass --home",
    )?;
    Ok(home.join(".keel"))
}

fn open(home: &PathBuf) -> Result<DuckStore> {
    DuckStore::open(home).with_context(|| format!("open the store at {}", home.display()))
}

fn run_fsck(home: &PathBuf, json: bool) -> Result<()> {
    let store = open(home)?;
    let report = fsck::check(&store)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else if report.findings.is_empty() {
        println!("clean — {} checks, nothing found", report.checks_run);
    } else {
        println!(
            "{} checks run, {} finding(s):\n",
            report.checks_run,
            report.findings.len()
        );
        for f in &report.findings {
            let marker = match f.severity {
                fsck::Severity::Error => "ERROR",
                fsck::Severity::Warning => "warn ",
            };
            println!("{marker}  {}", f.check);
            println!("        {}", f.detail);
            println!("        → {}\n", f.remedy);
        }
    }

    if !report.is_clean() {
        // Non-zero so this can gate a backup or a deploy. Warnings do not
        // fail: an orphaned task under an archived project is expected.
        bail!(
            "{} error-level finding(s); the store is not consistent",
            report.errors().count()
        );
    }
    Ok(())
}

fn run_backup(home: &PathBuf, dest: Option<PathBuf>, json: bool) -> Result<()> {
    let store = open(home)?;
    let dest = dest.unwrap_or_else(|| backup::default_backup_dir(home, chrono::Utc::now()));

    let manifest = backup::backup(&store, &dest)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&manifest)?);
    } else {
        println!(
            "backed up {} rows to {}",
            manifest.total_rows(),
            dest.display()
        );
        println!("  DuckDB → {}/duckdb", dest.display());
        println!("  Lance  → {}/lance  (documents and blobs)", dest.display());
    }
    Ok(())
}

fn run_restore(source: &PathBuf, target: &PathBuf, json: bool) -> Result<()> {
    let manifest = backup::restore(source, target)?;

    // Re-open and verify rather than trusting the restore. "Assert equality,
    // don't eyeball it" is the exit criterion, and a restore that silently
    // dropped a table is exactly the failure a backup exists to prevent.
    let restored = open(target)?;
    let problems = backup::verify_restore(&restored, &manifest)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "manifest": manifest,
                "problems": problems,
            }))?
        );
    } else if problems.is_empty() {
        println!(
            "restored {} rows to {} and verified every table",
            manifest.total_rows(),
            target.display()
        );
    } else {
        println!("restored with {} discrepancy(ies):", problems.len());
        for p in &problems {
            println!("  {p}");
        }
    }

    if !problems.is_empty() {
        bail!("the restored store does not match the backup manifest");
    }

    // A restored store must be a git repository, or the restore has quietly
    // cost you a recovery tier.
    //
    // SPEC §11 names three: the store's own git history (full fidelity,
    // including every revision), the Parquet backup, and the markdown mirror.
    // `restore` rebuilds from tier 2 into a fresh directory — and until now
    // handed back a store with no `.git`, so tier 1 was silently gone. Found
    // the hard way, one command before deleting the only copy that still had
    // it.
    match init_store_git(target) {
        Ok(true) => println!("  initialised {} as a git repository", target.display()),
        Ok(false) => {}
        // Never fail the restore for this. The rows are back and verified;
        // a missing git binary is a smaller problem than pretending the
        // restore did not happen.
        Err(e) => eprintln!(
            "  warning: could not initialise {} as a git repository: {e}\n  \
             The data is restored and verified, but SPEC §11's recovery tier 1 \
             is missing until you run: git -C {} init",
            target.display(),
            target.display()
        ),
    }
    Ok(())
}

/// Make a restored store its own git repository, as `plugin/install.sh` does
/// for a fresh one.
///
/// Returns whether it created anything. Deliberately no remote — that is Q-2
/// and it is KB's call.
fn init_store_git(target: &std::path::Path) -> Result<bool> {
    if target.join(".git").exists() {
        return Ok(false);
    }
    if std::process::Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        bail!("git is not on PATH");
    }

    let git = |args: &[&str]| -> Result<()> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(target)
            .args(args)
            .output()
            .with_context(|| format!("run git {}", args.join(" ")))?;
        if !out.status.success() {
            bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(())
    };

    git(&["init", "-q"])?;
    std::fs::write(
        target.join(".gitignore"),
        "# Model weights are large and re-downloadable.\nmodels/\n",
    )
    .with_context(|| format!("write {}/.gitignore", target.display()))?;
    git(&["add", "-A"])?;
    // An empty repository restores nothing, so the restored state is the first
    // commit. Identity is set per-invocation rather than relying on a global
    // config that may not exist.
    git(&[
        "-c",
        "user.name=keel",
        "-c",
        "user.email=keel@localhost",
        "commit",
        "-q",
        "-m",
        "chore: store restored from a Parquet backup",
    ])?;
    Ok(true)
}

fn run_fixture(home: &PathBuf, json: bool) -> Result<()> {
    let mut store = open(home)?;
    let summary = fixture::load(&mut store)?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "entities": summary.entities,
                "links": summary.links,
                "revisions": summary.revisions,
                "total_entities": summary.total_entities(),
                "total_links": summary.total_links(),
            }))?
        );
    } else {
        println!(
            "loaded {} entities, {} links, {} document revisions",
            summary.total_entities(),
            summary.total_links(),
            summary.revisions
        );
        for (ty, n) in &summary.entities {
            println!("  {n:>4}  {ty}");
        }
    }
    Ok(())
}

fn run_status(home: &PathBuf, json: bool) -> Result<()> {
    use keel_core::{EntityQuery, EntityStore, EntityType};
    let store = open(home)?;

    let projects = store.list(&EntityQuery::default().of_type(EntityType::Project))?;
    let open_tasks = store.list(
        &EntityQuery::default()
            .of_type(EntityType::Task)
            .with_status(["todo", "in_progress", "blocked", "review"]),
    )?;
    let open_questions = store.list(
        &EntityQuery::default()
            .of_type(EntityType::Question)
            .with_status(["open"]),
    )?;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "home": home.display().to_string(),
                "projects": projects.total,
                "open_tasks": open_tasks.total,
                "open_questions": open_questions.total,
            }))?
        );
    } else {
        println!("{}", home.display());
        println!("  {} project(s)", projects.total);
        println!("  {} open task(s)", open_tasks.total);
        println!("  {} open question(s)", open_questions.total);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod restore_git_tests {
    use super::init_store_git;

    fn have_git() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    #[test]
    fn a_restored_store_becomes_a_git_repository_with_its_state_committed() {
        // SPEC §11 tier 1. `restore` rebuilds from tier 2 into a fresh
        // directory, and used to hand back a store with no `.git` — so a
        // restore silently cost you the recovery tier with the most fidelity.
        if !have_git() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("keel.duckdb"), b"not really a database").unwrap();

        assert!(init_store_git(dir.path()).unwrap(), "it created the repo");
        assert!(dir.path().join(".git").exists());
        assert!(dir.path().join(".gitignore").exists());

        // An empty repository restores nothing, so the state must be committed.
        let log = std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["log", "--oneline"])
            .output()
            .unwrap();
        assert!(log.status.success(), "the repo has a HEAD");
        assert!(
            String::from_utf8_lossy(&log.stdout).contains("restored"),
            "the restored state is the first commit"
        );

        // Model weights are large and re-downloadable; committing them would
        // make the recovery tier unusable.
        assert!(
            std::fs::read_to_string(dir.path().join(".gitignore"))
                .unwrap()
                .contains("models/")
        );
    }

    #[test]
    fn an_existing_repository_is_left_alone() {
        // Restoring into a directory that is already a repo must not reinit it
        // and lose its history — which is exactly the loss this whole fix is
        // about, just in the other direction.
        if !have_git() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        assert!(!init_store_git(dir.path()).unwrap(), "it did nothing");
        assert!(
            !dir.path().join(".gitignore").exists(),
            "and wrote nothing over the existing repo"
        );
    }
}
