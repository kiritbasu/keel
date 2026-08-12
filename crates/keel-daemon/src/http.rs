//! The HTTP surface: the MCP endpoint and the local API.

use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::sse::{Event as SseEvent, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use keel_mcp::protocol::{
    Era, HEADER_METHOD, HEADER_NAME, HEADER_PROTOCOL_VERSION, HeaderCheck, PROTOCOL_VERSION,
    Request, Response as RpcResponse, RpcError, check_headers, codes, initialize_result,
};
use serde_json::{Value, json};
use std::convert::Infallible;

/// The largest request body the daemon will read.
///
/// Generous, because `keel_create` carries an inline image and the tool
/// documents a 1 MB decoded ceiling — base64 inflates that by a third, and a
/// limit that refuses a legitimate screenshot would be discovered by a user
/// rather than by a test. Small enough that a runaway client cannot make the
/// daemon hold hundreds of megabytes it will never use.
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;

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
        .route("/api/ready", get(api_ready))
        .route("/api/activity", get(api_activity))
        .route("/api/changes", get(api_changes))
        .route("/api/entity/{id}", get(api_entity))
        .route("/api/entity/{id}/history", get(api_entity_history))
        .route("/api/entities", get(api_entities))
        .route("/api/notes", get(api_notes))
        .route("/api/document/{id}", get(api_document))
        .route("/api/graph/{id}", get(api_graph))
        .route("/api/events", get(api_events_stream))
        // Generation writes files into the user's repository, so it is a POST:
        // it is not a safe, cacheable read even though it only reads the store.
        .route("/api/generate", post(api_generate))
        // Read-shaped CLI commands, served here because they cannot open the
        // store themselves while this process holds the write lock — which is
        // always (TQ-15, KEEL-57). `fsck` is the one that matters: an integrity
        // check you have to stop the thing you want to check in order to run is
        // not much of a check.
        .route("/api/blob/{id}", get(api_blob))
        .route("/api/fsck", get(api_fsck))
        .route("/api/lint", get(api_lint))
        .route("/api/status", get(api_status))
        .route("/api/render-status", get(api_render_status))
        // The Tauri webview is served from `tauri://localhost`, so every call
        // to the daemon is cross-origin and needs CORS. Scoped to the local
        // API: the MCP endpoint is not called from a browser, and giving it
        // CORS headers would only widen what a hostile page can reach.
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::predicate(|origin, _| {
                    origin
                        .to_str()
                        .is_ok_and(|o| is_local_origin(&o.to_ascii_lowercase()))
                }))
                // POST as well as GET. `/api/generate` is a POST and was
                // unreachable from the desktop app for as long as this said
                // GET only — the one endpoint the app needs to *do* anything
                // rather than read.
                .allow_methods([axum::http::Method::GET, axum::http::Method::POST])
                .allow_headers(tower_http::cors::Any),
        )
        .layer(tower_http::trace::TraceLayer::new_for_http())
        // A cap on how much the daemon will read, and a handler that explains
        // it in the shape the caller is speaking.
        //
        // Axum's own answer to an oversized body is a bare 413 with no body at
        // all. An MCP client parses that as a broken server rather than as a
        // request it should make smaller — so the one error a caller could
        // actually act on was the one that arrived unreadable.
        .layer(axum::middleware::from_fn(explain_body_limit))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .with_state(state)
}

/// Turn a bare 413 into a JSON-RPC error that names the limit.
///
/// A middleware rather than a change at each handler, because the rejection
/// happens in the extractor — before any handler runs — so there is nowhere
/// else to catch it.
async fn explain_body_limit(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let response = next.run(request).await;
    if response.status() != StatusCode::PAYLOAD_TOO_LARGE {
        return response;
    }

    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(json!({
            "jsonrpc": "2.0",
            "error": {
                "code": codes::INVALID_REQUEST,
                "message": format!(
                    "the request body is larger than {} bytes, which is this daemon's limit. \
                     An inline image is the usual cause: pass `image_path` instead, so the \
                     daemon reads the file itself and the bytes never travel as base64.",
                    MAX_BODY_BYTES
                )
            }
        })),
    )
        .into_response()
}

