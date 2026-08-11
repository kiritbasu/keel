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
mod work;

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
    Fsck {
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        ///
        /// Read through the daemon when one is running: it holds DuckDB's write
        /// lock and no second process can open the store while it does.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

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

    /// Copy the DuckDB-and-Lance store into a new SQLite one, and check it.
    ///
    /// Reads only. The old store is never modified, so a failed run costs
    /// nothing and can be repeated. Stop the daemon first — it holds DuckDB's
    /// write lock, and there is exactly one writer.
    Migrate {
        /// Where to write the SQLite store. Defaults to `<home>/keel.sqlite`.
        #[arg(long)]
        target: Option<PathBuf>,
        /// Check an existing SQLite store against the old one without copying.
        #[arg(long)]
        verify_only: bool,
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
    Status {
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        ///
        /// Read through the daemon when one is running: it holds DuckDB's write
        /// lock and no second process can open the store while it does.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

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
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
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
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// Print the generated tracker for a project to standard output.
    RenderStatus {
        /// Project id, slug or name. Must match exactly one project.
        project: String,
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
        /// Write here instead of standard output.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Write even if the result is dramatically smaller than what is there.
        #[arg(long)]
        force: bool,
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

    /// Report the rows a reader would struggle with. Never rewrites one.
    ///
    /// Three rules arrived after most of this store existed — a task needs a
    /// summary, a close needs a reason, prose should not lean on a bare
    /// identifier — and none of them can be enforced backwards. This is the
    /// list a person works through.
    ///
    /// It does not fix anything, and that is the design: a machine filling in a
    /// missing summary would write exactly the confident, plausible, wrong
    /// prose the requirement exists to prevent.
    Lint {
        /// Project id, slug or name.
        project: String,
        /// Only this rule: task_without_summary, unexpanded_identifier,
        /// closed_without_reason.
        #[arg(long)]
        check: Option<String>,
        /// How many findings to print. The total is reported either way.
        #[arg(long, default_value_t = 40)]
        limit: usize,
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// What can be worked on right now, best first.
    ///
    /// Open work with nothing live in its way. Ordered by what a task unblocks
    /// before its priority, so a p1 that releases three others comes above a p0
    /// that releases nothing.
    ///
    /// The same computation the MCP tool and the app read. There is one
    /// ranking, so the three cannot disagree.
    Ready {
        /// Project id, slug or name.
        project: String,
        /// Only work nobody is holding.
        #[arg(long)]
        unclaimed: bool,
        /// Only tasks carrying all of these labels. Repeatable.
        #[arg(long = "label")]
        labels: Vec<String>,
        /// Skip tasks carrying any of these labels. Repeatable.
        #[arg(long = "no-label")]
        no_labels: Vec<String>,
        /// Only work under this milestone. An id, or a name like "Phase 8".
        #[arg(long)]
        milestone: Option<String>,
        /// How many to show.
        #[arg(long, default_value_t = 10)]
        limit: usize,
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// Take a task: move it to in_progress and record who is on it.
    ///
    /// Refused if another session holds it, unless that claim has gone stale
    /// after three days or `--force` is passed. Closing releases it.
    Claim {
        /// The task — `KEEL-42` or a `tsk_…` id.
        task: String,
        /// Take a claim another session still holds.
        #[arg(long)]
        force: bool,
        /// The session doing the work. Keel never invents one.
        #[arg(long, env = "KEEL_SESSION")]
        session: Option<String>,
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
    },

    /// Close a task, saying why and showing the work.
    ///
    /// `done` needs a message and at least one piece of evidence; `wont_do` and
    /// `no_change` need a message; `duplicate` and `superseded` name the other
    /// task and draw the edge themselves.
    Close {
        /// The task — `KEEL-42` or a `tsk_…` id.
        task: String,
        /// done, wont_do, duplicate, superseded, no_change.
        #[arg(long)]
        reason: String,
        /// What actually happened, in a sentence or two.
        #[arg(long, short)]
        message: String,
        /// Typed proof. `commit:<sha>`, `pr:<url>`, `test:<command>`,
        /// `doc:<entity-id>`, `url:<url>`, `image:<blob-id>`. Repeatable.
        #[arg(long = "evidence")]
        evidence: Vec<String>,
        /// For `duplicate` and `superseded`: the other task.
        #[arg(long)]
        other: Option<String>,
        /// The session closing it, for attribution.
        #[arg(long, env = "KEEL_SESSION")]
        session: Option<String>,
        /// Daemon base URL. Defaults to `$KEEL_DAEMON_URL`, then the local daemon.
        #[arg(long, env = "KEEL_DAEMON_URL", default_value = "http://127.0.0.1:7654")]
        daemon: String,
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
        Command::Fsck { daemon } => run_fsck(&home, daemon, cli.json),
        Command::Backup { dest } => run_backup(&home, dest.clone(), cli.json),
        Command::Restore { source, target } => run_restore(source, target, cli.json),
        Command::Migrate {
            target,
            verify_only,
        } => run_migrate(&home, target.clone(), *verify_only, cli.json),
        Command::Fixture => run_fixture(&home, cli.json),
        Command::Status { daemon } => run_status(&home, daemon, cli.json),
        Command::RenderStatus {
            project,
            daemon,
            out,
            force,
        } => run_render_status(&home, daemon, project, out.clone(), *force),
        Command::Note { action } => run_note(&home, action, cli.json),
        Command::Archive { id, version } => {
            use keel_core::{Actor, EntityId, EntityStore, Provenance, Surface};
            let mut store = open(&home)?;
            let prov = Provenance::anonymous(Actor::Human).with_surface(Surface::Cli);
            let archived = store.archive(&EntityId::parse(id)?, *version, &prov)?;
            println!("{} — archived", archived.id());
            Ok(())
        }
        Command::Lint {
            project,
            check,
            limit,
            daemon,
        } => work::lint(daemon, project, check.as_deref(), *limit, cli.json),
        Command::Ready {
            project,
            unclaimed,
            labels,
            no_labels,
            milestone,
            limit,
            daemon,
        } => work::ready(
            &home,
            daemon,
            project,
            *unclaimed,
            labels,
            no_labels,
            milestone.as_deref(),
            *limit,
            cli.json,
        ),
        Command::Claim {
            task,
            force,
            session,
            daemon,
        } => work::claim(&home, daemon, task, *force, session.as_deref(), cli.json),
        Command::Close {
            task,
            reason,
            message,
            evidence,
            other,
            session,
            daemon,
        } => work::close(
            &home,
            daemon,
            task,
            reason,
            message,
            evidence,
            other.as_deref(),
            session.as_deref(),
            cli.json,
        ),
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
    let mut matches: Vec<keel_core::Entity> = projects
        .items
        .into_iter()
        .filter(|p| match p {
            Entity::Project(pr) => {
                pr.id.as_str() == reference
                    || pr.slug.eq_ignore_ascii_case(reference)
                    || pr.name.to_lowercase() == needle
            }
            _ => false,
        })
        .collect();

    // More than one match is refused rather than resolved by taking the first.
    // Silently picking one is how a render lands in the wrong project's file.
    if matches.len() > 1 {
        let names: Vec<String> = matches
            .iter()
            .map(|p| match p {
                Entity::Project(pr) => format!("{} ({})", pr.slug, pr.id),
                other => other.id().to_string(),
            })
            .collect();
        anyhow::bail!(
            "`{reference}` matches {} projects: {}. Name one exactly.",
            names.len(),
            names.join(", ")
        );
    }

    matches
        .pop()
        .with_context(|| format!("no project matches `{reference}`"))
}

/// How much smaller than the file it replaces a render may be before it stops
/// and asks.
///
/// A tracker does not lose half its content by accident. Pointed at the wrong
/// project — one that is near-empty — this is what stands between a mistyped
/// argument and a file nobody kept a copy of.
const SHRINK_FLOOR: f64 = 0.5;

fn run_render_status(
    home: &PathBuf,
    daemon: &str,
    project: &str,
    out: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    // Percent-encode by hand rather than adding a crate for it. A project
    // reference is a slug, key or name, so the reachable characters are few —
    // but a name with a space or an ampersand would otherwise truncate the
    // query and render the wrong project's tracker, which is silent and wrong.
    let encoded: String = project
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect();
    let path_and_query = format!("/api/render-status?project={encoded}");
    let markdown = match read_via_daemon(daemon, &path_and_query)? {
        Some(v) => v
            .get("markdown")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .context("the daemon's render-status response had no markdown")?,
        None => {
            let store = open(home)?;
            let found = resolve_project(&store, project)?;
            keel_core::render_status::render(&store, found.id())?
        }
    };
    let Some(path) = out else {
        print!("{markdown}");
        return Ok(());
    };

    // Compare before writing. The previous version wrote unconditionally, which
    // meant a regeneration that changed nothing still dirtied the tree, and a
    // regeneration that destroyed everything looked exactly the same.
    //
    // Compared with the banner stripped, because the banner carries a
    // generation timestamp: byte equality would never hold and the comparison
    // would be decoration. This is the same rule `keel generate --check` uses,
    // and using a different one here is how the two would disagree about
    // whether a file had changed.
    let existing = std::fs::read_to_string(&path).ok();
    if let Some(before) = &existing
        && keel_core::generate::strip_banner_public(before)
            == keel_core::generate::strip_banner_public(&markdown)
    {
        println!("{} is already up to date", path.display());
        return Ok(());
    }

    if let Some(before) = &existing
        && !force
        && !before.is_empty()
        && (markdown.len() as f64) < before.len() as f64 * SHRINK_FLOOR
    {
        anyhow::bail!(
            "refusing to write {}: the new tracker is {} bytes and the file there is {}. \
             That is the shape of a render pointed at the wrong project — check `{}` is the \
             one you meant, or pass --force if the shrink is real.",
            path.display(),
            markdown.len(),
            before.len(),
            project
        );
    }

    std::fs::write(&path, &markdown).with_context(|| format!("write {}", path.display()))?;
    println!("wrote {} ({} bytes)", path.display(), markdown.len());
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

    let mut task = Task::new(
        found.id().clone(),
        title,
        "A row this test needs in the store.",
    );
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

/// Ask the daemon for a read, returning `None` if it is not answering.
///
/// The store has one writer and DuckDB will not grant a second connection
/// while it holds the lock, so a read-shaped command has two choices: go
/// through the daemon, or work only when the daemon is stopped. The second is
/// what `fsck` used to do, and an integrity check you must stop the thing you
/// want to check in order to run is not much of a check (TQ-15, KEEL-57).
///
/// `None` means "no daemon answered", which is a normal state — nothing is
/// holding the lock, so opening the store directly is correct and safe. A
/// daemon that answers with an *error* is a different thing and is returned as
/// one, because silently falling back would hide a real failure behind a
/// conflicting-lock error a moment later.
fn read_via_daemon(base: &str, path: &str) -> Result<Option<serde_json::Value>> {
    let response = match ureq::get(&format!("{base}{path}"))
        .timeout(std::time::Duration::from_secs(30))
        .call()
    {
        Ok(r) => r,
        // A 404 is not the daemon declining — it is a daemon that predates the
        // endpoint, which means it is older than this binary. Falling back would
        // open the store it is holding the lock on and fail with a
        // conflicting-lock error that names none of this.
        Err(ureq::Error::Status(404, _)) => {
            bail!(
                "the daemon at {base} does not know {path}, so it is older than this binary.\n\n\
                 Restart it from a current build: `./plugin/install.sh` then `keel-daemon`.\n\
                 Until then this command can only run with the daemon stopped."
            );
        }
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let message = serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|v| {
                    v.pointer("/error/message")
                        .and_then(|m| m.as_str())
                        .map(str::to_owned)
                })
                .unwrap_or(body);
            bail!("the daemon at {base} refused {path} ({code}): {message}");
        }
        Err(_) => return Ok(None),
    };
    let body: serde_json::Value = response
        .into_json()
        .with_context(|| format!("read the daemon's response to {path}"))?;
    Ok(Some(body.get("data").cloned().unwrap_or(body)))
}

fn open(home: &PathBuf) -> Result<DuckStore> {
    DuckStore::open(home).with_context(|| format!("open the store at {}", home.display()))
}

fn run_fsck(home: &PathBuf, daemon: &str, json: bool) -> Result<()> {
    let report: fsck::FsckReport = match read_via_daemon(daemon, "/api/fsck")? {
        Some(v) => serde_json::from_value(v).context("parse the daemon's fsck report")?,
        None => fsck::check(&open(home)?)?,
    };

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

/// Copy the old store into a new SQLite one, then check the two agree.
///
/// Nothing here writes to the DuckDB store. The copy goes into a file beside
/// it, so the old store stays the working one until someone deliberately
/// switches over — which is what makes a failed migration cost nothing.
///
/// The verification is the point rather than the copy, so it runs every time
/// and a dirty result is a non-zero exit. A migration that reported success
/// without comparing anything would be worse than no migration command.
fn run_migrate(
    home: &PathBuf,
    target: Option<PathBuf>,
    verify_only: bool,
    json: bool,
) -> Result<()> {
    let old = open(home)?;
    let target = target.unwrap_or_else(|| home.join("keel.sqlite"));

    let mut new = keel_core::store::SqliteStore::open(&target)?;

    if !verify_only {
        let report = keel_core::migrate::migrate(&old, &mut new)?;
        if !json {
            println!("{report}");
        }
    }

    let verification = keel_core::migrate::verify(&old, &new)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&verification)?);
    } else {
        println!("{verification}");
        println!("\nold store: {}", home.display());
        println!("new store: {}", target.display());
    }

    if !verification.is_clean() {
        // Non-zero, and loudly. The whole value of this command is that it
        // refuses to say the two stores agree when they do not.
        anyhow::bail!(
            "the two stores do not agree — {} difference(s). The old store is untouched; \
             delete {} and run again once the cause is understood",
            verification.differences.len(),
            target.display()
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

fn run_status(home: &PathBuf, daemon: &str, json: bool) -> Result<()> {
    use keel_core::{EntityQuery, EntityStore, EntityType};

    let counts = match read_via_daemon(daemon, "/api/status")? {
        Some(v) => v,
        None => {
            let store = open(home)?;
            serde_json::json!({
                "projects": store
                    .list(&EntityQuery::default().of_type(EntityType::Project))?
                    .total,
                "open_tasks": store
                    .list(
                        &EntityQuery::default()
                            .of_type(EntityType::Task)
                            .with_status(["todo", "in_progress", "review"]),
                    )?
                    .total,
                "open_questions": store
                    .list(
                        &EntityQuery::default()
                            .of_type(EntityType::Question)
                            .with_status(["open"]),
                    )?
                    .total,
            })
        }
    };
    let n = |k: &str| {
        counts
            .get(k)
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0)
    };
    let (projects, open_tasks, open_questions) =
        (n("projects"), n("open_tasks"), n("open_questions"));

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "home": home.display().to_string(),
                "projects": projects,
                "open_tasks": open_tasks,
                "open_questions": open_questions,
            }))?
        );
    } else {
        println!("{}", home.display());
        println!("  {projects} project(s)");
        println!("  {open_tasks} open task(s)");
        println!("  {open_questions} open question(s)");
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod render_status_tests {
    //! The one genuine data-loss path in the CLI.
    //!
    //! `render-status --out` used to write unconditionally. Point it at a
    //! near-empty project by accident and it replaced a real tracker with that
    //! project's stub — no comparison, no backup, no way to tell afterwards.

    /// A port nothing listens on, so these exercise the direct-store path.
    ///
    /// Pinned rather than left to the default: the default is the real daemon,
    /// and a test that quietly passes only when the developer's daemon happens
    /// to be stopped is a test that fails in CI for reasons nobody can see.
    const NO_DAEMON: &str = "http://127.0.0.1:9";

    use super::*;
    use keel_core::{Actor, EntityStore, Project, Provenance};

    fn store_with(slugs: &[(&str, &str)]) -> (tempfile::TempDir, DuckStore) {
        let dir = tempfile::tempdir().unwrap();
        let mut store = DuckStore::open(dir.path()).unwrap();
        for (slug, name) in slugs {
            store
                .create(
                    Project::new(*slug, *name).into(),
                    &Provenance::anonymous(Actor::Human),
                )
                .unwrap();
        }
        (dir, store)
    }

    #[test]
    fn an_ambiguous_reference_is_refused_rather_than_resolved_to_the_first() {
        // One project answers to `keel` as its slug, another as its name.
        // Two projects sharing a *name* cannot both exist — near-duplicate
        // detection catches that on create — so this is the collision that is
        // actually reachable, and picking one of the two silently is how a
        // render lands in the wrong project's file.
        let (_d, store) = store_with(&[("keel", "Keel Project"), ("other", "keel")]);
        let err = resolve_project(&store, "keel").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("matches 2 projects"), "{message}");
        assert!(message.contains("keel"), "it must name them: {message}");
    }

    #[test]
    fn an_exact_slug_still_resolves_when_a_similar_one_exists() {
        let (_d, store) = store_with(&[("keel", "Keel"), ("keel-web", "Keel Web")]);
        let found = resolve_project(&store, "keel").unwrap();
        assert_eq!(found.label(), "Keel");
    }

    #[test]
    fn a_reference_that_names_nothing_says_so() {
        let (_d, store) = store_with(&[("keel", "Keel")]);
        assert!(resolve_project(&store, "harbour").is_err());
    }

    #[test]
    fn a_dramatically_smaller_render_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("STATUS.md");
        std::fs::write(&path, "x".repeat(10_000)).unwrap();

        let (home, _store) = store_with(&[("empty", "Empty")]);
        let err = run_render_status(
            &home.path().to_path_buf(),
            NO_DAEMON,
            "empty",
            Some(path.clone()),
            false,
        )
        .unwrap_err();

        let message = err.to_string();
        assert!(message.contains("refusing to write"), "{message}");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap().len(),
            10_000,
            "and the file it refused to write is untouched"
        );
    }

    #[test]
    fn force_writes_the_smaller_file_anyway() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("STATUS.md");
        std::fs::write(&path, "x".repeat(10_000)).unwrap();

        let (home, _store) = store_with(&[("empty", "Empty")]);
        run_render_status(
            &home.path().to_path_buf(),
            NO_DAEMON,
            "empty",
            Some(path.clone()),
            true,
        )
        .unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().len() < 10_000);
    }

    #[test]
    fn an_unchanged_render_does_not_rewrite_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("STATUS.md");
        let (home, _store) = store_with(&[("keel", "Keel")]);
        let home = home.path().to_path_buf();

        run_render_status(&home, NO_DAEMON, "keel", Some(path.clone()), false).unwrap();
        let first = std::fs::metadata(&path).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(20));
        run_render_status(&home, NO_DAEMON, "keel", Some(path.clone()), false).unwrap();
        let second = std::fs::metadata(&path).unwrap().modified().unwrap();

        assert_eq!(
            first, second,
            "regenerating an unchanged tracker must not dirty the tree"
        );
    }
}
