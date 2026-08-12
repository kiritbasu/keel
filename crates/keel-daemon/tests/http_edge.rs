//! The daemon's HTTP edge: who may talk to it, and how much they may say.
//!
//! Three cheap holes in a server that listens on loopback and is therefore
//! reachable from every web page the user has open.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_daemon::{AppState, http::MAX_BODY_BYTES, http::router};
use serde_json::{Value, json};

async fn daemon() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path(), false).expect("open the store");
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), dir)
}

fn body() -> Value {
    json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list", "params": {}})
}

/// `Origin: null` is what a browser sends from a sandboxed iframe, a `file://`
/// page and a redirected cross-origin request — every context an attacker can
/// arrange, and none a real MCP client uses. It was allowed.
#[tokio::test]
async fn a_null_origin_is_refused() {
    let (base, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("origin", "null")
        .json(&body())
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 403);
    let payload: Value = response.json().await.unwrap();
    assert_eq!(payload["jsonrpc"], "2.0");
    assert!(
        payload["error"]["message"]
            .as_str()
            .unwrap()
            .contains("Origin"),
        "the refusal should say which check refused it: {payload}"
    );
}

#[tokio::test]
async fn a_local_origin_and_no_origin_are_both_allowed() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    for origin in [
        Some("http://localhost:1420"),
        Some("tauri://localhost"),
        None,
    ] {
        let mut request = client.post(format!("{base}/mcp")).json(&body());
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let status = request.send().await.unwrap().status();
        assert_eq!(status, 200, "origin {origin:?} should have been allowed");
    }
}

/// The limiter used to run before the origin check, so any page the user had
/// open could drain the budget with requests that were going to be refused —
/// one fetch from the attacker, the user's next tool call from the user.
#[tokio::test]
async fn a_refused_origin_does_not_spend_the_rate_limit() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    // Enough to exhaust any sane budget if these were being counted.
    for _ in 0..200 {
        let status = client
            .post(format!("{base}/mcp"))
            .header("origin", "https://evil.example")
            .json(&body())
            .send()
            .await
            .unwrap()
            .status();
        assert_eq!(status, 403);
    }

    let after = client
        .post(format!("{base}/mcp"))
        .json(&body())
        .send()
        .await
        .unwrap();
    assert_eq!(
        after.status(),
        200,
        "a legitimate call was rate-limited by requests the daemon had already refused"
    );
}

/// Axum's own answer to an oversized body is a bare 413 with no body at all,
/// which an MCP client reads as a broken server rather than as a request it
/// should make smaller.
#[tokio::test]
async fn an_oversized_body_is_refused_in_the_shape_the_caller_speaks() {
    let (base, _dir) = daemon().await;

    let huge = "x".repeat(MAX_BODY_BYTES + 1_024);
    let response = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .header("content-type", "application/json")
        .body(huge)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 413);
    let payload: Value = response
        .json()
        .await
        .expect("the refusal must be JSON, not an empty body");
    assert_eq!(payload["jsonrpc"], "2.0");
    let message = payload["error"]["message"].as_str().unwrap();
    assert!(
        message.contains(&MAX_BODY_BYTES.to_string()),
        "the error has to name the limit, or a caller cannot act on it: {message}"
    );
    assert!(
        message.contains("image_path"),
        "and it should say what to do instead: {message}"
    );
}

/// A body under the limit still works, which is the half that would break
/// silently if the limit were set too low.
#[tokio::test]
async fn a_body_under_the_limit_is_served_normally() {
    let (base, _dir) = daemon().await;

    let padded = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/list",
        "params": {"_padding": "x".repeat(64 * 1024)}
    });
    let response = reqwest::Client::new()
        .post(format!("{base}/mcp"))
        .json(&padded)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}