/// Serve a JSON-RPC response with the right status.
fn rpc(id: Value, result: Result<Value, RpcError>, era: Era) -> Response {
    match result {
        Ok(value) => (StatusCode::OK, Json(RpcResponse::ok(id, value, era))).into_response(),
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
                    "This endpoint uses POST only. {PROTOCOL_VERSION} removed the GET stream \
                     and the DELETE session teardown along with protocol-level sessions; a \
                     2025-11-25 client should treat this as a server with no server-initiated \
                     messages and carry on."
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
    // `null` used to be allowed, and it is the one value that must not be. A
    // browser sends `Origin: null` from a sandboxed iframe, a `file://` page
    // and a redirected cross-origin request — which is to say, from exactly
    // the contexts an attacker can arrange and a local client never uses. It
    // was the widest hole in the check, wearing the costume of an edge case.
    is_local_origin(&normalised)
}

/// Whether a lowercased origin string names this machine.
fn is_local_origin(normalised: &str) -> bool {
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
    // First, before anything is spent on this request.
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

    // After the Origin check, and that order is the point. The limiter used to
    // run first, so any web page the user had open could spend the whole
    // budget on requests that were going to be refused anyway — a denial of
    // service that costs the attacker one fetch and the user their next tool
    // call. Checking who is asking before charging them is free.
    if let Err(retry_after) = state.rate_limit.check() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", retry_after.as_secs().to_string())],
            Json(json!({
                "jsonrpc": "2.0",
                "error": {
                    "code": codes::INVALID_REQUEST,
                    "message": format!(
                        "rate limited: too many calls in a short window. Retry in {}s. \
                         If you are retrying a failing call, read the error rather than \
                         sending it again — the same call will fail the same way.",
                        retry_after.as_secs()
                    )
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
                Era::Modern,
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
            Era::Modern,
        );
    }

    let header_of = |name: &str| headers.get(name).and_then(|v| v.to_str().ok());
    let era = match check_headers(
        &request,
        header_of(HEADER_METHOD),
        header_of(HEADER_NAME),
        header_of(HEADER_PROTOCOL_VERSION),
    ) {
        HeaderCheck::Ok(era) => era,
        HeaderCheck::Reject(err) => {
            return rpc(
                request.id.clone().unwrap_or(Value::Null),
                Err(err),
                Era::Modern,
            );
        }
    };

    // Notifications get 202 and no body. The current revision defines none
    // client-to-server, but 2025-11-25's `notifications/initialized` arrives
    // here and must be accepted rather than 404'd — a client that gets an
    // error for it treats the connection as failed.
    if request.is_notification() {
        return StatusCode::ACCEPTED.into_response();
    }

    let id = request.id.clone().unwrap_or(Value::Null);
    if let Some(info) = request.client_info() {
        tracing::debug!(method = %request.method, client = %info, "mcp request");
    }

    let result = match request.method.as_str() {
        // The 2025-11-25 handshake. Removed in the current revision, but this
        // is what Claude Code actually opens with.
        "initialize" => Ok(initialize_result(era)),
        // Also 2025-11-25. Cheap to answer and its absence looks like a dead
        // connection to a client that uses it as a keep-alive.
        "ping" => Ok(json!({})),
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
                    era,
                );
            };
            // Embedding the query is model inference — the one expensive thing
            // on a read path, and the last thing that should happen while every
            // other request waits on the store. Done here, before the lock, so
            // the critical section is two SQL queries.
            let query_vector = state.embed_query(name, request.arguments());

            let mut store = state.store();
            let before = latest_event(&store);
            let outcome = keel_mcp::dispatch_prepared(
                &mut store,
                keel_mcp::ToolCall {
                    name,
                    arguments: request.arguments(),
                },
                query_vector,
            );
            // Announce after the lock is released, so a slow subscriber can
            // never hold the write handle.
            let after = latest_event(&store);
            drop(store);
            if let (Some(after_id), true) = (after.clone(), before != after) {
                state.announce(after_id, format!("{name} completed"));
            } else if name == "keel_note"
                && let Ok(value) = &outcome
                && !value
                    .get("isError")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            {
                // A note writes no event row, so the check above cannot see it
                // and an open app kept showing a stale note stream with nothing
                // to say it was stale (TQ-29). Announced under its own kind so
                // a client can tell the two apart.
                let entity_id = value
                    .pointer("/structuredContent/note/entity_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                state.announce_note(entity_id, "keel_note completed");
            }
            outcome
        }
        other => Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "this server implements initialize, ping, server/discover, tools/list \
                 and tools/call. `{other}` is not one of them."
            ),
        )),
    };

    rpc(id, result, era)
}

/// The newest event id, used to detect that a call changed something.
fn latest_event(store: &keel_core::Store) -> Option<keel_core::EventId> {
    use keel_core::EntityStore;
    store.latest_event_id().ok().flatten()
}

// --- The local API -------------------------------------------------------
//
// Keel's own surface, not MCP. Identical in shape to what a remote daemon
// would serve, so the desktop app and any future web build are one bundle
// with a different base URL.

/// Liveness, and it must never block.
///
/// This is the probe the CLI uses to decide whether a daemon owns the store, so
/// it is asked at exactly the moment a slow write is in progress — and it used
/// to take the store lock, which meant the question could not be answered
/// precisely when it mattered. A `keel generate` holding the lock for thirty
/// seconds made health hang for thirty seconds, the CLI concluded the daemon
/// was unreachable, and it opened the store itself. The probe caused the second
/// writer it existed to prevent.
///
/// So: `try_store`, never `store`. When the lock is held the last known project
/// count is reported and `store_busy` says the number may be stale. A stale
/// count on a health page costs nothing; a health page that hangs costs the
/// constraint.
async fn health(State(state): State<AppState>) -> Json<Value> {
    let (projects, busy) = match state.try_store() {
        Some(store) => {
            use keel_core::{EntityQuery, EntityStore, EntityType};
            let n = store
                .list(&EntityQuery::default().of_type(EntityType::Project))
                .map(|p| p.total)
                .unwrap_or(0);
            drop(store);
            state.remember_project_count(n);
            (n, false)
        }
        None => (state.last_project_count().unwrap_or(0), true),
    };

    Json(json!({
        "status": "ok",
        "protocol": PROTOCOL_VERSION,
        "version": env!("CARGO_PKG_VERSION"),
        // The number another process should compare itself against. `version`
        // moves for reasons that have nothing to do with the tables — a CLI
        // one patch release ahead of the daemon is fine, and a CLI one
        // migration ahead is not. Reported from what this binary ships rather
        // than from the store, because it answers "what does the process
        // holding the store believe the tables look like", and after startup
        // those are the same number.
        "schema": keel_core::shipped_schema_version(),
        "projects": projects,
        "store_busy": busy,
    }))
}

