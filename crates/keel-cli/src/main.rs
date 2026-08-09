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
mod generate;
mod import;

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

    /// Print the generated tracker for a project to standard output.
    RenderStatus {
        /// Project id, slug or name.
        project: String,
        /// Write here instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
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
    Ok(())
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
