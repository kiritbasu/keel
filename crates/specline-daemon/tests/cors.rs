//! Which routes a page on another origin can reach.
//!
//! The daemon's CORS layer sits on the read routes and **not** on the mutating
//! ones, because `guarded` is merged into the router after the layer is
//! applied. That started as an accident of ordering; these tests make it the
//! intent, because it is the safer of the two shapes and nothing needs the
//! other one (B-89).
//!
//! The reason to pin it rather than leave it implicit is that it is invisible
//! from the interface people actually use. The app is served by the daemon, so
//! every call it makes is same-origin and never preflighted — a change here
//! would break nothing anybody would notice until something else called in.
//! KEEL-309 is what that costs: a session added `PATCH` to the allow-list
//! believing it was needed, and only a test showed that `POST` was not reaching
//! the list either.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use specline_daemon::{AppState, TOKEN_HEADER, http::router};

const TOKEN: &str = "a-known-token";

/// Somewhere a browser might plausibly be, that is not the daemon.
const OTHER_LOCAL_ORIGIN: &str = "http://localhost:3000";

async fn daemon() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store =
        specline_core::Store::open(specline_core::store_path(dir.path())).expect("open the store");
    {
        use specline_core::EntityStore;
        store
            .create(
                specline_core::Project::new("harbour", "Harbour").into(),
                &specline_core::Provenance::anonymous(specline_core::Actor::Claude),
            )
            .unwrap();
    }
    let state = AppState::from_store_with_token(store, TOKEN);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state)).await;
    });
    (format!("http://{addr}"), dir)
}

/// Whether a response told the browser the origin may read it.
fn allows_origin(response: &reqwest::Response) -> bool {
    response
        .headers()
        .contains_key("access-control-allow-origin")
}

/// Reads are reachable cross-origin, from a local origin, on purpose.
///
/// This is the half that was always intended, and it is asserted here so the
/// absence below reads as a decision rather than as the same bug twice.
#[tokio::test]
async fn a_read_is_reachable_from_another_local_origin() {
    let (base, _dir) = daemon().await;

    let response = reqwest::Client::new()
        .get(format!("{base}/api/health"))
        .header("origin", OTHER_LOCAL_ORIGIN)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    assert!(
        allows_origin(&response),
        "reads carry CORS deliberately: {:?}",
        response.headers()
    );
}

/// Writes are not, and that is the property worth keeping.
///
/// The token is what actually stops a hostile page — it is minted per daemon
/// and only a same-origin page can read it out of the document. But a second
/// thing standing between another origin and the write path costs nothing
/// while no other origin exists, and taking it away would be a change nobody
/// asked for.
///
/// If this test ever fails, somebody has moved `.merge(guarded)` above the CORS
/// layer. That may well be right — reviving the Tauri shell would need it — but
/// it is a decision, and this is the thing that makes them take it deliberately
/// rather than discover it.
#[tokio::test]
async fn a_write_is_not_reachable_from_another_origin() {
    let (base, _dir) = daemon().await;

    for (method, path) in [
        (reqwest::Method::POST, "/api/tasks"),
        (reqwest::Method::PATCH, "/api/tasks/tsk_whatever"),
        (reqwest::Method::POST, "/api/entity/tsk_whatever/notes"),
        (reqwest::Method::POST, "/api/tasks/tsk_whatever/close"),
        (reqwest::Method::POST, "/api/generate"),
    ] {
        // The preflight the browser would send first.
        let preflight = reqwest::Client::new()
            .request(reqwest::Method::OPTIONS, format!("{base}{path}"))
            .header("origin", OTHER_LOCAL_ORIGIN)
            .header("access-control-request-method", method.as_str())
            .header("access-control-request-headers", TOKEN_HEADER)
            .send()
            .await
            .unwrap();

        assert!(
            !allows_origin(&preflight),
            "{method} {path} answered a cross-origin preflight, so a page on \
             another origin can now attempt this write: {:?}",
            preflight.headers()
        );

        // And the request itself, for a client that skipped the preflight.
        let direct = reqwest::Client::new()
            .request(method.clone(), format!("{base}{path}"))
            .header("origin", OTHER_LOCAL_ORIGIN)
            .header(TOKEN_HEADER, TOKEN)
            .json(&json!({ "version": 1 }))
            .send()
            .await
            .unwrap();

        assert!(
            !allows_origin(&direct),
            "{method} {path} let another origin read its response: {:?}",
            direct.headers()
        );
    }
}

/// The near-miss the origin check exists for, at the CORS layer rather than at
/// the MCP endpoint where it is already tested. A domain an attacker can
/// register must not be mistaken for the loopback one.
#[tokio::test]
async fn a_lookalike_origin_is_not_treated_as_local() {
    let (base, _dir) = daemon().await;

    for origin in [
        "https://localhost.evil.example",
        "http://127.0.0.1.evil.example",
        "https://example.com",
    ] {
        let response = reqwest::Client::new()
            .get(format!("{base}/api/health"))
            .header("origin", origin)
            .send()
            .await
            .unwrap();

        assert!(
            !allows_origin(&response),
            "{origin} was treated as local: {:?}",
            response.headers()
        );
    }
}