/// Turn a tool call into an HTTP response, for the REST surface.
/// Regenerate a project's repository files from Keel.
///
/// Lives here rather than in the CLI because D-5 says non-daemon processes go
/// through this API. Generation reads the whole store and writes files from it,
/// so it wants the state the single writer has actually committed — and the
/// daemon is the only thing that can answer for that.
async fn api_generate(State(state): State<AppState>, Json(body): Json<Value>) -> Response {
    use keel_core::{Entity, EntityQuery, EntityStore, EntityType, Mode, generate};

    let reference = body
        .get("project")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    if reference.is_empty() {
        return bad_request("`project` is required — pass a project id, slug or name");
    }
    let check = body.get("check").and_then(Value::as_bool).unwrap_or(false);

    let store = state.store();

    let projects = match store.list(&EntityQuery::default().of_type(EntityType::Project)) {
        Ok(page) => page,
        Err(e) => return internal_error(&format!("list projects: {e}")),
    };
    let needle = reference.to_lowercase();
    let Some(Entity::Project(project)) = projects.items.into_iter().find(|p| match p {
        Entity::Project(pr) => {
            pr.id.as_str() == reference
                || pr.slug.eq_ignore_ascii_case(&reference)
                || pr.name.to_lowercase() == needle
        }
        _ => false,
    }) else {
        return bad_request(&format!("no project matches `{reference}`"));
    };

    let repo_root = match body.get("repo").and_then(Value::as_str) {
        Some(path) => std::path::PathBuf::from(path),
        None => match project.root_path.as_deref() {
            Some(path) => std::path::PathBuf::from(path),
            None => {
                return bad_request(&format!(
                    "{} has no root_path recorded, so there is nowhere to write. Pass `repo`, or \
                     set root_path on the project",
                    project.slug
                ));
            }
        },
    };

    let mode = if check { Mode::Check } else { Mode::Write };

    // Decide with the store, write without it.
    //
    // This used to be one `generate::all` under the lock, and the lock covered
    // several dozen small file writes as well as every read. A generate against
    // this project's own store took long enough that the CLI's health probe
    // timed out and concluded no daemon was there — so the daemon produced the
    // second writer the probe exists to prevent.
    let plan = match generate::plan(&store, &project.id, &repo_root) {
        Ok(plan) => plan,
        Err(e) => return internal_error(&format!("plan the generate for {}: {e}", project.slug)),
    };
    let slug = project.slug.clone();
    drop(store);

    match plan.apply(mode) {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({ "data": {
                "written": report.written,
                "unchanged": report.unchanged,
                "unrepresented": report.unrepresented,
                "orphans": report.orphans,
                "checked": check,
            }})),
        )
            .into_response(),
        Err(e) => internal_error(&format!("generate {slug}: {e}")),
    }
}

/// One error shape for the whole local API.
///
/// There were three: a bare string, `{message}`, and the full `{code, message}`
/// that the MCP side returns. The desktop client reads `error.message`, so the
/// bare-string form arrived as `undefined` and the app showed "Request failed
/// (400)" — the one case where the daemon had actually explained itself.
///
/// The shape is the MCP one, because that is the one a caller may already know
/// and because the two surfaces are supposed to be the same surface.
fn api_error(status: StatusCode, code: i32, message: impl std::fmt::Display) -> Response {
    (
        status,
        Json(json!({ "error": { "code": code, "message": message.to_string() } })),
    )
        .into_response()
}

fn bad_request(message: &str) -> Response {
    api_error(StatusCode::BAD_REQUEST, codes::INVALID_PARAMS, message)
}

fn internal_error(message: &str) -> Response {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": { "code": codes::INTERNAL_ERROR, "message": message } })),
    )
        .into_response()
}

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

