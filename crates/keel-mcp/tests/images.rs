//! Inline images: the ingestion path for design artifacts (TQ-6, KEEL-46).
//!
//! Base64 in the tool call is the only path that works from every surface. A
//! filesystem path works only where there is a filesystem, which excludes chat
//! and Cowork — the two places a design image actually comes from.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use base64::Engine as _;
use keel_core::{Actor, DocumentStore, DuckStore, EntityStore, Project, Provenance};
use keel_mcp::{ToolCall, dispatch};
use serde_json::{Value, json};

/// The smallest real PNG: 1×1, transparent. Magic bytes are what matter here.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn store() -> (DuckStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let mut s = DuckStore::open(dir.path()).unwrap();
    s.create(
        Project::new("harbour", "Harbour").into(),
        &Provenance::anonymous(Actor::Claude),
    )
    .unwrap();
    (s, dir)
}

fn call(store: &mut DuckStore, args: Value) -> Result<Value, keel_mcp::protocol::RpcError> {
    dispatch(
        store,
        ToolCall {
            name: "keel_create",
            arguments: &args,
        },
    )
}

#[test]
fn a_base64_image_is_stored_and_the_design_points_at_it() {
    let (mut store, _d) = store();
    let result = call(
        &mut store,
        json!({
            "type": "design", "project": "harbour", "name": "Invoice screen",
            "image": b64(PNG), "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect("a design with an inline image");

    let blob_id = result["structuredContent"]["entity"]["blob_id"]
        .as_str()
        .expect("the design must point at the blob it was given");

    let blob = store
        .get_blob(&keel_core::BlobId::parse(blob_id).unwrap())
        .unwrap()
        .expect("the blob is stored");
    assert_eq!(blob.bytes, PNG, "byte-for-byte, not re-encoded");

    // Sniffed from the magic bytes, not taken on trust.
    assert_eq!(blob.media_type, "image/png");

    // Owned, so `fsck` can trace it. A blob with no entity is bytes nobody
    // dares delete.
    assert!(blob.entity_id.is_some(), "the blob must name its owner");
    assert!(blob.project_id.is_some());
}

#[test]
fn a_data_url_is_accepted_because_a_model_will_produce_one() {
    let (mut store, _d) = store();
    let result = call(
        &mut store,
        json!({
            "type": "design", "project": "harbour", "name": "From a data URL",
            "image": format!("data:image/png;base64,{}", b64(PNG)),
            "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect("a data: URL is a reasonable thing to be handed");
    assert!(result["structuredContent"]["entity"]["blob_id"].is_string());
}

#[test]
fn wrapped_base64_still_decodes() {
    // A model breaking a long payload across lines has valid intent and
    // invalid base64. Failing on it would be a papercut with no upside.
    let (mut store, _d) = store();
    let wrapped = b64(PNG)
        .as_bytes()
        .chunks(20)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join("\n");
    let result = call(
        &mut store,
        json!({
            "type": "design", "project": "harbour", "name": "Wrapped",
            "image": wrapped, "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect("whitespace is stripped before decoding");
    assert!(result["structuredContent"]["entity"]["blob_id"].is_string());
}

#[test]
fn an_oversized_image_is_refused_with_its_size_and_nothing_is_created() {
    let (mut store, _d) = store();
    let huge = vec![0x89u8; 1_048_577];
    let err = call(
        &mut store,
        json!({
            "type": "design", "project": "harbour", "name": "Too big",
            "image": b64(&huge), "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect_err("over the cap");

    assert!(
        err.message.contains("1048577"),
        "name the actual size: {}",
        err.message
    );
    assert!(
        err.message.contains("1048576"),
        "and the limit: {}",
        err.message
    );

    // The check runs before anything is written, so a refused image leaves no
    // half-made design behind. Truncating instead would be worse still: a
    // corrupt file that looks like a successful write.
    let designs = store
        .list(&keel_core::EntityQuery::default().of_type(keel_core::EntityType::Design))
        .unwrap();
    assert!(designs.items.is_empty(), "nothing may be created");
}

#[test]
fn undecodable_base64_says_so_rather_than_storing_rubbish() {
    let (mut store, _d) = store();
    let err = call(
        &mut store,
        json!({
            "type": "design", "project": "harbour", "name": "Nonsense",
            "image": "this is not base64 !!!", "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect_err("must not be stored as bytes");
    assert!(err.message.contains("base64"), "{}", err.message);
}

#[test]
fn a_type_that_holds_no_image_says_which_ones_do() {
    let (mut store, _d) = store();
    let err = call(
        &mut store,
        json!({
            "type": "task", "project": "harbour", "title": "Not an image holder",
            "image": b64(PNG), "session_id": "ses_t", "surface": "chat"
        }),
    )
    .expect_err("a task has nowhere to put it");
    assert!(
        err.message.contains("design"),
        "point at the right type: {}",
        err.message
    );
}
