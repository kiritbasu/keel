//! The live-update stream, which the desktop app depends on and nothing tested.
//!
//! It is the surface where a break is least visible: an app whose event stream
//! has stopped looks exactly like an app nobody is writing to. There is no
//! error, no empty state, no spinner — just a screen that stays correct until
//! it is not.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use futures_util::StreamExt;
use serde_json::{Value, json};
use specline_daemon::{AppState, http::router};
use std::time::Duration;

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

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    })
}

/// Read from an SSE response until `want` appears in the bytes, or time runs
/// out.
///
/// Byte-oriented rather than parsed, because the framing is part of what is
/// being asserted: an event the client cannot see because it never got flushed
/// is the failure this covers, and a parser that waits for a complete message
/// would hide exactly that.
async fn read_until(response: reqwest::Response, want: &str, limit: Duration) -> String {
    let mut seen = String::new();
    let mut stream = response.bytes_stream();
    let deadline = tokio::time::Instant::now() + limit;
    while tokio::time::Instant::now() < deadline {
        let chunk = tokio::time::timeout_at(deadline, stream.next()).await;
        match chunk {
            Ok(Some(Ok(bytes))) => {
                seen.push_str(&String::from_utf8_lossy(&bytes));
                if seen.contains(want) {
                    return seen;
                }
            }
            Ok(Some(Err(e))) => panic!("the stream errored: {e}"),
            Ok(None) => break,
            Err(_) => break,
        }
    }
    seen
}

/// The stream says something the instant it opens, before any write.
///
/// Not politeness. A stream that sends no bytes is one an intermediary is free
/// to sit on — a buffering proxy holds the headers too, so the browser's
/// `EventSource` never fires `open` and live refresh is silently dead. That
/// happened behind the dev server's proxy, and nothing in the suite would have
/// caught it happening again.
#[tokio::test]
async fn the_stream_sends_something_before_the_first_write() {
    let (base, _state, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .starts_with("text/event-stream"),
        "the content type is what makes a browser treat this as a stream"
    );

    let seen = read_until(response, ": specline", Duration::from_secs(5)).await;
    assert!(
        seen.contains(": specline"),
        "nothing arrived before the first write; a proxy would hold the headers: {seen:?}"
    );
}

/// A write reaches an open stream.
#[tokio::test]
async fn a_create_reaches_an_open_stream() {
    let (base, _state, _dir) = daemon().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();

    // Subscribe first, then write. The other order is a race the test would
    // lose intermittently, which is worse than not testing it.
    let reading = tokio::spawn(read_until(
        response,
        "event: change",
        Duration::from_secs(10),
    ));
    tokio::time::sleep(Duration::from_millis(50)).await;

    let created = client
        .post(format!("{base}/mcp"))
        .json(&tool_call(
            "specline_create",
            json!({"type": "project", "title": "Streamed", "slug": "streamed"}),
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(created.status(), 200);

    let seen = reading.await.unwrap();
    assert!(
        seen.contains("event: change"),
        "a create did not reach the open stream, which is how a live app goes quiet \
         without anything saying so: {seen:?}"
    );
}

/// A subscriber that fell behind is told, rather than quietly missing writes.
///
/// The broadcast channel drops the oldest messages when a receiver lags. A
/// stream that swallowed that would leave a UI showing stale state for as long
/// as it stayed open — correct-looking and wrong, which is the failure this
/// codebase treats as the serious one. It should say `lagged` and let the
/// client refetch.
#[tokio::test]
async fn a_lagged_subscriber_is_told_and_the_daemon_keeps_serving() {
    let (base, state, _dir) = daemon().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base}/api/events"))
        .send()
        .await
        .unwrap();

    // Overrun the channel without reading, which is what a slow client does.
    // Announced directly rather than through writes: the point is the receiver
    // falling behind, and 512 real creates would be testing the store.
    tokio::time::sleep(Duration::from_millis(50)).await;
    for i in 0..512 {
        state.announce_note(None, format!("flood {i}"));
    }

    let seen = read_until(response, "event: lagged", Duration::from_secs(10)).await;
    assert!(
        seen.contains("event: lagged"),
        "a subscriber that missed messages was not told: {seen:?}"
    );

    // And the daemon is still answering, which is the half that matters more:
    // a wedged stream must not take the process with it.
    let after = client
        .get(format!("{base}/api/health"))
        .send()
        .await
        .unwrap();
    assert_eq!(after.status(), 200);
}
