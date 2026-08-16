//! What a person does in the interface.
//!
//! Hard constraint 7, as rewritten by B-78: the interface writes what a person
//! *does* — create a task, comment on one, archive a row, close one — and never
//! what a person *reasons*. These tests are about the three properties that
//! make that safe rather than merely possible:
//!
//! - the write is attributed to a **human on the `ui` surface**, so a month
//!   later "who changed this" has an answer that is not a guess;
//! - **archive means archive**, because hard constraint 3 says nothing is ever
//!   removed and an interface is where somebody expects Delete to mean gone;
//! - **closing still needs a reason, a message and evidence**, because that
//!   check lives in the storage layer precisely so no surface is the easy way
//!   round it.
//!
//! Every one of these goes through the token, since every one of them mutates.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use keel_daemon::{AppState, TOKEN_HEADER, http::router};
use serde_json::{Value, json};

const TOKEN: &str = "a-known-token";

async fn daemon() -> (String, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut store =
        keel_core::Store::open(keel_core::store_path(dir.path())).expect("open the store");
    {
        use keel_core::EntityStore;
        store
            .create(
                keel_core::Project::new("harbour", "Harbour").into(),
                &keel_core::Provenance::anonymous(keel_core::Actor::Claude),
            )
            .unwrap();
    }
    let state = AppState::from_store_with_token(store, TOKEN);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = router(state);
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (format!("http://{addr}"), dir)
}

async fn post(base: &str, path: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .post(format!("{base}{path}"))
        .header(TOKEN_HEADER, TOKEN)
        .json(&body)
        .send()
        .await
        .unwrap();
    let status = response.status().as_u16();
    let parsed = response.json::<Value>().await.unwrap_or(Value::Null);
    (status, parsed)
}

async fn a_task(base: &str) -> String {
    let (status, body) = post(
        base,
        "/api/tasks",
        json!({
            "project": "harbour",
            "title": "Something a person typed",
            "summary": "Created from the interface, which is the point."
        }),
    )
    .await;
    assert_eq!(status, 200, "creating a task: {body}");
    body["data"]["id"].as_str().unwrap().to_owned()
}

#[tokio::test]
async fn a_task_created_in_the_interface_is_attributed_to_a_person() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let entity: Value = reqwest::get(format!("{base}/api/entity/{id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let audit = &entity["data"]["artifacts"][0]["entity"]["audit"];

    assert_eq!(
        audit["created_by"], "human",
        "a person clicked it, so the row must say so: {audit}"
    );
    assert_eq!(audit["surface"], "ui");
    assert!(
        audit["session_id"].is_null(),
        "there is no conversation behind a button, and the daemon never invents \
         a session id (hard constraint 5): {audit}"
    );
}

#[tokio::test]
async fn a_comment_from_the_interface_is_a_note_by_a_human() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, body) = post(
        &base,
        &format!("/api/entity/{id}/notes"),
        json!({ "body": "Tried it on the train; the second screen never loads." }),
    )
    .await;

    assert_eq!(status, 200, "adding a note: {body}");
    assert_eq!(body["data"]["author"], "human");
    assert!(
        body["data"]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("second screen")
    );
}

/// An empty comment is a click that recorded nothing. Refusing is kinder than
/// storing a blank note somebody has to work out how to remove.
#[tokio::test]
async fn an_empty_comment_is_refused() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, _) = post(
        &base,
        &format!("/api/entity/{id}/notes"),
        json!({ "body": "   " }),
    )
    .await;

    assert_eq!(status, 400);
}

/// The one somebody will get wrong from the interface, because Delete is the
/// word a person means and hard constraint 3 says nothing is ever removed.
#[tokio::test]
async fn archiving_hides_the_row_and_keeps_it() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, body) = post(&base, &format!("/api/entity/{id}/archive"), json!({})).await;
    assert_eq!(status, 200, "archiving: {body}");

    let entity: Value = reqwest::get(format!("{base}/api/entity/{id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let row = &entity["data"]["artifacts"][0]["entity"];
    assert!(
        !row["id"].is_null(),
        "an archived row is still readable — that is the whole difference \
         between archiving and deleting: {entity}"
    );
    assert!(
        !row["audit"]["archived_at"].is_null(),
        "and it must be marked as archived: {entity}"
    );
}

/// The storage layer refuses a close with no reason behind it, and the
/// interface does not get an easier version of that rule. The form has to ask.
#[tokio::test]
async fn closing_without_a_reason_is_refused_from_the_interface_too() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, body) = post(&base, &format!("/api/tasks/{id}/close"), json!({})).await;
    assert_eq!(
        status, 400,
        "a close with no reason must be refused: {body}"
    );

    // `done` with a message and no evidence is the other half of the same rule.
    let (status, body) = post(
        &base,
        &format!("/api/tasks/{id}/close"),
        json!({ "reason": "done", "message": "Finished it." }),
    )
    .await;
    assert_eq!(
        status, 400,
        "done without evidence must be refused as well: {body}"
    );
}

#[tokio::test]
async fn a_close_with_everything_it_needs_is_accepted() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, body) = post(
        &base,
        &format!("/api/tasks/{id}/close"),
        json!({
            "reason": "done",
            "message": "Shipped and checked against the published build.",
            "evidence": ["commit:abc1234"]
        }),
    )
    .await;

    assert_eq!(status, 200, "closing: {body}");
    assert_eq!(body["data"]["status"], "done");
    assert_eq!(
        body["data"]["audit"]["updated_by"], "human",
        "and the close is attributed to the person who did it"
    );
}

/// The line B-78 draws, as a test. There is no endpoint here that takes a
/// document body, and the absence is the constraint rather than an oversight —
/// so if one is ever added, this is what should have to be deleted first.
#[tokio::test]
async fn the_interface_cannot_write_prose() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    for path in [
        format!("/api/entity/{id}/document"),
        format!("/api/entity/{id}/body"),
        "/api/documents".to_owned(),
    ] {
        let (status, _) = post(&base, &path, json!({ "body": "# A spec somebody typed" })).await;
        assert_eq!(
            status, 404,
            "{path} answered, which means the interface has grown a way to author \
             prose — that needs a decision, not an endpoint"
        );
    }
}
