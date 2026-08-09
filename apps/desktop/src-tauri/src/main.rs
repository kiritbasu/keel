//! The Tauri shell.
//!
//! Its whole job is to put a window around the React bundle and make sure the
//! daemon is running behind it. Nothing about Keel's behaviour lives here —
//! SPEC §1.2 is explicit that building daemon-first is what stops logic getting
//! trapped in the desktop app, and this file is what that discipline looks like
//! in practice.
//!
//! The frontend reaches the daemon at an absolute URL baked in at build time
//! (`VITE_KEEL_BASE`), because the webview is served from `tauri://localhost`
//! and a relative `/api` would never leave it. In development Vite proxies
//! `/api` instead, so the same source works both ways — which is the property
//! SPEC §10 wants: one bundle, different base URL.
//!
//! The daemon runs as a child process rather than being linked in. That is not
//! an accident of packaging: D-5 says one process owns the write handle, and
//! embedding the store in the UI would make the desktop app a second writer the
//! moment anyone opened it alongside a `keel` command.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::{Child, Command};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A daemon this app started, so it can be stopped again on exit.
///
/// `None` when the daemon was already running — in that case it belongs to
/// someone else (a terminal, a launchd job) and killing it on window close
/// would be rude and surprising.
struct Daemon(Mutex<Option<Child>>);

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .manage(Daemon(Mutex::new(None)))
        .setup(|app| {
            use tauri::Manager;
            let handle = app.handle().clone();
            let state = handle.state::<Daemon>();

            if daemon_is_up() {
                // Already running. Leave it alone.
                return Ok(());
            }

            match start_daemon() {
                Ok(child) => {
                    if let Ok(mut slot) = state.0.lock() {
                        *slot = Some(child);
                    }
                    // Wait briefly for it to bind, so the first screen does not
                    // flash an error before the daemon is ready.
                    let deadline = Instant::now() + Duration::from_secs(15);
                    while Instant::now() < deadline && !daemon_is_up() {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                }
                Err(e) => {
                    // Not fatal. The window still opens and the UI shows the
                    // "cannot reach the daemon" message, which tells the human
                    // exactly what to do — better than refusing to start.
                    eprintln!(
                        "keel-desktop: could not start the daemon ({e}). \
                         Start it yourself with `keel-daemon`."
                    );
                }
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Destroyed = event {
                use tauri::Manager;
                if let Some(state) = window.app_handle().try_state::<Daemon>()
                    && let Ok(mut slot) = state.0.lock()
                    && let Some(mut child) = slot.take()
                {
                    // Only ever a daemon this app started. Terminating one
                    // that was already running would take out whatever the
                    // human had pointed at it.
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("the Tauri runtime failed to start");
}

/// Whether something is already serving Keel's API.
fn daemon_is_up() -> bool {
    // A raw TCP connect rather than an HTTP client: this is asked once a second
    // during startup and the answer only needs to be "is the port open".
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:7654".parse().expect("a literal socket address"),
        Duration::from_millis(300),
    )
    .is_ok()
}

/// Launch the daemon.
///
/// Looks for a sibling binary first — the bundled case, where `keel-daemon`
/// ships inside the app — then falls back to `PATH`, which is the development
/// case and the one where someone installed with `plugin/install.sh`.
fn start_daemon() -> std::io::Result<Child> {
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("keel-daemon")))
        .filter(|p| p.exists());

    let program = sibling
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|| "keel-daemon".to_owned());

    Command::new(program)
        // Embeddings are off: the first run downloads a model, and a desktop
        // app that appears to hang on first launch is worse than one whose
        // search is keyword-only until asked otherwise.
        .env("KEEL_DAEMON_STARTED_BY", "desktop")
        .spawn()
}