/// Turn a query string into tool arguments, using the tool's own schema.
///
/// This is what makes the REST surface and the MCP surface the same surface
/// rather than two that resemble each other. The previous version guessed from
/// the *value*: anything that parsed as an integer became a number, "true" and
/// "false" became booleans, everything else stayed a string. Two bugs fell out
/// of that guess, and both were live:
///
///  - **`?types=spec` was silently dropped.** The schema says `types` is an
///    array; a bare string was passed through, the tool ignored it, and the
///    search returned every type with no error at all. A filter that is ignored
///    without complaint is worse than one that fails.
///  - **`?query=404` failed with "query must be a string".** It parsed as an
///    integer, so it arrived as the number 404 and the tool rejected it. The
///    one search term guaranteed to be numeric is an HTTP status code, which is
///    exactly the sort of thing anyone would search a project for.
///
/// Reading the declared type instead of guessing fixes both, and cannot drift:
/// the schema being consulted is the one the tool advertises.
fn params_to_json(tool: &str, params: std::collections::HashMap<String, String>) -> Value {
    let schema = keel_mcp::tools::all()
        .into_iter()
        .find(|t| t.name == tool)
        .map(|t| t.input_schema);
    let properties = schema
        .as_ref()
        .and_then(|s| s.get("properties"))
        .and_then(Value::as_object);

    let mut out = serde_json::Map::new();
    for (key, raw) in params {
        let declared = properties
            .and_then(|p| p.get(&key))
            .and_then(|p| p.get("type"))
            .and_then(Value::as_str);

        let value = match declared {
            Some("array") => Value::Array(
                raw.split(',')
                    .map(str::trim)
                    .filter(|part| !part.is_empty())
                    .map(|part| Value::String(part.to_owned()))
                    .collect(),
            ),
            Some("integer") | Some("number") => raw
                .parse::<i64>()
                .map(|n| json!(n))
                .unwrap_or_else(|_| json!(raw)),
            Some("boolean") => json!(raw == "true"),
            Some("string") => json!(raw),
            // Undeclared: pass it through untouched. Guessing is what caused
            // both bugs above, and a parameter the schema does not mention is
            // the last place to start guessing.
            _ => json!(raw),
        };
        out.insert(key, value);
    }
    Value::Object(out)
}

async fn api_context(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json("keel_context", params);
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
    let args = params_to_json("keel_projects", params);
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
    let args = params_to_json("keel_search", params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_search",
            arguments: &args,
        },
    ))
}

/// What can be worked on right now.
///
/// The same `keel_ready` the CLI and a model call, reached the same way every
/// other read is: through the tool, with the query string mapped by the tool's
/// own schema. That is what makes "the app agrees with the session" a property of
/// the code rather than a thing to keep checking — there is one ranking, and all
/// three surfaces read it.
///
/// `?blocked=true` adds the ids of the tasks something live is blocking. It is
/// not a tool parameter and deliberately not one: a model asking "what can I
/// pick up" does not want the stuck list, and the board does — it draws a
/// blocked column. The board's alternative was the whole digest, which costs
/// twenty-seven kilobytes and every section of a project summary to read one
/// field, so the parameter exists to stop a view paying for a briefing (B-15:
/// the local API may have more than the tool surface does, because a UI knows
/// what it wants).
///
/// The ids come from [`keel_core::next::blocked_tasks`], which is *the*
/// definition of blocked. Recomputing it here in any other way is how the app
/// and the digest would come to disagree.
///
/// It is not free, and the cost is written down rather than left to be
/// rediscovered. Asking for blocked walks the `blocks` edges a second time: the
/// ranking inside the tool has already walked them and thrown that half away.
/// Measured over fifteen rounds against a copy of the live store, all three on
/// the same build — ranking alone 316 ms, ranking with blocked 558 ms, the
/// digest this replaces 724 ms and twenty-three times the bytes.
///
/// So it is still the cheaper call, and it is one of four the board makes in
/// parallel. The version with no second walk means either the daemon stops
/// going through the tool — and the app's ranking stops being the tool's by
/// construction — or `keel_ready` starts returning a stuck list no model asked
/// for. Neither is worth 240 ms on a screen that loads once. If it ever is, the
/// fix is a `blocked` field on [`keel_core::Ready`] carrying what the ranking
/// already computed, not a second ranking here.
async fn api_ready(
    State(state): State<AppState>,
    Query(mut params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    // Taken out before the arguments are built: `keel_ready` has no `blocked`
    // in its schema, and passing an undeclared parameter through to a tool is
    // how a filter gets silently ignored.
    let want_blocked = params.remove("blocked").is_some_and(|v| v == "true");
    let project = params.get("project").cloned();

    let args = params_to_json("keel_ready", params);
    let mut store = state.store();
    let mut result = keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_ready",
            arguments: &args,
        },
    );

    if want_blocked && let Ok(value) = &mut result {
        let Some(slug) = project else {
            return bad_request("`blocked=true` needs `project` — blocked is per project");
        };
        let project_id = match keel_mcp::dispatch::resolve_project(&store, &slug) {
            Ok(id) => id,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
            }
        };
        match keel_core::next::blocked_tasks(&*store, &project_id) {
            Ok(ids) => {
                // Sorted so two identical stores answer identically. The set is
                // a `HashSet`, and an order that wobbles between calls would
                // make a snapshot test flap for no reason.
                let mut ids: Vec<String> = ids.iter().map(ToString::to_string).collect();
                ids.sort();
                if let Some(obj) = value
                    .get_mut("structuredContent")
                    .and_then(Value::as_object_mut)
                {
                    obj.insert("blocked".to_owned(), json!(ids));
                }
            }
            Err(e) => return internal_error(&format!("list what is blocked in {slug}: {e}")),
        }
    }

    as_api(result)
}

