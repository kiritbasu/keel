//! The HTTP surface: the MCP endpoint and the local API.

use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event as SseEvent, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use keel_mcp::protocol::{
    HEADER_METHOD, HEADER_NAME, HEADER_PROTOCOL_VERSION, HeaderCheck, PROTOCOL_VERSION, Request,
    Response as RpcResponse, RpcError, check_headers, codes,
};
use serde_json::{Value, json};
use std::convert::Infallible;

/// Build the router.
pub fn router(state: AppState) -> Router {
    Router::new()
        // The single MCP endpoint. GET and DELETE are answered 405 rather than
        // 404: an older client may try the pre-2026-07-28 GET stream or the
        // session-terminating DELETE, and 405 tells it plainly that this
        // server does not do that, while 404 would suggest no endpoint here.
        .route(
            "/mcp",
            post(mcp_endpoint)
                .get(method_not_allowed)
                .delete(method_not_allowed),
        )
        .route("/api/health", get(health))
        .route("/api/context", get(api_context))
        .route("/api/projects", get(api_projects))
        .route("/api/search", get(api_search))
        .route("/api/activity", get(api_activity))
        .route("/api/entity/{id}", get(api_entity))
        .route("/api/events", get(api_events_stream))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

/// Serve a JSON-RPC response with the right status.
fn rpc(id: Value, result: Result<Value, RpcError>) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(RpcResponse::ok(id, value))).into_response(),
        Err(err) => {
            let status = StatusCode::from_u16(err.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            (status, Json(RpcResponse::err(id, err))).into_response()
        }
    }
}

/// GET or DELETE on the MCP endpoint.
async fn method_not_allowed() -> Response {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": codes::METHOD_NOT_FOUND,
                "message": format!(
                    "This endpoint speaks MCP {PROTOCOL_VERSION}, which uses POST only. The GET \
                     stream and the DELETE session teardown were removed with protocol-level \
                     sessions."
                )
            }
        })),
    )
        .into_response()
}

/// Whether an `Origin` header is acceptable.
///
/// The transport makes this a MUST, and the reason is specific: a local server
/// is reachable from any web page the user has open, so a DNS-rebinding attack
/// can drive it from a hostile origin. Only same-origin loopback is allowed —
/// a browser page on the public internet has no business here.
fn origin_ok(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|v| v.to_str().ok()) else {
        // Absent is fine: a non-browser client (which is every MCP client)
        // does not send one.
        return true;
    };
    let normalised = origin.trim().to_ascii_lowercase();
    if normalised == "null" {
        return true;
    }

    // The host is compared exactly, never by prefix. `starts_with("https://
    // localhost")` accepts `https://localhost.evil.example`, which is a
    // domain an attacker can simply register — the check would then wave
    // through precisely the request it exists to stop. Caught by the test
    // below, which is why that test names the near-miss explicitly.
    let Some((scheme, rest)) = normalised.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "http" | "https" | "tauri") {
        return false;
    }
    // An Origin has no path, but strip one defensively rather than trusting
    // that.
    let authority = rest.split('/').next().unwrap_or("");
    let host = match authority.rsplit_once(':') {
        // Only treat the tail as a port when it actually is one, so an IPv6
        // literal is not truncated at its last colon.
        Some((h, port)) if port.chars().all(|c| c.is_ascii_digit()) && !port.is_empty() => h,
        _ => authority,
    };

    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

