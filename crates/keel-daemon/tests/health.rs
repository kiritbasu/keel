//! `/api/health` must answer while the store is busy.
//!
//! It is the probe the CLI uses to decide whether a daemon owns the store, so
//! it is asked at exactly the moment a slow write is in progress. It used to
//! take the store lock, which meant a `keel generate` holding that lock for
//! thirty seconds made health hang for thirty seconds — the CLI concluded the
//! daemon was unreachable and opened the store itself. The probe produced the
//! second writer it existed to prevent.
//!
//! # Why these tests have no sleeps in them
//!
//! A first draft held the lock for a fixed 600 ms and asserted health returned
//! inside 400 ms. It passed alone and failed once under a full `cargo test
//! --workspace`, where the machine is loaded and 400 ms of wall clock means
//! nothing. A timing assertion that cries wolf is worse than none: it teaches
//! whoever sees it to re-run rather than look.
//!
//! So the lock is held until the test says otherwise, and the assertion is a
//! generous timeout. Without the fix health waits for a lock nothing will
//! release, so the timeout fires every time; with it, health answers at once
//! and the number is never close to the boundary.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_daemon::{AppState, http::router};
use serde_json::Value;
use std::sync::mpsc;
use std::time::Duration;

/// Start a daemon and keep a handle on its state, so a test can hold the lock
/// the way a slow request would.
async fn daemon() -> (String, AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path(), false).expect("open the store");
    let app = router(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    (format!("http://{addr}"), state, dir)
}

async fn health(base: &str) -> Value {
    reqwest::get(format!("{base}/api/health"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap()
}

/// Holds the store lock on another thread until told to let go.
///
/// The guard cannot be held across an await here — a `std::sync::MutexGuard`
/// across an await is what deadlocks a single-threaded runtime, and clippy
/// refuses it even in a test — so it lives on a thread, and the channels make
/// the handover deterministic rather than timed.
struct HeldLock {
    release: mpsc::Sender<()>,
    thread: std::thread::JoinHandle<()>,
}

impl HeldLock {
    fn take(state: AppState) -> Self {
        let (acquired, wait_for_acquired) = mpsc::channel();
        let (release, wait_for_release) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let guard = state.store();
            acquired.send(()).expect("report that the lock is held");
            let _ = wait_for_release.recv();
            drop(guard);
        });
        wait_for_acquired
            .recv()
            .expect("the lock was taken before the test continued");
        HeldLock { release, thread }
    }

    fn give_back(self) {
        let _ = self.release.send(());
        self.thread.join().unwrap();
    }
}

#[tokio::test]
async fn health_answers_while_another_request_holds_the_store() {
    let (base, state, _dir) = daemon().await;

    // Warm the cached count, the way any first request does.
    let first = health(&base).await;
    assert_eq!(first["status"], "ok");
    assert_eq!(first["store_busy"], false);

    let held = HeldLock::take(state);

    let busy = tokio::time::timeout(Duration::from_secs(2), health(&base))
        .await
        .expect("health waited for the store lock — the probe is the bug again");

    assert_eq!(busy["status"], "ok");
    assert_eq!(
        busy["store_busy"], true,
        "a health answer given without the store must say the numbers may be stale"
    );
    assert_eq!(
        busy["projects"], 0,
        "it should report the last count it saw rather than inventing one"
    );

    held.give_back();

    // And once the lock is free it reads the store again.
    let after = health(&base).await;
    assert_eq!(after["store_busy"], false);
}

/// A busy daemon still says what it is, so a version check against health does
/// not turn into an outage the moment a write runs long.
#[tokio::test]
async fn a_busy_daemon_still_reports_its_version_and_protocol() {
    let (base, state, _dir) = daemon().await;
    let held = HeldLock::take(state);

    let busy = tokio::time::timeout(Duration::from_secs(2), health(&base))
        .await
        .expect("health must not wait for the store lock");

    assert_eq!(busy["status"], "ok");
    assert!(busy["version"].is_string());
    assert!(busy["protocol"].is_string());

    held.give_back();
}

/// The schema number is what another binary compares itself against before
/// writing through this daemon, so it has to be there and it has to be the
/// number this build actually ships — not the package version, which moves for
/// reasons that have nothing to do with the tables.
#[tokio::test]
async fn health_reports_the_schema_a_caller_should_compare_against() {
    let (base, _state, _dir) = daemon().await;

    let body = health(&base).await;

    assert_eq!(
        body["schema"],
        keel_core::shipped_schema_version(),
        "a CLI cannot tell whether this daemon is older without this"
    );
}

/// And it survives a busy store, for the same reason the version does: a check
/// that fails during a long write would refuse writes exactly when the daemon
/// is working hardest.
#[tokio::test]
async fn the_schema_is_reported_without_the_store() {
    let (base, state, _dir) = daemon().await;
    let held = HeldLock::take(state);

    let busy = tokio::time::timeout(Duration::from_secs(2), health(&base))
        .await
        .expect("health must not wait for the store lock");
    assert_eq!(busy["schema"], keel_core::shipped_schema_version());

    held.give_back();
}
