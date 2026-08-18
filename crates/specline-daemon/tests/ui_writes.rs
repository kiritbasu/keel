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

use serde_json::{Value, json};
use specline_daemon::{AppState, TOKEN_HEADER, http::router};

const TOKEN: &str = "a-known-token";

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

async fn patch(base: &str, path: &str, body: Value) -> (u16, Value) {
    let response = reqwest::Client::new()
        .patch(format!("{base}{path}"))
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

/// The fields a person moves while looking at a row, which is what hard
/// constraint 7 names as the interface's half.
#[tokio::test]
async fn the_fields_a_person_moves_can_be_moved_from_the_interface() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (status, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({
            "version": 1,
            "status": "review",
            "priority": "p0",
            "kind": "bug",
            "labels": ["desktop", "ux"]
        }),
    )
    .await;

    assert_eq!(status, 200, "changing a task: {body}");
    assert_eq!(body["data"]["status"], "review");
    assert_eq!(body["data"]["priority"], "p0");
    assert_eq!(body["data"]["kind"], "bug");
    assert_eq!(body["data"]["labels"], json!(["desktop", "ux"]));
    assert_eq!(
        body["data"]["audit"]["updated_by"], "human",
        "a person moved it, so the row must say so: {body}"
    );
    assert_eq!(body["data"]["audit"]["surface"], "ui");
}

/// Closing owes a reason, a message and evidence. A status field that could
/// reach `done` would be a way round the form that collects them, so it is
/// refused here and the refusal says where to go instead.
#[tokio::test]
async fn a_terminal_status_cannot_be_set_as_an_ordinary_field() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    for status in ["done", "wont_do"] {
        let (code, body) = patch(
            &base,
            &format!("/api/tasks/{id}"),
            json!({ "version": 1, "status": status }),
        )
        .await;
        assert_eq!(code, 400, "{status} must go through the close form: {body}");
        assert!(
            body["error"]["message"]
                .as_str()
                .unwrap_or_default()
                .contains("close"),
            "and the refusal has to say where to go instead: {body}"
        );
    }
}

/// Starting work is a claim, and a claim records which session is on it. A
/// person at the interface has no session, so the honest answer is to refuse
/// rather than leave the board showing work in flight against nobody.
#[tokio::test]
async fn starting_a_task_is_a_claim_and_the_interface_says_so() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": 1, "status": "in_progress" }),
    )
    .await;

    assert_eq!(code, 400, "in_progress must be refused: {body}");
    assert!(
        body["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("claim"),
        "and the refusal has to explain that a claim records who: {body}"
    );
}

/// A row moved out of `in_progress` must not keep the claim, or `specline_claim`
/// refuses it for three days in the name of a session that walked away.
#[tokio::test]
async fn moving_out_of_in_progress_releases_the_claim() {
    let (base, dir) = daemon().await;
    let id = a_task(&base).await;

    // Claimed the way it is really claimed — by a session, through core.
    {
        use specline_core::{EntityId, EntityStore};
        let mut store =
            specline_core::Store::open(specline_core::store_path(dir.path())).expect("reopen");
        let entity_id = EntityId::parse(&id).unwrap();
        let version = store.get(&entity_id).unwrap().unwrap().audit().version;
        store
            .update(
                &entity_id,
                version,
                json!({ "status": "in_progress", "claimed_by": "ses_someone", "claimed_at": "2026-08-18T10:00:00Z" })
                    .as_object()
                    .unwrap(),
                &specline_core::Provenance::anonymous(specline_core::Actor::Claude),
            )
            .unwrap();
    }

    let entity: Value = reqwest::get(format!("{base}/api/entity/{id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let row = &entity["data"]["artifacts"][0]["entity"];
    assert_eq!(row["claimed_by"], "ses_someone", "precondition: {entity}");

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": row["audit"]["version"], "status": "todo" }),
    )
    .await;

    assert_eq!(code, 200, "moving back to todo: {body}");
    assert_eq!(body["data"]["status"], "todo");
    assert!(
        body["data"]["claimed_by"].is_null(),
        "a todo row held by nobody must not still name a claimant: {body}"
    );
    assert!(body["data"]["claimed_at"].is_null());
}

/// Two people on the same row. The second write is told rather than silently
/// losing, and is given what it needs to decide what to do about it.
#[tokio::test]
async fn a_stale_version_is_a_conflict_with_the_current_state_attached() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (code, _) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": 1, "priority": "p0" }),
    )
    .await;
    assert_eq!(code, 200);

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": 1, "priority": "p3" }),
    )
    .await;

    assert_eq!(code, 409, "the second write is a conflict: {body}");
    assert_eq!(
        body["current_state"]["priority"], "p0",
        "and it carries what is actually there, so the caller can resolve it: {body}"
    );
    assert!(body["latest_version"].as_i64().unwrap_or_default() > 1);
}

