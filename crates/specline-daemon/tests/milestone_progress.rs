//! What `/api/entities?type=milestone` says about how far a phase has got.
//!
//! The roadmap's right-hand column used to render `target_date`, falling back
//! to the words "no target" — and nothing on any create path sets a target, so
//! it said that for most rows and said the seed date for the rest. It now says
//! how many of the phase's tasks are closed, which means the counts have to
//! come down this endpoint: the browser holds only the milestones, so counting
//! in the app would mean fetching every task in the project (KEEL-332).
//!
//! The all-projects case is here because it is the one that was wrong first.
//! Deriving progress only for `query.project_id` left every row in the
//! all-projects roadmap with no counts, and the screen would have rendered that
//! as "not scoped" — a claim about the phase rather than an admission about the
//! reply.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::Value;
use specline_daemon::{AppState, router};

struct Daemon {
    base: String,
    client: reqwest::Client,
    _dir: tempfile::TempDir,
    _handle: tokio::task::JoinHandle<()>,
}

impl Daemon {
    async fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut store = specline_core::Store::open(dir.path().join("specline.sqlite"))
                .expect("open the store");
            specline_core::fixture::load(&mut store).expect("load the fixture");
        }
        let state = AppState::open(dir.path(), false).expect("open the store");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            let _ = axum::serve(listener, router(state)).await;
        });
        Daemon {
            base: format!("http://{addr}"),
            client: reqwest::Client::new(),
            _dir: dir,
            _handle: handle,
        }
    }

    async fn milestones(&self, query: &str) -> Vec<Value> {
        let response = self
            .client
            .get(format!("{}/api/entities?type=milestone&{query}", self.base))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status().as_u16(), 200);
        let body: Value = response.json().await.unwrap();
        body["data"]["items"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    }
}

#[tokio::test]
async fn a_project_scoped_list_carries_the_task_counts() {
    let daemon = Daemon::start().await;
    let items = daemon.milestones("project=harbour&limit=100").await;
    assert!(!items.is_empty(), "the fixture has milestones");

    for m in &items {
        assert!(
            m.get("tasks_total").and_then(Value::as_u64).is_some(),
            "every milestone should carry a total: {m}"
        );
        assert!(
            m.get("tasks_closed").and_then(Value::as_u64).is_some(),
            "every milestone should carry a closed count: {m}"
        );
        assert!(m.get("state").is_some(), "and the derived state: {m}");
    }

    // The counts have to be consistent with each other, or the bar the roadmap
    // draws from them can exceed its own track.
    for m in &items {
        let total = m["tasks_total"].as_u64().unwrap();
        let closed = m["tasks_closed"].as_u64().unwrap();
        assert!(closed <= total, "closed cannot exceed total: {m}");
    }
}

#[tokio::test]
async fn the_all_projects_list_carries_them_too() {
    let daemon = Daemon::start().await;
    let items = daemon.milestones("limit=200").await;
    assert!(items.len() > 1, "the fixture has more than one project");

    // Every row, not just the ones from whichever project happened to be first.
    // Absent counts are what the screen renders as nothing at all, so a gap
    // here is a roadmap that silently stops saying how far anything has got.
    let missing: Vec<&Value> = items
        .iter()
        .filter(|m| m.get("tasks_total").and_then(Value::as_u64).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "{} of {} milestones came back with no counts: {:?}",
        missing.len(),
        items.len(),
        missing.first()
    );

    let projects: std::collections::HashSet<&str> = items
        .iter()
        .filter_map(|m| m["project_id"].as_str())
        .collect();
    assert!(
        projects.len() > 1,
        "this test only means something across several projects"
    );
}

#[tokio::test]
async fn a_milestone_that_has_never_moved_reports_no_activity_rather_than_a_date() {
    let daemon = Daemon::start().await;
    let items = daemon.milestones("limit=200").await;

    // `last_activity` is nullable on purpose. A release row carries no tasks,
    // so the honest answer is nothing — and the epoch, which is what a
    // defaulted timestamp would give, reads on the roadmap as "last touched in
    // 1970".
    for m in &items {
        match m.get("last_activity") {
            Some(Value::Null) | None => {}
            Some(Value::String(at)) => {
                assert!(
                    chrono::DateTime::parse_from_rfc3339(at).is_ok(),
                    "last_activity should be RFC3339: {at}"
                );
                assert!(!at.starts_with("1970"), "a defaulted timestamp leaked: {m}");
            }
            other => panic!("last_activity should be a string or null, got {other:?}"),
        }
    }
}
