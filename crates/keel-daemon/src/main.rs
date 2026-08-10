//! `keel-daemon` — the binary.
//!
//! Argument parsing and process lifecycle only. Everything else lives in the
//! library half of this crate, so integration tests can drive the real router
//! rather than a re-implementation of it.

use anyhow::{Context, Result};
use clap::Parser;
use keel_daemon::{AppState, router};
use std::net::SocketAddr;
use std::path::PathBuf;

/// The Keel daemon.
#[derive(Parser, Debug)]
#[command(
    name = "keel-daemon",
    version,
    about = "Keel's MCP and local API daemon"
)]
struct Args {
    /// Where the store lives. Defaults to `~/.keel`.
    #[arg(long, env = "KEEL_HOME")]
    home: Option<PathBuf>,

    /// Address to bind.
    ///
    /// Localhost by default and it should stay that way until Phase 5: the
    /// daemon has no authentication, and the MCP transport requires `Origin`
    /// validation precisely because a local server is reachable from any web
    /// page the user happens to have open.
    #[arg(long, default_value = "127.0.0.1:7654")]
    bind: SocketAddr,

    /// Load the local embedding model, enabling semantic search.
    ///
    /// Off by default because the first run downloads it. Keyword search works
    /// either way, so this degrades rather than breaking.
    #[arg(long)]
    embeddings: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "keel=info,keel_daemon=info,tower_http=warn".into()),
        )
        .init();

    let args = Args::parse();
    let home = match args.home {
        Some(h) => h,
        None => {
            let base = std::env::var_os("HOME")
                .map(PathBuf::from)
                .context("HOME is not set; pass --home")?;
            base.join(".keel")
        }
    };

    let state = AppState::open(&home, args.embeddings)
        .with_context(|| format!("open the Keel store at {}", home.display()))?;

    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind(args.bind)
        .await
        .with_context(|| format!("bind {}", args.bind))?;

    tracing::info!(
        home = %home.display(),
        bind = %args.bind,
        protocol = keel_mcp::PROTOCOL_VERSION,
        "keel-daemon listening"
    );
    tracing::info!("  MCP endpoint  http://{}/mcp", args.bind);
    tracing::info!("  local API     http://{}/api", args.bind);

    // Graceful shutdown, but on a deadline.
    //
    // `with_graceful_shutdown` waits for in-flight connections, and `/api/events`
    // is a Server-Sent Events stream that by design never ends. So the daemon
    // would sit there after SIGTERM until someone lost patience and sent
    // SIGKILL — which is how an ART index ends up disagreeing with its table,
    // and how this project spent an evening chasing a store that looked
    // corrupt while `fsck` insisted it was clean.
    let deadline = std::time::Duration::from_secs(5);
    let serving = axum::serve(listener, app).with_graceful_shutdown(shutdown());
    tokio::select! {
        result = serving => result.context("serve")?,
        () = expire(deadline) => tracing::warn!(
            "graceful shutdown exceeded {deadline:?} — an open SSE stream is the usual \
             reason. Closing anyway, after a checkpoint."
        ),
    }

    // The last thing, always. An unflushed write is the whole failure mode.
    match state.store().checkpoint() {
        Ok(()) => tracing::info!("checkpointed; the write handle is released cleanly"),
        Err(e) => tracing::error!(error = %e, "checkpoint failed — the store may need a restore"),
    }
    Ok(())
}

/// Wait for the shutdown signal, then allow `grace` for in-flight work.
async fn expire(grace: std::time::Duration) {
    shutdown().await;
    tokio::time::sleep(grace).await;
}

/// Wait for Ctrl-C or SIGTERM.
///
/// Graceful shutdown matters more here than in most servers: the daemon holds
/// the only write handle, and killing it mid-write is the one way to leave the
/// two engines disagreeing.
async fn shutdown() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down; the write handle is released cleanly");
}
