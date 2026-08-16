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
//!
//! # The warm-up, which is not politeness
//!
//! `daemon()` makes one throwaway request before handing back, and the timed
//! sections would be meaningless without it. Measured on macOS: the *first*
//! HTTP request a test process makes takes **12.7 seconds** and the second
//! takes **0.9 milliseconds**. That is `reqwest` building its client and
//! loading the system trust store, and it has nothing whatever to do with the
//! daemon — but a two-second assertion that happens to be the first request
//! measures it and blames the store lock.
//!
//! This bit once already. Two tests here failed for a day looking exactly like
//! a health regression, and the only difference between them and the one that
//! passed was that the passing one made a call before starting its clock.

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
    let base = format!("http://{addr}");

    // Pay the client's one-off startup cost here, outside anything timed. See
    // the module doc: this is twelve seconds on a cold process.
    let _ = health(&base).await;

    (base, state, dir)
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

/// Nothing staged means nothing to apply, and the refusal says so.
///
/// The endpoint B-75 permits takes no arguments, so this is the only way it can
/// be wrong: asked to apply when there is nothing there. It must refuse rather
/// than restart.
///
/// The request carries the daemon's token, because the endpoint is behind it
/// now (KEEL-238) — and that is the point of sending it here: this test is
/// about the *empty* apply being refused, so it has to get past the guard to
/// reach the thing it is testing. `tests/token.rs` covers the guard itself.
#[tokio::test]
async fn applying_an_update_that_is_not_staged_is_refused() {
    let (base, state, home) = daemon().await;
    let token = keel_core::token::read(home.path())
        .expect("read the token")
        .expect("a daemon mints one at startup");

    let response = reqwest::Client::new()
        .post(format!("{base}/api/update/apply"))
        .header("x-keel-token", token)
        .send()
        .await
        .expect("the apply endpoint answers");

    assert_eq!(response.status(), 400, "an empty apply must not succeed");
    let body = response.text().await.unwrap_or_default();
    assert!(
        body.contains("nothing is staged"),
        "the refusal should say what is missing, got: {body}"
    );
    drop(state);
}

/// Health carries what the daemon already knows, so the interface needs no new
/// endpoint to show that an update is waiting.
#[tokio::test]
async fn health_reports_whether_an_update_is_staged() {
    let (base, state, _home) = daemon().await;

    let body = health(&base).await;
    assert!(
        body.get("staged_version").is_some(),
        "health should always carry the field, null when nothing is staged: {body}"
    );
    assert!(
        body["staged_version"].is_null(),
        "nothing was staged in this test, so it should be null: {body}"
    );
    drop(state);
}

/// KEEL-227. `staged_version: null` says only "nothing is waiting", and that
/// reads as "you are current" whether the daemon checked an hour ago, has been
/// failing quietly for a month, or has checks switched off entirely. Three
/// states, one appearance — and the further behind you are, the less it says.
#[tokio::test]
async fn health_says_whether_update_checking_is_happening_at_all() {
    let (base, state, _home) = daemon().await;

    let body = health(&base).await;
    let check = body
        .get("update_check")
        .expect("health has to carry the state of checking, not only its result");

    // Present from this version on. Its *absence* is what tells the interface
    // it is talking to a daemon older than the updater, which is the whole
    // mechanism — no outbound request, no known-latest to compare against.
    assert!(
        check.get("enabled").is_some(),
        "whether checks are on is a state a person chose and should see: {body}"
    );
    assert!(
        check.get("last_checked_at").is_some(),
        "the field is always there, null when no check has completed: {body}"
    );
    assert!(
        check["last_checked_at"].is_null(),
        "no check has run in this test: {body}"
    );

    // Which binary, not only which version. Two installs with the one on your
    // PATH not being the one you updated is the case this exists for.
    assert!(
        body["executable"]
            .as_str()
            .is_some_and(|p| p.contains("keel")),
        "health should name the binary it is running: {body}"
    );
    drop(state);
}

/// KEEL-220. Two of the three release targets cannot link the ONNX runtime, so
/// `keel 0.1.x` on Intel macOS and `keel 0.1.x` on arm64 are different binaries
/// with the same name. A version number cannot say which; this can.
#[tokio::test]
async fn health_says_whether_this_build_can_embed_at_all() {
    let (base, state, _home) = daemon().await;

    let body = health(&base).await;
    let embeddings = body
        .get("embeddings")
        .expect("health has to say what this build can do, not only what it is called");

    assert_eq!(
        embeddings["built_in"],
        serde_json::json!(keel_daemon::EMBEDDINGS_BUILT_IN),
        "the reported capability must be this binary's, not a guess: {body}"
    );

    // Three answers, not two: cannot, could and has not yet, is. `loaded` is
    // null only when the store was busy — which is not the same as no model,
    // and reporting the second for the first is how "semantic search is off"
    // gets believed about a daemon that is merely mid-write.
    assert!(
        embeddings.get("loaded").is_some(),
        "the field is always present: {body}"
    );
    drop(state);
}

/// The half that is a property of the build rather than of this moment, checked
/// against what the daemon will actually do with `--embeddings`.
#[tokio::test]
async fn a_build_without_a_model_never_reports_one_as_loaded() {
    let (base, state, _home) = daemon().await;
    let body = health(&base).await;

    if !keel_daemon::EMBEDDINGS_BUILT_IN {
        assert_ne!(
            body["embeddings"]["loaded"],
            serde_json::json!(true),
            "a build with no model in it cannot have one loaded: {body}"
        );
    }
    drop(state);
}

/// KEEL-258. The check a person can ask for, rather than waiting up to an hour
/// for the timer.
///
/// This asserts the route exists and is guarded, not that it finds a release —
/// the outcome depends on what is published at the moment the test runs, and a
/// test that reaches the network is a test that fails on a train.
#[tokio::test]
async fn the_update_check_endpoint_exists_and_is_behind_the_token() {
    let (base, state, _home) = daemon().await;

    let unauthenticated = reqwest::Client::new()
        .post(format!("{base}/api/update/check"))
        .send()
        .await
        .expect("the route answers");

    assert_eq!(
        unauthenticated.status().as_u16(),
        401,
        "checking is a mutating action and sits behind the same token as applying"
    );
    drop(state);
}
