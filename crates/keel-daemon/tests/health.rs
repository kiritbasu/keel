//! `/api/health` must answer while the store is busy.
//!
//! It is the probe the CLI uses to decide whether a daemon owns the store, so
//! it is asked at exactly the moment a slow write is in progress. It used to
//! take the store lock, which meant a `keel generate` holding that lock for
//! thirty seconds made health hang for thirty seconds — the CLI concluded the
//! daemon was unreachable and opened the store itself. The probe produced the
//! second writer it existed to prevent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_daemon::{AppState, http::router};
use serde_json::Value;

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

#[tokio::test]
async fn health_answers_while_another_request_holds_the_store() {
    let (base, state, _dir) = daemon().await;

    // Warm the cached count, the way any first request does.
    let first = health(&base).await;
    assert_eq!(first["status"], "ok");
    assert_eq!(first["store_busy"], false);

    // Now hold the lock, as a slow generate or an embed would.
    let held = std::thread::spawn(move || {
        let guard = state.store();
        std::thread::sleep(std::time::Duration::from_millis(600));
        drop(guard);
    });

    // The whole point: this returns rather than waiting for the lock.
    let started = std::time::Instant::now();
    let busy = tokio::time::timeout(std::time::Duration::from_millis(400), health(&base))
        .await
        .expect("health blocked on the store lock — the probe is the bug again");
    let elapsed = started.elapsed();

    assert_eq!(busy["status"], "ok");
    assert_eq!(
        busy["store_busy"], true,
        "a health answer given without the store must say the numbers may be stale"
    );
    assert!(
        elapsed < std::time::Duration::from_millis(400),
        "health took {elapsed:?}, which means it waited for the lock"
    );

    held.join().unwrap();

    // And once the lock is free it reads the store again.
    let after = health(&base).await;
    assert_eq!(after["store_busy"], false);
}

/// A busy daemon reports the last count it knew rather than inventing one, and
/// never claims to be down.
#[tokio::test]
async fn a_busy_daemon_still_reports_its_version_and_protocol() {
    let (base, state, _dir) = daemon().await;

    // The guard is held on another thread rather than across the await here.
    // Holding a std Mutex across an await is the thing that deadlocks a
    // single-threaded runtime, and clippy is right to refuse it even in a test.
    let (release, wait) = std::sync::mpsc::channel::<()>();
    let held = std::thread::spawn(move || {
        let guard = state.store();
        let _ = wait.recv();
        drop(guard);
    });

    let busy = tokio::time::timeout(std::time::Duration::from_millis(400), health(&base))
        .await
        .expect("health must not wait for the store lock");

    assert_eq!(busy["status"], "ok");
    assert!(busy["version"].is_string());
    assert!(busy["protocol"].is_string());

    let _ = release.send(());
    held.join().unwrap();
}
