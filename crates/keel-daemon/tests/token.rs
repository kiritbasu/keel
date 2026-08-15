//! Who is allowed to change something.
//!
//! The daemon listens on loopback, which every process and every open web page
//! can reach. The `Origin` check turns away an ordinary cross-origin request
//! and cannot help against DNS rebinding, where the attacker's page arrives
//! looking same-origin — so before there was a token, "a person clicked it" and
//! "a page did it" were the same request (KEEL-168, KEEL-238).
//!
//! These tests are about the guard rather than the secret. That the token is
//! unguessable and its file unreadable by anyone else is `keel_core::token`'s
//! property and is tested there.
//!
//! **`/api/update/restart` is the endpoint used throughout, and only ever in
//! its refusing direction.** Letting a request through would `exec` the test
//! binary into the daemon, which is a thing to be quite careful not to do.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_daemon::{AppState, TOKEN_HEADER, http::router};

const TOKEN: &str = "a-known-token";

async fn daemon() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let store = keel_core::Store::open(keel_core::store_path(dir.path())).expect("open the store");
    let state = AppState::from_store_with_token(store, TOKEN);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), dir)
}

/// The case the whole thing exists for: a page that reached this origin, with
/// no way to have read the token, must not be able to change anything.
#[tokio::test]
async fn a_mutating_request_without_the_token_is_refused() {
    let (base, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .post(format!("{base}/api/update/restart"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    let message = body["error"].as_str().unwrap_or_default();
    assert!(
        message.contains(TOKEN_HEADER),
        "the refusal must name the header that would work: {message}"
    );
}

/// A token from a daemon that has since restarted. Same refusal, and the
/// message has to say so, because "it worked five minutes ago" is exactly when
/// somebody assumes the endpoint is broken.
#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (base, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .post(format!("{base}/api/update/restart"))
        .header(TOKEN_HEADER, "a-token-from-an-earlier-daemon")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 401);
    let body: serde_json::Value = response.json().await.unwrap();
    assert!(
        body["error"]
            .as_str()
            .unwrap_or_default()
            .contains("earlier daemon"),
        "and it should say that a token has the lifetime of one daemon"
    );
}

/// A near miss must not pass. The comparison is constant-time, which is exactly
/// the kind of code where a prefix quietly counting as a match would go
/// unnoticed.
#[tokio::test]
async fn a_token_that_is_only_a_prefix_is_refused() {
    let (base, _dir) = daemon().await;

    for offered in ["a-known", "a-known-tokenx", ""] {
        let response = reqwest::Client::new()
            .post(format!("{base}/api/update/restart"))
            .header(TOKEN_HEADER, offered)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{offered:?} must not be accepted");
    }
}

/// Reads are not behind the token, and must not be. The interface fetches
/// health before it has anything to send, `keel doctor` reads through the
/// daemon, and a read surface that needed a secret would make looking at a busy
/// store harder at exactly the moment it matters.
#[tokio::test]
async fn reading_needs_no_token() {
    let (base, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/api/health"))
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
}

/// The guard is registered per route, so the test that matters most is that
/// every mutating route actually carries it. A new endpoint added to the wrong
/// router is the failure this cannot detect by inspection.
#[tokio::test]
async fn every_mutating_endpoint_is_behind_the_token() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    for path in ["/api/generate", "/api/update/apply", "/api/update/restart"] {
        let response = client
            .post(format!("{base}{path}"))
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            401,
            "{path} answered without a token, so it is not behind the guard"
        );
    }
}
