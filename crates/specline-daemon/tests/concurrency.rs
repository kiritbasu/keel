//! Phase 1's exit criterion: two concurrent sessions writing produce zero
//! duplicates and zero lost updates.
//!
//! This is the test that was written in Phase 0 as `#[ignore]` and lives here
//! now that there is something to run it against. It could not live in
//! `specline-core`: a `Store` is one connection and is not `Sync` on purpose,
//! because D-5 says the daemon owns the single write path. Driving two stores
//! at one file would have tested SQLite's own locking, which is not the claim
//! being made.
//!
//! The claim being made is that **many concurrent agent sessions, going
//! through the daemon, cannot duplicate an entity or silently lose an edit.**

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{Value, json};
use specline_daemon::{AppState, router};
use specline_mcp::protocol::PROTOCOL_VERSION;
use std::sync::Arc;

/// A daemon plus a client, shareable across tasks.
#[derive(Clone)]
struct Client {
    base: Arc<String>,
    http: reqwest::Client,
}

impl Client {
    async fn call(&self, tool: &str, args: Value) -> (u16, Value) {
        let body = json!({
            "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": {
                "name": tool,
                "arguments": args,
                "_meta": { "io.modelcontextprotocol/protocolVersion": PROTOCOL_VERSION }
            }
        });
        let response = self
            .http
            .post(format!("{}/mcp", self.base))
            .header("content-type", "application/json")
            .header("MCP-Protocol-Version", PROTOCOL_VERSION)
            .header("Mcp-Method", "tools/call")
            .header("Mcp-Name", tool)
            .json(&body)
            .send()
            .await
            .unwrap();
        let status = response.status().as_u16();
        let json: Value = response.json().await.unwrap_or(Value::Null);
        (status, json)
    }

    async fn ok(&self, tool: &str, args: Value) -> Value {
        let (status, body) = self.call(tool, args).await;
        assert_eq!(status, 200, "{tool}: {body}");
        body["result"]["structuredContent"].clone()
    }
}

async fn start() -> (Client, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let state = AppState::open(dir.path(), false).unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, router(state)).await;
    });
    (
        Client {
            base: Arc::new(format!("http://{addr}")),
            http: reqwest::Client::new(),
        },
        dir,
    )
}

