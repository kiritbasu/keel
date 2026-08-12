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

// --- Malformed input ------------------------------------------------------
//
// `/mcp` had never been fed anything that was not a well-formed request. A
// JSON-RPC client cannot act on a bare 400 with an HTML body or an empty one:
// it needs the envelope, because that is the only thing it knows how to read.
// A server that stops speaking the protocol at the first bad byte is one whose
// errors are indistinguishable from being down.

/// Every one of these is broken in a different way, and every one has to come
/// back as JSON-RPC.
#[tokio::test]
async fn malformed_bodies_come_back_as_json_rpc_errors() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    let cases: Vec<(&str, String)> = vec![
        ("not json at all", "this is not json".to_owned()),
        ("empty", String::new()),
        ("a bare array", "[]".to_owned()),
        ("a bare string", "\"hello\"".to_owned()),
        ("truncated", "{\"jsonrpc\": \"2.0\", \"method\":".to_owned()),
        ("no method", json!({"jsonrpc": "2.0", "id": 1}).to_string()),
        (
            "method is not a string",
            json!({"jsonrpc": "2.0", "id": 1, "method": 42}).to_string(),
        ),
        (
            "unknown method",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/teleport"}).to_string(),
        ),
        (
            "tools/call with no name",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {}}).to_string(),
        ),
        (
            "arguments is a string",
            json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                   "params": {"name": "keel_search", "arguments": "query"}})
            .to_string(),
        ),
    ];

    for (what, body) in cases {
        let response = client
            .post(format!("{base}/mcp"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap();

        let status = response.status();
        assert!(
            status.is_client_error() || status.is_success(),
            "{what}: a malformed request is the caller's fault, not a 5xx — got {status}"
        );

        let payload: Value = response
            .json()
            .await
            .unwrap_or_else(|e| panic!("{what}: the answer was not JSON at all ({e})"));
        assert_eq!(
            payload["jsonrpc"], "2.0",
            "{what}: a JSON-RPC client can only read the envelope — got {payload}"
        );
        let message = payload
            .pointer("/error/message")
            .and_then(Value::as_str)
            .unwrap_or_else(|| panic!("{what}: no error message in {payload}"));
        assert!(
            !message.is_empty(),
            "{what}: an empty message tells the caller nothing"
        );
    }
}

/// The one thing a malformed request must never do is take the daemon with it.
#[tokio::test]
async fn a_daemon_fed_rubbish_keeps_answering() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    for junk in ["", "{", "\u{0}\u{0}\u{0}", "[[[[[[[[", "{\"jsonrpc\":"] {
        let _ = client
            .post(format!("{base}/mcp"))
            .header("content-type", "application/json")
            .body(junk)
            .send()
            .await;
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
        "a real call after the rubbish must still work"
    );
}

// --- Blob responses -------------------------------------------------------

/// A blob is bytes an agent put in the store while reading prose it did not
/// write, and the daemon hands them to a renderer. Two headers stand between
/// that and script execution.
///
/// The sharp case is SVG: a document that may contain `<script>`, served with
/// an image media type. A diagram written by a prompt-influenced agent is
/// stored cross-site scripting the moment something renders it as a document
/// rather than as an image.
#[tokio::test]
async fn a_blob_is_served_with_headers_that_stop_it_executing() {
    let (base, _dir) = daemon().await;
    let client = reqwest::Client::new();

    let created: Value = client
        .post(format!("{base}/mcp"))
        .json(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                      "params": {"name": "keel_create",
                                 "arguments": {"type": "project", "title": "Blobs",
                                               "slug": "blobs"}}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(created.pointer("/result").is_some(), "{created}");

    // A one-pixel PNG, base64. The bytes do not matter; the headers do.
    let png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let design: Value = client
        .post(format!("{base}/mcp"))
        .json(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
                      "params": {"name": "keel_create",
                                 "arguments": {"type": "design", "project": "blobs",
                                               "title": "A mockup", "image": png}}}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let blob_id = design
        .pointer("/result/structuredContent/entity/blob_id")
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("no blob id in {design}"));

    let response = client
        .get(format!("{base}/api/blob/{blob_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned()
    };

    assert_eq!(
        header("x-content-type-options"),
        "nosniff",
        "without this the declared media type is a suggestion, and a browser is free to \
         decide a blob starting with `<` is really HTML"
    );
    let csp = header("content-security-policy");
    assert!(
        csp.contains("sandbox"),
        "an SVG is a document that may contain <script>; the sandbox is what denies it \
         whatever it turns out to contain: {csp:?}"
    );
    assert!(csp.contains("default-src 'none'"), "{csp:?}");
}
