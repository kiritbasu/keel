//! `keel generate` — write a project's repository files from Keel.
//!
//! # Why this goes through the daemon
//!
//! D-5 says everything other than the daemon either connects read-only or goes
//! through the daemon's API. The read-only half turns out not to exist:
//! **DuckDB refuses a read-only connection while any process holds the write
//! lock**, so a second process cannot read the store while the daemon is
//! running — and the daemon is always running. Verified, not assumed; the
//! error is the same conflicting-lock message a writer gets.
//!
//! That leaves the API, which is the right answer anyway. Generation is
//! exactly the operation you want to run against a *live* store, because the
//! whole point is that the repository reflects what Keel currently holds.
//!
//! The direct fallback exists for the case the API cannot serve: no daemon
//! running at all. Opening the store then is safe precisely because nothing
//! else holds it.

use anyhow::{Context, Result, bail};
use keel_core::{DuckStore, Mode, generate};
use std::path::PathBuf;

/// Run a generation, preferring the daemon.
pub fn run(
    home: &PathBuf,
    project: &str,
    repo: Option<PathBuf>,
    check: bool,
    daemon: &str,
    json: bool,
) -> Result<()> {
    let report = match via_daemon(daemon, project, repo.as_deref(), check) {
        Ok(report) => report,
        Err(e) => {
            tracing::debug!(error = %e, "daemon unavailable, opening the store directly");
            directly(home, project, repo, check)?
        }
    };

    let changed = !report.written.is_empty() || !report.orphans.is_empty();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "written": report.written,
                "unchanged": report.unchanged,
                "unrepresented": report.unrepresented,
                "orphans": report.orphans,
                "checked": check,
            }))?
        );
    } else {
        for path in &report.written {
            println!("  {} {path}", if check { "stale" } else { "wrote" });
        }
        for path in &report.orphans {
            // Named individually and never merely counted: this is the only
            // thing generation deletes, and a deletion nobody can see is how
            // a tool stops being trusted.
            println!("  {} {path}", if check { "orphaned" } else { "removed" });
        }
        for note in &report.unrepresented {
            println!("  skipped {note}");
        }
        if !changed {
            println!("up to date ({} files)", report.unchanged.len());
        } else if check {
            println!(
                "{} file(s) differ from Keel, {} orphaned, {} current",
                report.written.len(),
                report.orphans.len(),
                report.unchanged.len()
            );
        } else {
            println!(
                "{} file(s) written, {} removed, {} unchanged",
                report.written.len(),
                report.orphans.len(),
                report.unchanged.len()
            );
        }
    }

    // `--check` is meant for a hook, so it has to fail loudly. A plain run
    // that wrote files is a success — writing is the job.
    if check && changed {
        std::process::exit(1);
    }
    Ok(())
}

/// The daemon's report, mirroring [`keel_core::GenerateReport`] on the wire.
#[derive(serde::Deserialize)]
struct WireReport {
    written: Vec<String>,
    unchanged: Vec<String>,
    #[serde(default)]
    unrepresented: Vec<String>,
    #[serde(default)]
    orphans: Vec<String>,
}

fn via_daemon(
    base: &str,
    project: &str,
    repo: Option<&std::path::Path>,
    check: bool,
) -> Result<keel_core::GenerateReport> {
    let mut body = serde_json::json!({ "project": project, "check": check });
    if let Some(repo) = repo {
        body["repo"] = serde_json::Value::String(repo.display().to_string());
    }

    let response = ureq::post(&format!("{base}/api/generate"))
        .timeout(std::time::Duration::from_secs(30))
        .send_json(body)
        .map_err(|e| anyhow::anyhow!("ask the daemon at {base} to generate: {e}"))?;

    let wire: serde_json::Value = response
        .into_json()
        .context("read the daemon's generate response")?;
    let data = wire.get("data").unwrap_or(&wire).clone();
    let report: WireReport =
        serde_json::from_value(data).context("parse the daemon's generate response")?;

    Ok(keel_core::GenerateReport {
        written: report.written,
        unchanged: report.unchanged,
        unrepresented: report.unrepresented,
        orphans: report.orphans,
    })
}

fn directly(
    home: &PathBuf,
    project: &str,
    repo: Option<PathBuf>,
    check: bool,
) -> Result<keel_core::GenerateReport> {
    let store = DuckStore::open(home).with_context(|| {
        format!(
            "open the store at {}. No daemon answered either, so there is no way to read Keel",
            home.display()
        )
    })?;
    let found = crate::resolve_project(&store, project)?;
    let root = repo_root_for(&found, repo)?;
    let mode = if check { Mode::Check } else { Mode::Write };
    Ok(generate::all(&store, found.id(), &root, mode)?)
}

/// Resolve where to write, preferring an explicit flag over the recorded path.
pub fn repo_root_for(project: &keel_core::Entity, explicit: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    match project {
        keel_core::Entity::Project(p) => p.root_path.as_ref().map(PathBuf::from).with_context(|| {
            format!(
                "{} has no root_path recorded, so there is nowhere to write. Pass --repo, or set \
                 root_path on the project",
                p.slug
            )
        }),
        _ => bail!("not a project"),
    }
}
