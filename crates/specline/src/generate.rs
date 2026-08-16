//! `specline generate` — write a project's repository files from Specline.
//!
//! # Why this goes through the daemon
//!
//! D-5 says everything other than the daemon either connects read-only or goes
//! through the daemon's API. Under the old engine the read-only half did not
//! exist at all — it refused a second connection outright while the daemon held
//! the write lock — so the API was the only route. SQLite in WAL mode does
//! permit that second reader, so the constraint is now Specline's rather than the
//! engine's.
//!
//! The API is still the right answer, for the reason that was always the better
//! one: generation is exactly the operation you want to run against a *live*
//! store, because the whole point is that the repository reflects what Specline
//! currently holds. A reader going round the daemon sees a consistent snapshot,
//! but not necessarily the write the daemon is in the middle of.
//!
//! The direct fallback exists for the case the API cannot serve: no daemon
//! running at all. Opening the store then is unambiguous, because nothing is
//! writing to it.

use anyhow::{Context, Result, bail};
use specline_core::{Mode, generate};
use std::path::{Path, PathBuf};

/// Run a generation, preferring the daemon.
pub fn run(
    home: &Path,
    project: &str,
    repo: Option<PathBuf>,
    check: bool,
    daemon: &str,
    json: bool,
) -> Result<()> {
    let report = match via_daemon(home, daemon, project, repo.as_deref(), check) {
        Ok(report) => report,
        Err(e) => {
            // The fallback used to fire on *any* transport error, including
            // the thirty-second timeout you get from a daemon that is alive
            // and busy. That is the case most likely to produce a second
            // writer — a slow generate against a working daemon — so the
            // fallback caused the thing it was meant to route around.
            //
            // `may_read_directly` asks the only question with a safe answer:
            // is anything holding the port. Connection refused means no; a
            // timeout means yes and busy, which is a reason to stop.
            // `{e}` already names the daemon and what was being attempted, so
            // this says only what *this* step establishes: whether falling back
            // to a direct read is safe. Wrapping it in a second "ask the daemon
            // at … to generate" printed the address twice and the verb twice.
            crate::writes::may_read_directly(daemon).with_context(|| e.to_string())?;
            tracing::debug!(error = %e, "no daemon is running, opening the store directly");
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
                "{} file(s) differ from Specline, {} orphaned, {} current",
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

/// The daemon's report, mirroring [`specline_core::GenerateReport`] on the wire.
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
    home: &Path,
    base: &str,
    project: &str,
    repo: Option<&std::path::Path>,
    check: bool,
) -> Result<specline_core::GenerateReport> {
    let mut body = serde_json::json!({ "project": project, "check": check });
    if let Some(repo) = repo {
        body["repo"] = serde_json::Value::String(repo.display().to_string());
    }

    // Generation writes files, so the daemon wants the token (KEEL-238). Read
    // it from beside the store rather than caching it: each daemon mints its
    // own, and a stale one is exactly what a restart leaves behind.
    //
    // A missing token is not fatal here. The daemon will say what is wrong far
    // better than a guess can, and refusing locally would turn "the daemon is
    // not running" into a confusing complaint about a file.
    let token = specline_core::token::read(home)
        .unwrap_or_default()
        .unwrap_or_default();

    let response = ureq::post(&format!("{base}/api/generate"))
        .set(specline_daemon::TOKEN_HEADER, &token)
        .timeout(std::time::Duration::from_secs(30))
        .send_json(body)
        .map_err(|e| match e {
            // The daemon writes a sentence saying what would work. `ureq`'s
            // own Display for a status error is "status code 401", which is
            // the least actionable half of what arrived — and this is the one
            // failure a person can actually do something about.
            ureq::Error::Status(status, response) => {
                let explanation = response
                    .into_string()
                    .ok()
                    .and_then(|body| {
                        serde_json::from_str::<serde_json::Value>(&body)
                            .ok()
                            .and_then(|v| v["error"].as_str().map(str::to_owned))
                    })
                    .unwrap_or_else(|| format!("HTTP {status}"));
                anyhow::anyhow!("the daemon at {base} refused to generate: {explanation}")
            }
            other => anyhow::anyhow!("ask the daemon at {base} to generate: {other}"),
        })?;

    let wire: serde_json::Value = response
        .into_json()
        .context("read the daemon's generate response")?;
    let data = wire.get("data").unwrap_or(&wire).clone();
    let report: WireReport =
        serde_json::from_value(data).context("parse the daemon's generate response")?;

    Ok(specline_core::GenerateReport {
        written: report.written,
        unchanged: report.unchanged,
        unrepresented: report.unrepresented,
        orphans: report.orphans,
    })
}

fn directly(
    home: &Path,
    project: &str,
    repo: Option<PathBuf>,
    check: bool,
) -> Result<specline_core::GenerateReport> {
    // Not `Store::open`, which creates when the file is absent — generating
    // from a store that was made a microsecond ago writes an empty mirror over
    // a full one (KEEL-137).
    let store = crate::open(home)
        .context("no daemon answered either, so there is no way to read Specline")?;
    let found = crate::resolve_project(&store, project)?;
    let root = repo_root_for(&found, repo)?;
    let mode = if check { Mode::Check } else { Mode::Write };
    Ok(generate::all(&store, found.id(), &root, mode)?)
}

/// Resolve where to write, preferring an explicit flag over the recorded path.
pub fn repo_root_for(
    project: &specline_core::Entity,
    explicit: Option<PathBuf>,
) -> Result<PathBuf> {
    if let Some(p) = explicit {
        return Ok(p);
    }
    match project {
        specline_core::Entity::Project(p) => p.root_path.as_ref().map(PathBuf::from).with_context(|| {
            format!(
                "{} has no root_path recorded, so there is nowhere to write. Pass --repo, or set \
                 root_path on the project",
                p.slug
            )
        }),
        _ => bail!("not a project"),
    }
}
