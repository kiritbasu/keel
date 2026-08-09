//! `keel` — the command-line client.
//!
//! Deliberately thin. Everything it does lives in `keel-core`; this crate
//! resolves a store path, parses arguments, and prints. That split is what
//! lets the daemon expose the same operations without either of them growing a
//! dependency on the other.
//!
//! Phase 0 gives it `fsck`, `backup`, `restore` and `fixture`. `render-status`
//! arrives in Phase 1, with the dogfooding switch.

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

    /// Print a one-line summary of what is in the store.
    Status,
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
    }
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