/// The MCP endpoint.
async fn mcp_endpoint(State(state): State<AppState>, headers: HeaderMap, body: String) -> Response {
    if !origin_ok(&headers) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": codes::INVALID_REQUEST,
                    "message": "rejected by Origin check: this daemon serves local clients only"
                }
            })),
        )
            .into_response();
    }

    let request: Request = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return rpc(
                Value::Null,
                Err(RpcError::new(
                    codes::PARSE_ERROR,
                    format!("could not parse the request body as JSON-RPC: {e}"),
                )),
            );
        }
    };

    if request.jsonrpc != "2.0" {
        return rpc(
            request.id.clone().unwrap_or(Value::Null),
            Err(RpcError::new(
                codes::INVALID_REQUEST,
                format!("`jsonrpc` must be \"2.0\", got \"{}\"", request.jsonrpc),
            )),
        );
    }

    let header_of = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    if let HeaderCheck::Reject(err) = check_headers(
        &request,
        header_of(HEADER_METHOD),
        header_of(HEADER_NAME),
        header_of(HEADER_PROTOCOL_VERSION),
    ) {
        return rpc(request.id.clone().unwrap_or(Value::Null), Err(err));
    }

    // A notification gets 202 and no body. This revision defines no
    // client-to-server notifications, so reaching here means a non-conforming
    // client — but answering correctly costs one branch.
    if request.is_notification() {
        return StatusCode::ACCEPTED.into_response();
    }

    let id = request.id.clone().unwrap_or(Value::Null);
    if let Some(info) = request.client_info() {
        tracing::debug!(method = %request.method, client = %info, "mcp request");
    }

    let result = match request.method.as_str() {
        "server/discover" => Ok(keel_mcp::discover_result()),
        "tools/list" => Ok(keel_mcp::list_result()),
        "tools/call" => {
            let Some(name) = request.tool_name() else {
                return rpc(
                    id,
                    Err(RpcError::new(
                        codes::INVALID_PARAMS,
                        "`params.name` is required for tools/call",
                    )),
                );
            };
            let mut store = state.store();
            let before = latest_event(&store);
            let outcome = keel_mcp::dispatch(
                &mut store,
                keel_mcp::ToolCall {
                    name,
                    arguments: request.arguments(),
                },
            );
            // Announce after the lock is released, so a slow subscriber can
            // never hold the write handle.
            let after = latest_event(&store);
            drop(store);
            if let (Some(after_id), true) = (after.clone(), before != after) {
                state.announce(after_id, format!("{name} completed"));
            }
            outcome
        }
        other => Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this server implements server/discover, tools/list and tools/call. \
                 `{other}` is not one of them."
            ),
        )),
    };

    rpc(id, result)
}

/// The newest event id, used to detect that a call changed something.
fn latest_event(store: &keel_core::DuckStore) -> Option<keel_core::EventId> {
    use keel_core::EntityStore;
    store
        .events(&keel_core::Cursor::Beginning, None, 100_000)
        .ok()
        .and_then(|p| p.items.last().map(|e| e.id.clone()))
}

// --- The local API -------------------------------------------------------
//
// Keel's own surface, not MCP. Identical in shape to what a remote daemon
// would serve, so the desktop app and any future web build are one bundle
// with a different base URL.

async fn health(State(state): State<AppState>) -> Json<Value> {
    let store = state.store();
    let projects = {
        use keel_core::{EntityQuery, EntityStore, EntityType};
        store
            .list(&EntityQuery::default().of_type(EntityType::Project))
            .map(|p| p.total)
            .unwrap_or(0)
    };
    Json(json!({
        "status": "ok",
        "protocol": PROTOCOL_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        "projects": projects,
    }))
}

/// Turn a tool call into an HTTP response, for the REST surface.
fn as_api(result: Result<Value, RpcError>) -> Response {
    match result {
        // The REST surface wants the data, not the MCP content envelope.
        Ok(value) => {
            let structured = value
                .get("structuredContent")
                .cloned()
                .unwrap_or(value.clone());
            let text = value
                .get("content")
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("text"))
                .cloned()
                .unwrap_or(Value::Null);
            (
                StatusCode::OK,
                Json(json!({ "data": structured, "summary": text })),
            )
                .into_response()
        }
        Err(err) => {
            let status = StatusCode::from_u16(err.http_status())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            (status, Json(json!({ "error": err }))).into_response()
        }
    }
}