/// One row's whole history — every field change, with its before and after.
///
/// Its own endpoint rather than a parameter on `/api/activity`, because
/// `/api/activity` *is* `keel_activity` and that tool no longer takes one
/// (TQ-24). B-15 is why this is not a contradiction: the local API has more
/// endpoints than the tool surface has tools, since a UI knows exactly what it
/// wants and a model chooses worse among more options.
///
/// Not paged from the feed and filtered, which is what a caller would otherwise
/// have to do: that silently misses anything older than the page, and a history
/// that quietly starts partway through is worse than no history at all.
async fn api_entity_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    use keel_core::EntityStore as _;

    let store = state.store();
    let entity_id = match store.resolve_ref(&id) {
        Ok(Some(id)) => id,
        Ok(None) => {
            return api_error(
                StatusCode::NOT_FOUND,
                codes::INVALID_PARAMS,
                format!("`{id}` names nothing in this store"),
            );
        }
        Err(e) => return api_error(StatusCode::BAD_REQUEST, codes::INVALID_PARAMS, e),
    };
    let limit = params
        .get("limit")
        .and_then(|l| l.parse::<usize>().ok())
        .unwrap_or(500)
        .clamp(1, 5_000);
    match store.events_for(&entity_id, limit) {
        Ok(page) => (
            StatusCode::OK,
            Json(json!({ "data": {
                "events": page.items,
                "total": page.total,
                "truncated": page.truncated,
            }})),
        )
            .into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

/// The bytes of a stored blob, with its own content type.
///
/// Served raw rather than base64 in JSON: this is what an `<img src>` points
/// at, and making the app decode a megabyte of JSON to show a screenshot would
/// be paying the tool-call tax twice for no reason.
async fn api_blob(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let blob_id = match keel_core::BlobId::parse(&id) {
        Ok(b) => b,
        Err(e) => return api_error(StatusCode::BAD_REQUEST, codes::INVALID_PARAMS, e),
    };
    let store = state.store();
    match store.get_blob(&blob_id) {
        Ok(Some(blob)) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, blob.media_type.clone()),
                // Content-addressed and never rewritten, so it can be cached
                // hard. A blob id names one sequence of bytes forever.
                (
                    header::CACHE_CONTROL,
                    "public, max-age=31536000, immutable".to_owned(),
                ),
            ],
            blob.bytes,
        )
            .into_response(),
        Ok(None) => api_error(
            StatusCode::NOT_FOUND,
            codes::INVALID_PARAMS,
            format!("no blob `{id}`"),
        ),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

/// Cross-engine integrity, run inside the process that holds the lock.
/// What changed, grouped by the conversation that changed it.
///
/// Its own endpoint rather than a shape on `/api/activity`, because that URL *is*
/// the `keel_activity` tool and this is a different question: the tool answers
/// "every mutation since a cursor", paged, for a model catching up, and this
/// answers "what did each session do", for a person who left Claude working and
/// came back. B-15 is the rule — the local API has more endpoints than the tool
/// surface has tools, because a UI knows exactly what it wants.
///
/// The union with notes is the part that could not be done client-side: notes
/// leave no row in `events` (TQ-29), so a per-session count built from the feed
/// alone silently misses the part most worth reading.
async fn api_changes(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let store = state.store();

    let project_id = match params.get("project") {
        None => None,
        Some(reference) => match keel_mcp::resolve_project(&store, reference) {
            Ok(id) => Some(id),
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({ "error": { "code": e.code, "message": e.message } })),
                )
                    .into_response();
            }
        },
    };

    let since = match params.get("since") {
        None => None,
        Some(raw) => match chrono::DateTime::parse_from_rfc3339(raw) {
            Ok(t) => Some(t.with_timezone(&chrono::Utc)),
            Err(_) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    codes::INVALID_PARAMS,
                    keel_core::Error::Invariant {
                        operation: "read what changed".to_owned(),
                        problem: format!("`since` is not an RFC 3339 timestamp: {raw}"),
                    },
                );
            }
        },
    };

    let actor = match params.get("actor") {
        None => None,
        Some(raw) => match keel_core::Actor::parse(raw) {
            Ok(a) => Some(a),
            Err(e) => return api_error(StatusCode::BAD_REQUEST, codes::INVALID_PARAMS, e),
        },
    };

    let query = keel_core::ChangeQuery {
        project_id,
        since,
        actor,
        limit: params
            .get("limit")
            .and_then(|l| l.parse::<usize>().ok())
            .unwrap_or(300)
            .clamp(1, 2_000),
    };

    match keel_core::changes::by_session(&store, &query) {
        Ok(log) => (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "sessions": log.sessions.iter().map(|s| json!({
                        "session_id": s.session_id,
                        "actor": s.actor.as_str(),
                        "started_at": s.started_at,
                        "ended_at": s.ended_at,
                        "headline": s.headline,
                        "changes": s.changes.iter().map(|c| json!({
                            "id": c.id,
                            "kind": c.kind.as_str(),
                            "entity_id": c.entity_id.to_string(),
                            "entity_type": c.entity_type.as_str(),
                            "reference": c.reference,
                            "summary": c.summary,
                            "at": c.at,
                        })).collect::<Vec<_>>(),
                    })).collect::<Vec<_>>(),
                    "changes": log.changes,
                    "truncated": log.truncated,
                }
            })),
        )
            .into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