/// `milestone` distinguishes "leave it alone" from "clear it", because
/// clearing a phase is a thing somebody means to do.
#[tokio::test]
async fn an_empty_milestone_clears_the_phase_rather_than_being_ignored() {
    let (base, dir) = daemon().await;
    let id = a_task(&base).await;

    let milestone = {
        use specline_core::EntityStore;
        let mut store =
            specline_core::Store::open(specline_core::store_path(dir.path())).expect("reopen");
        let project = store
            .list(
                &specline_core::EntityQuery::default().of_type(specline_core::EntityType::Project),
            )
            .unwrap()
            .items
            .into_iter()
            .next()
            .unwrap();
        let created = store
            .create(
                specline_core::Milestone::new(
                    project.id().clone(),
                    "Phase 1",
                    "The first one, so a task has a phase to be cleared from.",
                )
                .into(),
                &specline_core::Provenance::anonymous(specline_core::Actor::Claude),
            )
            .unwrap();
        created.entity.id().to_string()
    };

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": 1, "milestone": milestone }),
    )
    .await;
    assert_eq!(code, 200, "setting a phase: {body}");
    assert_eq!(body["data"]["milestone_id"], milestone);

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": body["data"]["audit"]["version"], "milestone": "" }),
    )
    .await;
    assert_eq!(code, 200, "clearing it: {body}");
    assert!(
        body["data"]["milestone_id"].is_null(),
        "an empty milestone means none, not 'leave it': {body}"
    );
}

/// A patch that changes nothing is a mistake worth naming rather than a
/// successful no-op — it is what a form with a broken field sends.
#[tokio::test]
async fn a_patch_with_no_fields_is_refused() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let (code, body) = patch(&base, &format!("/api/tasks/{id}"), json!({ "version": 1 })).await;
    assert_eq!(code, 400, "{body}");

    let (code, body) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "status": "review" }),
    )
    .await;
    assert_eq!(code, 400, "and a patch with no version is refused: {body}");
}

/// A field of the wrong shape is refused rather than skipped.
///
/// `and_then(as_str)` on its own turns `{"status": 5}` into a silent no-op, so
/// a caller whose form sent a number would be told the write succeeded while
/// one field of it quietly vanished — and would build on that.
#[tokio::test]
async fn a_field_of_the_wrong_type_is_refused_rather_than_dropped() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    for body in [
        json!({ "version": 1, "status": 5 }),
        json!({ "version": 1, "priority": ["p0"] }),
        json!({ "version": 1, "kind": true }),
        json!({ "version": 1, "milestone": 7 }),
        json!({ "version": 1, "labels": "desktop" }),
        json!({ "version": 1, "labels": ["desktop", 3] }),
    ] {
        let (code, answer) = patch(&base, &format!("/api/tasks/{id}"), body.clone()).await;
        assert_eq!(code, 400, "{body} must be refused: {answer}");
    }

    // And nothing landed while those were being refused — including the
    // priority sent alongside a bad status, which is the case that would
    // otherwise half-apply.
    let (code, answer) = patch(
        &base,
        &format!("/api/tasks/{id}"),
        json!({ "version": 1, "priority": "p0", "status": 5 }),
    )
    .await;
    assert_eq!(code, 400, "{answer}");

    let entity: Value = reqwest::get(format!("{base}/api/entity/{id}"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let row = &entity["data"]["artifacts"][0]["entity"];
    assert_eq!(
        row["priority"], "p2",
        "a refused patch must not have applied half of itself: {entity}"
    );
}

/// Every mutating endpoint sits behind the token, and a new one is exactly the
/// kind of thing that gets added outside the layer by accident.
#[tokio::test]
async fn changing_a_task_needs_the_token() {
    let (base, _dir) = daemon().await;
    let id = a_task(&base).await;

    let response = reqwest::Client::new()
        .patch(format!("{base}/api/tasks/{id}"))
        .json(&json!({ "version": 1, "priority": "p0" }))
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status().as_u16(),
        401,
        "a write with no token must be refused"
    );
}