/// Query parameters are passed through to the tool layer verbatim, so the REST
/// surface and the MCP surface can never drift apart in what they accept.
fn params_to_json(params: std::collections::HashMap<String, String>) -> Value {
    let mut out = serde_json::Map::new();
    for (k, v) in params {
        // Numbers and booleans arrive as strings over a query string; the tool
        // layer wants them typed.
        let value = if let Ok(n) = v.parse::<i64>() {
            json!(n)
        } else if v == "true" || v == "false" {
            json!(v == "true")
        } else {
            json!(v)
        };
        out.insert(k, value);
    }
    Value::Object(out)
}

async fn api_context(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json(params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_context",
            arguments: &args,
        },
    ))
}

async fn api_projects(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json(params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_projects",
            arguments: &args,
        },
    ))
}

async fn api_search(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json(params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_search",
            arguments: &args,
        },
    ))
}

async fn api_activity(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json(params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_activity",
            arguments: &args,
        },
    ))
}

async fn api_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let mut args = params_to_json(params);
    if let Some(obj) = args.as_object_mut() {
        obj.insert("ids".to_owned(), json!([id]));
    }
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_get",
            arguments: &args,
        },
    ))
}

/// Live change notifications for the desktop app.
async fn api_events_stream(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.changes.subscribe();
    let stream = async_stream::stream! {
        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(change) => {
                    let data = serde_json::to_string(&change).unwrap_or_else(|_| "{}".to_owned());
                    yield Ok(SseEvent::default().event("change").data(data));
                }
                // Lagged means this subscriber fell behind and lost messages.
                // Say so rather than pretending: a UI that missed changes
                // should refetch, and silently continuing would leave it
                // showing stale state indefinitely.
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    yield Ok(SseEvent::default()
                        .event("lagged")
                        .data(json!({ "missed": n }).to_string()));
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };
    Sse::new(stream).keep_alive(axum::response::sse::KeepAlive::default())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn headers_with(origin: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::ORIGIN, origin.parse().unwrap());
        h
    }

    #[test]
    fn loopback_origins_are_allowed() {
        for ok in [
            "http://localhost:1420",
            "http://127.0.0.1:7654",
            "https://localhost",
            "tauri://localhost",
            "HTTP://LOCALHOST:1420",
            "null",
        ] {
            assert!(origin_ok(&headers_with(ok)), "{ok} should be allowed");
        }
    }

    #[test]
    fn a_remote_origin_is_rejected() {
        // The DNS-rebinding case the transport requires this check for.
        for bad in [
            "https://evil.example",
            "http://keel.attacker.test",
            // The near-misses. Each of these defeats a prefix or substring
            // check, and each is a hostname an attacker can simply register.
            "https://localhost.evil.example",
            "http://127.0.0.1.evil.example",
            "https://notlocalhost",
            "http://evil.example/localhost",
            "https://evil.example#http://localhost",
            "file:///etc/passwd",
            "localhost",
        ] {
            assert!(!origin_ok(&headers_with(bad)), "{bad} should be rejected");
        }
    }

    #[test]
    fn ipv6_loopback_is_allowed_and_not_truncated_at_its_colons() {
        assert!(origin_ok(&headers_with("http://[::1]:7654")));
        assert!(origin_ok(&headers_with("http://[::1]")));
    }

    #[test]
    fn an_absent_origin_is_allowed() {
        // Every MCP client is a non-browser client and sends none.
        assert!(origin_ok(&HeaderMap::new()));
    }

    #[test]
    fn query_parameters_are_typed_on_the_way_in() {
        let mut params = std::collections::HashMap::new();
        params.insert("limit".to_owned(), "25".to_owned());
        params.insert("include_archived".to_owned(), "true".to_owned());
        params.insert("query".to_owned(), "onboarding".to_owned());

        let json = params_to_json(params);
        assert_eq!(json["limit"], 25);
        assert_eq!(json["include_archived"], true);
        assert_eq!(json["query"], "onboarding");
    }
}