/// Which rows a reader would struggle with.
///
/// Served here for the same reason `fsck` is: the CLI cannot open the store while
/// this process holds the write lock, and a report you have to stop the daemon to
/// read is one nobody runs.
///
/// Not an MCP tool, deliberately. Thirteen is the ceiling and this is
/// housekeeping a person works through — a model handed a list of ninety rows to
/// improve would improve them by inventing prose, which is the failure the rule
/// exists to prevent.
async fn api_lint(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let store = state.store();
    let Some(reference) = params.get("project") else {
        return api_error(
            StatusCode::BAD_REQUEST,
            codes::INVALID_PARAMS,
            keel_core::Error::Invariant {
                operation: "lint a project".to_owned(),
                problem: "no `project` given, and lint reports on one project at a time".to_owned(),
            },
        );
    };
    let project = match keel_mcp::resolve_project(&store, reference) {
        Ok(id) => id,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": { "code": e.code, "message": e.message } })),
            )
                .into_response();
        }
    };
    let limit = params.get("limit").and_then(|l| l.parse::<usize>().ok());
    match keel_core::lint(&store, &project, limit) {
        Ok(report) => (
            StatusCode::OK,
            Json(json!({
                "data": {
                    "findings": report.findings.iter().map(|f| json!({
                        "check": f.check,
                        "id": f.id.to_string(),
                        "reference": f.reference,
                        "detail": f.detail,
                    })).collect::<Vec<_>>(),
                    "by_check": report.by_check().iter()
                        .map(|(c, n)| json!({ "check": c, "count": n }))
                        .collect::<Vec<_>>(),
                    "scanned": report.scanned,
                    "total": report.total,
                    "truncated": report.truncated,
                }
            })),
        )
            .into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

async fn api_fsck(State(state): State<AppState>) -> Response {
    let store = state.store();
    match keel_core::fsck::check(&store) {
        Ok(report) => (StatusCode::OK, Json(json!({ "data": report }))).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

/// A one-line summary of what is in the store.
async fn api_status(State(state): State<AppState>) -> Response {
    use keel_core::{EntityQuery, EntityStore, EntityType};
    let store = state.store();
    let counts = (|| -> keel_core::Result<Value> {
        let projects = store.list(&EntityQuery::default().of_type(EntityType::Project))?;
        let tasks = store.list(
            &EntityQuery::default()
                .of_type(EntityType::Task)
                .with_status(["todo", "in_progress", "review"]),
        )?;
        let questions = store.list(
            &EntityQuery::default()
                .of_type(EntityType::Question)
                .with_status(["open"]),
        )?;
        Ok(json!({
            "projects": projects.total,
            "open_tasks": tasks.total,
            "open_questions": questions.total,
        }))
    })();
    match counts {
        Ok(v) => (StatusCode::OK, Json(json!({ "data": v }))).into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

/// The tracker as markdown, rendered from the task rows.
///
/// Returns the text rather than writing a file: where it goes is the caller's
/// business, and the daemon has no idea which repository the caller is standing
/// in. `POST /api/generate` is the one that writes.
async fn api_render_status(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    use keel_core::EntityStore as _;

    let Some(project) = params.get("project") else {
        return api_error(
            StatusCode::BAD_REQUEST,
            codes::INVALID_PARAMS,
            "`project` is required: a tracker belongs to one project",
        );
    };
    use keel_core::{EntityQuery, EntityType};

    let store = state.store();
    // Matched by slug, key or name, the same three a person would type. The
    // CLI resolves the same way; a project the CLI can name and the daemon
    // cannot would be a difference nobody could explain.
    let needle = project.to_lowercase();
    let found = match store.list(&EntityQuery::default().of_type(EntityType::Project)) {
        Ok(page) => page.items.into_iter().find(|e| match e {
            keel_core::Entity::Project(p) => {
                p.slug.to_lowercase() == needle
                    || p.key.to_lowercase() == needle
                    || p.name.to_lowercase() == needle
            }
            _ => false,
        }),
        Err(e) => return api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    };
    let Some(found) = found else {
        return api_error(
            StatusCode::NOT_FOUND,
            codes::INVALID_PARAMS,
            format!("no project named `{project}`"),
        );
    };
    match keel_core::render_status::render(&store, found.id()) {
        Ok(markdown) => (
            StatusCode::OK,
            Json(json!({ "data": { "markdown": markdown } })),
        )
            .into_response(),
        Err(e) => api_error(StatusCode::INTERNAL_SERVER_ERROR, codes::INTERNAL_ERROR, e),
    }
}

async fn api_activity(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let args = params_to_json("keel_activity", params);
    let mut store = state.store();
    as_api(keel_mcp::dispatch(
        &mut store,
        keel_mcp::ToolCall {
            name: "keel_activity",
            arguments: &args,
        },
    ))
}

/// Resolve a path parameter that may be a ULID or a readable reference.
///
/// The app puts `KEEL-42` in its URLs, because that is what a person copies out
/// of a conversation and pastes into the address bar. A 400 distinguishes "that
/// is not a reference" from a 404's "no such thing".
// The error variant is a whole `Response`, which clippy would rather was boxed.
// It is not: this is the one-per-request path, the alternative is an allocation
// on the failure branch of a handler that is about to allocate a JSON body
// anyway, and boxing it would put a `*` at every call site for nothing.
#[allow(clippy::result_large_err)]
fn resolve_path_id(
    store: &keel_core::Store,
    raw: &str,
) -> std::result::Result<keel_core::EntityId, Response> {
    use keel_core::EntityStore;
    match store.resolve_ref(raw) {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(api_error(
            StatusCode::NOT_FOUND,
            codes::INVALID_PARAMS,
            format!("`{raw}` does not name anything"),
        )),
        Err(e) => Err(api_error(
            StatusCode::BAD_REQUEST,
            codes::INVALID_PARAMS,
            e.to_string(),
        )),
    }
}

async fn api_entity(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let mut args = params_to_json("keel_get", params);
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

/// A row's running commentary.
///
/// Its own endpoint rather than a field on `/api/entities`: a board renders
/// seventy cards and wants none of the note bodies, while a detail view wants
/// one card's in full. Folding them into the list would make the common case
/// pay for the rare one.
///
/// `entity` fetches one stream; `project` fetches every live note in a project,
/// which is what a view showing several cards at once actually needs.
///
/// `?counts=true` returns `{entity_id: n}` instead of the notes themselves. The
/// board renders a hundred and twenty cards and puts a number on each one; it
/// was reading a hundred and fifty kilobytes of note prose across the wire to
/// count them and then throwing every body away. The read against the store is
/// the same either way — the saving is the transfer and the parse, which is the
/// part the browser was actually waiting on.
async fn api_notes(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    use keel_core::EntityStore;

    let counts_only = params.get("counts").is_some_and(|v| v == "true");
    let store = state.store();
    let notes = if let Some(entity) = params.get("entity") {
        match resolve_path_id(&store, entity) {
            Ok(id) => store.notes_for(&id, params.get("all").is_some_and(|v| v == "true")),
            Err(response) => return response,
        }
    } else if let Some(project) = params.get("project") {
        match keel_mcp::dispatch::resolve_project(&store, project) {
            Ok(id) => store.notes_in_project(&id),
            // `RpcError` is a wire shape, not a Display type — pass it through
            // as the structured error it already is.
            Err(e) => {
                // `RpcError` already serialises as `{code, message}` — the
                // same shape `api_error` builds — so it is passed through
                // whole rather than flattened to its message.
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
            }
        }
    } else {
        return bad_request(
            "pass `entity` for one row's notes, or `project` for all of a project's",
        );
    };

    match notes {
        Ok(notes) if counts_only => {
            let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
            for note in &notes {
                *counts.entry(note.entity_id.to_string()).or_default() += 1;
            }
            (
                StatusCode::OK,
                Json(json!({ "data": { "counts": counts, "total": notes.len() } })),
            )
                .into_response()
        }
        Ok(notes) => (
            StatusCode::OK,
            Json(json!({ "data": { "notes": notes, "total": notes.len() } })),
        )
            .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            codes::INTERNAL_ERROR,
            e.to_string(),
        ),
    }
}

/// List entities with filters.
///
/// Part of Keel's own API, not MCP. The tool surface is capped at ten because
/// more tools makes a model choose worse (SPEC §6.1) — that reasoning does not
/// apply to a UI, which knows exactly what it wants and would otherwise have to
/// fetch everything and filter client-side.
async fn api_entities(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    use keel_core::{EntityQuery, EntityStore, EntityType};

    let store = state.store();
    let mut query = EntityQuery::default();

    if let Some(project) = params.get("project") {
        match keel_mcp::dispatch::resolve_project(&store, project) {
            Ok(id) => query.project_id = Some(id),
            Err(e) => {
                // `RpcError` already serialises as `{code, message}` — the
                // same shape `api_error` builds — so it is passed through
                // whole rather than flattened to its message.
                return (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))).into_response();
            }
        }
    }
    if let Some(types) = params.get("type") {
        let parsed: Result<Vec<EntityType>, _> = types.split(',').map(EntityType::parse).collect();
        match parsed {
            Ok(t) => query.entity_types = t,
            Err(e) => {
                return api_error(
                    StatusCode::BAD_REQUEST,
                    codes::INVALID_PARAMS,
                    e.to_string(),
                );
            }
        }
    }
    if let Some(status) = params.get("status") {
        query.statuses = status.split(',').map(str::to_owned).collect();
    }
    query.include_archived = params.get("include_archived").is_some_and(|v| v == "true");
    query.limit = params.get("limit").and_then(|l| l.parse().ok());

    match store.list(&query) {
        Ok(page) => (
            StatusCode::OK,
            Json(json!({
                "data": {
                    // The same shaping every other surface uses, so `version`
                    // is where a caller expects it regardless of endpoint.
                    "items": page.items.iter().map(keel_mcp::entity_json).collect::<Vec<_>>(),
                    "total": page.total,
                    "truncated": page.truncated
                }
            })),
        )
            .into_response(),
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            codes::INTERNAL_ERROR,
            e.to_string(),
        ),
    }
}

/// A document's full revision history, and optionally a diff.
async fn api_document(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let store = state.store();
    let entity_id = match resolve_path_id(&store, &id) {
        Ok(i) => i,
        Err(response) => return response,
    };

    let history = store.revisions(&entity_id).unwrap_or_default();
    let current = params
        .get("version")
        .and_then(|v| v.parse::<i32>().ok())
        .or_else(|| history.last().map(|d| d.version));

    let body = current.and_then(|v| history.iter().find(|d| d.version == v).cloned());

    let diff = match (
        params
            .get("diff_against")
            .and_then(|v| v.parse::<i32>().ok()),
        current,
    ) {
        (Some(other), Some(v)) => store
            .diff(&entity_id, other.min(v), other.max(v))
            .ok()
            .map(|d| serde_json::to_value(d).unwrap_or(Value::Null)),
        _ => None,
    };

    (
        StatusCode::OK,
        Json(json!({
            "data": {
                "revisions": history.iter().map(|d| json!({
                    "version": d.version,
                    "title": d.title,
                    "author": d.author,
                    "session_id": d.session_id,
                    "surface": d.surface,
                    "created_at": d.created_at,
                    "status": d.status,
                })).collect::<Vec<_>>(),
                "document": body,
                "diff": diff,
            }
        })),
    )
        .into_response()
}

/// The graph around an entity.
async fn api_graph(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    use keel_core::{DEFAULT_DEPTH, Direction, GraphStore};

    let store = state.store();
    let entity_id = match resolve_path_id(&store, &id) {
        Ok(i) => i,
        Err(response) => return response,
    };
    let direction = params
        .get("direction")
        .and_then(|d| Direction::parse(d).ok())
        .unwrap_or(Direction::Both);
    let depth = params
        .get("depth")
        .and_then(|d| d.parse::<u8>().ok())
        .unwrap_or(DEFAULT_DEPTH);

    match store.neighbours(&entity_id, direction, &[], depth) {
        Ok(neighbours) => {
            let links = store.links_of(&entity_id, direction).unwrap_or_default();
            (
                StatusCode::OK,
                Json(json!({ "data": { "neighbours": neighbours, "links": links } })),
            )
                .into_response()
        }
        Err(e) => api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            codes::INTERNAL_ERROR,
            e.to_string(),
        ),
    }
}

/// Live change notifications for the desktop app.
async fn api_events_stream(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = state.changes.subscribe();
    let stream = async_stream::stream! {
        let mut rx = rx;

        // Say something immediately, before waiting for a change.
        //
        // Nothing else would flow until the first write or the first keep-alive
        // fifteen seconds later, and a stream that sends no bytes is one an
        // intermediary is free to sit on: a proxy that buffers until it has a
        // body holds the *headers* too, so the browser's EventSource never
        // fires `open` and live refresh is silently dead. That is exactly what
        // happened behind the dev server's proxy.
        //
        // A comment rather than an event: `EventSource` ignores it, so no
        // client has to know this exists, and it costs one line on the wire.
        yield Ok(SseEvent::default().comment("keel"));

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
            // The one that used to be on the allowed list. A browser sends
            // it from a sandboxed iframe, a `file://` page and a redirected
            // cross-origin request — every context an attacker can arrange,
            // and none that a real MCP client ever produces.
            "null",
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
        params.insert("query".to_owned(), "onboarding".to_owned());

        let json = params_to_json("keel_search", params);
        assert_eq!(json["limit"], 25);
        assert_eq!(json["query"], "onboarding");

        // A boolean, from the tool that actually declares one. This assertion
        // used to name `include_archived` on `keel_search`, which does not
        // take it — the old value-guessing conversion turned it into a boolean
        // anyway, so the test passed while describing a parameter that was
        // being silently discarded one layer down.
        let mut params = std::collections::HashMap::new();
        params.insert("include_archived".to_owned(), "true".to_owned());
        assert_eq!(
            params_to_json("keel_projects", params)["include_archived"],
            true
        );
    }

    #[test]
    fn a_list_parameter_arrives_as_a_list() {
        // `?types=spec` used to be passed through as the string "spec", which
        // the tool ignored — so a search restricted to specs returned every
        // type, with no error. A filter that is ignored without complaint is
        // worse than one that fails.
        let mut params = std::collections::HashMap::new();
        params.insert("types".to_owned(), "spec,decision".to_owned());
        let json = params_to_json("keel_search", params);
        assert_eq!(json["types"], json!(["spec", "decision"]));
    }

    #[test]
    fn a_numeric_looking_search_term_stays_a_string() {
        // The one search term guaranteed to be numeric is an HTTP status code,
        // and `?query=404` failed with "query must be a string".
        let mut params = std::collections::HashMap::new();
        params.insert("query".to_owned(), "404".to_owned());
        let json = params_to_json("keel_search", params);
        assert_eq!(json["query"], "404");
    }

    #[test]
    fn a_number_still_arrives_as_a_number() {
        // The schema says `limit` is an integer, so it must not become "25".
        let mut params = std::collections::HashMap::new();
        params.insert("limit".to_owned(), "25".to_owned());
        assert_eq!(params_to_json("keel_search", params)["limit"], 25);
    }
}
