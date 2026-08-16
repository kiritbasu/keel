//! What the daemon holds the store for, and for how long.
//!
//! The daemon serialises the whole store behind one mutex. That is the right
//! shape at one user — but only if the critical section is the database work.
//! A generate used to hold it through several dozen filesystem writes as well,
//! so regenerating a project's files made the daemon unresponsive to everything
//! else for the length of the slowest disk it was running on.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use specline_core::{Actor, EntityStore, Project, Provenance, Store};
use specline_daemon::{AppState, http::router};

/// A daemon with one project whose checkout is `repo`.
async fn daemon(repo: &std::path::Path) -> (String, String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(dir.path().join("specline.sqlite")).unwrap();

    let mut project = Project::new("demo", "Demo");
    project.root_path = Some(repo.display().to_string());
    store
        .create(project.into(), &Provenance::anonymous(Actor::Human))
        .unwrap();

    let state = AppState::from_store(store);
    // Generation mutates, so it is behind the token (KEEL-238). Taken from the
    // state rather than written out here, so a change to how a test daemon is
    // built cannot leave these tests quietly asserting 401.
    let token = state.token().to_owned();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), token, dir)
}

fn tool_call(name: &str, arguments: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": name, "arguments": arguments},
    })
}

/// A generate through the daemon still produces the files, now that deciding
/// and writing are two phases with the lock dropped in between.
#[tokio::test]
async fn a_generate_through_the_daemon_writes_the_files() {
    let repo = tempfile::tempdir().unwrap();
    let (base, token, _dir) = daemon(repo.path()).await;

    let response = reqwest::Client::new()
        .post(format!("{base}/api/generate"))
        .header("x-specline-token", &token)
        .json(&json!({"project": "demo"}))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let body: Value = response.json().await.unwrap();
    let written = body["data"]["written"].as_array().unwrap();
    assert!(
        !written.is_empty(),
        "a generate that wrote nothing has not been verified: {body}"
    );
    assert!(
        repo.path().join(".specline/manifest.json").is_file(),
        "the mirror manifest should exist after a generate"
    );
}

/// Requests that need the store make progress while a generate is running.
///
/// Not a timing assertion — those cry wolf on a loaded machine, and this suite
/// has a note about that in `health.rs`. This is the weaker, deterministic
/// claim: a generate and a pile of store-taking calls issued against the same
/// daemon at the same moment all complete. Before the split the generate held
/// the lock across its filesystem tail, and this is the shape of test that
/// would have caught a regression to that by taking the length of the disk
/// writes to finish rather than deadlocking outright.
#[tokio::test]
async fn a_generate_does_not_stop_the_daemon_answering() {
    let repo = tempfile::tempdir().unwrap();
    let (base, token, _dir) = daemon(repo.path()).await;
    let client = reqwest::Client::new();

    let generating = {
        let client = client.clone();
        let base = base.clone();
        tokio::spawn(async move {
            client
                .post(format!("{base}/api/generate"))
                .header("x-specline-token", &token)
                .header("x-specline-token", &token)
                .json(&json!({"project": "demo"}))
                .send()
                .await
                .unwrap()
                .status()
        })
    };

    let mut others = Vec::new();
    for _ in 0..20 {
        let client = client.clone();
        let base = base.clone();
        others.push(tokio::spawn(async move {
            let store_call = client
                .post(format!("{base}/mcp"))
                .json(&tool_call("specline_projects", json!({})))
                .send()
                .await
                .unwrap()
                .status();
            let probe = client
                .get(format!("{base}/api/health"))
                .send()
                .await
                .unwrap()
                .status();
            (store_call, probe)
        }));
    }

    assert_eq!(generating.await.unwrap(), 200);
    for other in others {
        let (store_call, probe) = other.await.unwrap();
        assert_eq!(store_call, 200, "a store-taking call was not served");
        assert_eq!(probe, 200, "the health probe was not served");
    }
}
