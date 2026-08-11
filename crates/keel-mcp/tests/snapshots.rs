//! Snapshot tests for the tool surface.
//!
//! These are an API contract. An agent's behaviour is shaped by the tool
//! descriptions and by the exact shape of what comes back, so a change to
//! either is a change to the product — and it should show up as a reviewable
//! diff rather than as an agent quietly behaving differently next week.
//!
//! Ids and timestamps are redacted: they are the parts that legitimately
//! differ on every run, and a snapshot that churns is a snapshot people stop
//! reading.
//!
//! Run `cargo insta review` after an intentional change.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_core::{Actor, DuckStore, EntityStore, Project, Provenance, Spec, Task};
use keel_mcp::{ToolCall, dispatch};
use serde_json::{Value, json};

/// Replace the values that legitimately change each run.
fn settings() -> insta::Settings {
    let mut s = insta::Settings::clone_current();
    s.add_filter(
        r"(prj|tsk|spc|dec|que|trm|fbk|dsg|env|mtr|obs|art|lnk|evt|doc)_[0-9A-HJKMNP-TV-Z]{26}",
        "[id]",
    );
    s.add_filter(
        r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?(Z|[+-]\d{2}:\d{2})",
        "[timestamp]",
    );
    s.add_filter(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}", "[time]");
    s.add_filter(r"\d{4}-\d{2}-\d{2}", "[date]");
    s.add_filter(r#""version": "\d+\.\d+\.\d+""#, r#""version": "[semver]""#);
    s.add_filter(r"[0-9a-f]{32}", "[hash]");
    s
}

/// A store with a small, predictable project.
fn seeded() -> (DuckStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store = DuckStore::open(dir.path()).unwrap();
    let prov = Provenance::anonymous(Actor::Claude).with_session("ses_snapshot");

    let project = store
        .create(Project::new("harbour", "Harbour").into(), &prov)
        .unwrap()
        .entity
        .id()
        .clone();
    store
        .create(Spec::new(project.clone(), "Usage metering").into(), &prov)
        .unwrap();
    store
        .create(
            Task::new(
                project,
                "Dedupe usage events by idempotency key",
                "A row this test needs in the store.",
            )
            .into(),
            &prov,
        )
        .unwrap();
    (store, dir)
}

fn call(store: &mut DuckStore, name: &str, arguments: Value) -> Value {
    dispatch(
        store,
        ToolCall {
            name,
            arguments: &arguments,
        },
    )
    .unwrap_or_else(|e| json!({ "error": { "code": e.code, "message": e.message } }))
}

#[test]
fn tool_definitions() {
    // The single most important snapshot in the repo: these descriptions are
    // the only documentation an agent gets, and changing one changes how it
    // behaves.
    settings().bind(|| {
        insta::assert_json_snapshot!("tools_list", keel_mcp::list_result());
    });
}

#[test]
fn server_discovery() {
    settings().bind(|| {
        insta::assert_json_snapshot!("server_discover", keel_mcp::discover_result());
    });
}

#[test]
fn context_digest_shape() {
    let (mut store, _dir) = seeded();
    let result = call(&mut store, "keel_context", json!({"project": "harbour"}));
    settings().bind(|| {
        insta::assert_json_snapshot!("keel_context", result);
    });
}

#[test]
fn context_rollup_shape() {
    let (mut store, _dir) = seeded();
    let result = call(&mut store, "keel_context", json!({}));
    settings().bind(|| {
        insta::assert_json_snapshot!("keel_context_rollup", result);
    });
}

#[test]
fn create_response_shape() {
    let (mut store, _dir) = seeded();
    let result = call(
        &mut store,
        "keel_create",
        json!({
            "type": "decision",
            "project": "harbour",
            "title": "Aggregate hourly, not per-minute",
            "body": "## Decision\n\nHourly buckets.\n",
            "session_id": "ses_snapshot"
        }),
    );
    settings().bind(|| {
        insta::assert_json_snapshot!("keel_create", result);
    });
}

#[test]
fn search_response_shape() {
    let (mut store, _dir) = seeded();
    let result = call(&mut store, "keel_search", json!({"query": "idempotency"}));
    settings().bind(|| {
        insta::assert_json_snapshot!("keel_search", result);
    });
}

#[test]
fn projects_response_shape() {
    let (mut store, _dir) = seeded();
    let result = call(&mut store, "keel_projects", json!({}));
    settings().bind(|| {
        insta::assert_json_snapshot!("keel_projects", result);
    });
}

#[test]
fn error_shapes() {
    // Errors are read by a model that has to work out what to send instead, so
    // their wording is as much a contract as the success path.
    let (mut store, _dir) = seeded();
    let cases = json!({
        "unknown_field": call(
            &mut store, "keel_update",
            json!({"id": "tsk_01H8XK4RPVBQ2N7DZM9C3FGTWY", "version": 1,
                   "changes": {"asignee": "kb"}})
        ),
        "missing_argument": call(&mut store, "keel_search", json!({})),
        "unknown_project": call(
            &mut store, "keel_context", json!({"project": "does-not-exist"})
        ),
        "unknown_tool": call(&mut store, "keel_delete", json!({})),
        "bad_timestamp": call(
            &mut store, "keel_activity", json!({"since": "last tuesday"})
        ),
    });
    settings().bind(|| {
        insta::assert_json_snapshot!("errors", cases);
    });
}

#[test]
fn every_advertised_tool_is_dispatchable_and_vice_versa() {
    // The two lists are maintained by hand in different files: `tools::all()`
    // is what a client is told exists, and the `match` in `dispatch` is what
    // actually runs. Nothing tied them together, so a tool could be advertised
    // and unimplemented, or implemented and invisible — and the only symptom
    // would be a model calling something that answers "no tool named that".
    //
    // Dispatching every advertised name with empty arguments is enough to tell
    // the two apart: a *missing* tool answers METHOD_NOT_FOUND, while a present
    // one fails on its arguments, which is a different code.
    let (mut store, _dir) = seeded();

    for tool in keel_mcp::tools::all() {
        let result = dispatch(
            &mut store,
            ToolCall {
                name: tool.name,
                arguments: &json!({}),
            },
        );
        if let Err(e) = result {
            assert!(
                !e.message.contains("no tool named"),
                "`{}` is advertised by tools::all() but dispatch has no arm for it",
                tool.name
            );
        }
    }

    // And the other direction: a name dispatch does not know must not be
    // silently tolerated.
    let unknown = dispatch(
        &mut store,
        ToolCall {
            name: "keel_teleport",
            arguments: &json!({}),
        },
    )
    .unwrap_err();
    // A bad argument, not a missing method — so 400, not the 404 that means
    // "there is no MCP server at this address".
    assert_eq!(unknown.code, keel_mcp::protocol::codes::INVALID_PARAMS);
    assert_eq!(unknown.http_status(), 400);
    assert!(
        unknown.message.contains("keel_context"),
        "the error should list what does exist: {}",
        unknown.message
    );
}

#[test]
fn the_tool_count_is_what_the_documentation_claims() {
    // "Nine" was written in five places after the tenth tool landed, and "ten"
    // in as many after the thirteenth. A number in prose drifts; a number in an
    // assertion does not.
    assert_eq!(
        keel_mcp::tools::all().len(),
        13,
        "thirteen is the ceiling and the count — if this changes, every place \
         that states it has to change with it"
    );
}