/// How many simultaneous sessions to simulate.
///
/// Sixteen rather than two. The criterion says "two concurrent sessions", but
/// two is exactly the number at which a race can pass by luck; sixteen makes
/// an interleaving failure overwhelmingly likely to show up.
const SESSIONS: usize = 16;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_identical_creates_produce_exactly_one_entity() {
    let (client, _dir) = start().await;
    let project = client
        .ok(
            "specline_create",
            json!({"type": "project", "title": "Contended"}),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    // Every session tries to create the same task at the same moment, the way
    // several agents told the same thing by the same human would.
    let mut handles = Vec::new();
    for i in 0..SESSIONS {
        let client = client.clone();
        let project = project.clone();
        handles.push(tokio::spawn(async move {
            client
                .ok(
                    "specline_create",
                    json!({
                        "type": "task",
                        "project": project,
                        "title": "Fix the rounding bug",
                        "summary": "Totals are a penny out on some invoices. Done when the \
                                    arithmetic matches the ledger.",
                        "session_id": format!("ses_concurrent_{i}")
                    }),
                )
                .await
        }));
    }

    let mut ids = Vec::new();
    let mut created_count = 0;
    for h in handles {
        let result = h.await.unwrap();
        ids.push(result["entity"]["id"].as_str().unwrap().to_owned());
        if result["created"] == json!(true) {
            created_count += 1;
        }
    }

    assert_eq!(
        created_count, 1,
        "exactly one caller may be told it created the task; {created_count} were"
    );
    let distinct: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(
        distinct.len(),
        1,
        "every caller must receive the same entity; got {} distinct ids",
        distinct.len()
    );

    // And the store agrees.
    let listed = client
        .ok(
            "specline_search",
            json!({"query": "rounding bug", "types": ["task"]}),
        )
        .await;
    assert_eq!(
        listed["hits"].as_array().unwrap().len(),
        1,
        "zero duplicates"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_updates_lose_nothing_when_each_writer_retries() {
    // Zero lost updates. Every session appends a distinct label; if any write
    // is silently dropped, the final label set is short.
    let (client, _dir) = start().await;
    let project = client
        .ok(
            "specline_create",
            json!({"type": "project", "title": "Contended"}),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let task = client
        .ok(
            "specline_create",
            json!({
                "type": "task", "project": project, "title": "Accumulate labels",
                "summary": "Two writers add labels at once. Done when neither drops the other's."
            }),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut handles = Vec::new();
    for i in 0..SESSIONS {
        let client = client.clone();
        let task = task.clone();
        handles.push(tokio::spawn(async move {
            let label = format!("session-{i}");
            // The loop an agent is expected to run on a 409: re-read, merge,
            // retry. If the 409 payload did not carry the current state this
            // would be unwriteable without a full round trip, which is why
            // SPEC §7.3 specifies it.
            for attempt in 0..64 {
                let current = client.ok("specline_get", json!({"ids": [task]})).await;
                let entity = &current["artifacts"][0]["entity"];
                let version = entity["version"].as_i64().unwrap();
                let mut labels: Vec<String> = entity["labels"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_owned)
                            .collect()
                    })
                    .unwrap_or_default();
                labels.push(label.clone());

                let (status, _) = client
                    .call(
                        "specline_update",
                        json!({
                            "id": task,
                            "version": version,
                            "changes": { "labels": labels },
                            "session_id": format!("ses_concurrent_{i}")
                        }),
                    )
                    .await;

                match status {
                    200 => return true,
                    409 => {
                        // Contended. Back off a little and re-read.
                        tokio::time::sleep(std::time::Duration::from_millis(
                            2 + (attempt % 7) as u64,
                        ))
                        .await;
                    }
                    other => panic!("unexpected status {other}"),
                }
            }
            false
        }));
    }

    for h in handles {
        assert!(h.await.unwrap(), "a writer gave up after 64 attempts");
    }

    let final_state = client.ok("specline_get", json!({"ids": [task]})).await;
    let labels: Vec<String> = final_state["artifacts"][0]["entity"]["labels"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();

    let distinct: std::collections::HashSet<&String> = labels.iter().collect();
    assert_eq!(
        distinct.len(),
        SESSIONS,
        "every session's write must survive; {} of {SESSIONS} did. Missing: {:?}",
        distinct.len(),
        (0..SESSIONS)
            .map(|i| format!("session-{i}"))
            .filter(|l| !distinct.contains(l))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        labels.len(),
        SESSIONS,
        "and none may be applied twice: {labels:?}"
    );

    // The version counter agrees with the number of writes that landed.
    let version = final_state["artifacts"][0]["entity"]["version"]
        .as_i64()
        .unwrap();
    assert_eq!(
        version,
        SESSIONS as i64 + 1,
        "version should be one per successful write, plus the create"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_writers_produce_a_gapless_event_log() {
    // The event log is what "what changed since I last looked" rests on. If
    // concurrent writes could interleave badly enough to produce out-of-order
    // ULIDs, a cursor query would silently skip rows (DECISIONS B-9).
    let (client, _dir) = start().await;
    let project = client
        .ok(
            "specline_create",
            json!({"type": "project", "title": "Busy"}),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut handles = Vec::new();
    for i in 0..SESSIONS {
        let client = client.clone();
        let project = project.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..4 {
                client
                    .ok(
                        "specline_create",
                        json!({
                            "type": "task",
                            "project": project,
                            "title": format!("Task {i}-{j}"),
                            "summary": "A row this test needs in the store.",
                            "session_id": format!("ses_concurrent_{i}")
                        }),
                    )
                    .await;
            }
        }));
    }
    for h in handles {
        h.await.unwrap();
    }

    // Walk the log by cursor, in small pages, and check it is complete and
    // strictly increasing.
    let mut seen: Vec<String> = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut args = json!({ "project": project, "limit": 5 });
        if let (Some(c), Some(obj)) = (&cursor, args.as_object_mut()) {
            obj.insert("cursor".to_owned(), json!(c));
        }
        let page = client.ok("specline_activity", args).await;
        let events = page["events"].as_array().unwrap();
        if events.is_empty() {
            break;
        }
        for e in events {
            seen.push(e["id"].as_str().unwrap().to_owned());
        }
        cursor = page["cursor"].as_str().map(str::to_owned);
    }

    let mut sorted = seen.clone();
    sorted.sort();
    assert_eq!(
        seen, sorted,
        "event ids must be strictly increasing, or a cursor query skips rows"
    );

    let distinct: std::collections::HashSet<&String> = seen.iter().collect();
    assert_eq!(distinct.len(), seen.len(), "no event may be returned twice");

    // One creation event per task, plus the project.
    assert_eq!(
        seen.len(),
        SESSIONS * 4 + 1,
        "every write must have produced exactly one event"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_links_between_the_same_pair_produce_one_edge() {
    let (client, _dir) = start().await;
    let project = client
        .ok(
            "specline_create",
            json!({"type": "project", "title": "Linked"}),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let task = client
        .ok(
            "specline_create",
            json!({
                "type": "task", "project": project, "title": "Implement it",
                "summary": "A row this test needs, so an edge has something to point at."
            }),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let spec = client
        .ok(
            "specline_create",
            json!({
                "type": "spec",
                "project": project,
                "title": "The spec",
                // A prose-bearing type has to arrive with prose (KEEL-171).
                // This test is about edges, not bodies, but the rule does not
                // make exceptions for what a test happens to care about.
                "body": "The spec these concurrent links point at.",
            }),
        )
        .await["entity"]["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let mut handles = Vec::new();
    for _ in 0..SESSIONS {
        let client = client.clone();
        let (task, spec) = (task.clone(), spec.clone());
        handles.push(tokio::spawn(async move {
            client
                .ok(
                    "specline_link",
                    json!({"from": task, "rel": "implements", "to": spec}),
                )
                .await["link"]["id"]
                .as_str()
                .unwrap()
                .to_owned()
        }));
    }

    let mut ids = Vec::new();
    for h in handles {
        ids.push(h.await.unwrap());
    }
    let distinct: std::collections::HashSet<&String> = ids.iter().collect();
    assert_eq!(
        distinct.len(),
        1,
        "re-asserting the same true fact concurrently must yield one edge, not {}",
        distinct.len()
    );

    let neighbours = client
        .ok(
            "specline_get",
            json!({"ids": [spec], "depth": 1, "direction": "inbound"}),
        )
        .await;
    assert_eq!(
        neighbours["artifacts"][0]["neighbours"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
