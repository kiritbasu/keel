//! JSON-RPC and the 2026-07-28 stateless wire contract.
//!
//! The headline of this revision is that MCP has **no sessions**. The
//! `initialize`/`notifications/initialized` handshake is gone, `Mcp-Session-Id`
//! is gone, and every request carries its own protocol version and client
//! identity in `_meta`. That is genuinely simpler to serve — but it is also
//! why `session_id` has to be a *domain* concept supplied by the caller
//! (SPEC §6.5, D-10). There is no protocol session to borrow.
//!
//! `product/SPEC.md` §6 was written from the announcement rather than the
//! finished specification, so several things here are not in it. They are
//! recorded in `product/DECISIONS.md` under "MCP deltas"; the important ones:
//! `server/discover` is required, every result carries `resultType`, and
//! `tools/list` must return cache hints.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// The current protocol revision.
pub const PROTOCOL_VERSION: &str = "2026-07-28";

/// The previous revision, still spoken by shipping clients.
///
/// Claude Code 2.1.185 — the primary client this whole product exists to serve
/// — opens with `initialize` and declares `2025-11-25`. A server that speaks
/// only the current revision is unusable with it, which would make Phase 2's
/// gate impossible to even attempt. Supporting both is a MAY in the spec's
/// backward-compatibility section; here it is the difference between working
/// and not. See DECISIONS B-17.
pub const LEGACY_PROTOCOL_VERSION: &str = "2025-11-25";

/// Every revision this daemon serves, newest first.
pub const SUPPORTED_VERSIONS: [&str; 2] = [PROTOCOL_VERSION, LEGACY_PROTOCOL_VERSION];

/// Which revision a request belongs to.
///
/// The two differ in ways that reach the response, not just the request:
/// `Modern` requires `resultType` on every result and mirrored headers on every
/// POST; `Legacy` has an `initialize` handshake and neither of those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Era {
    /// 2026-07-28: stateless, mirrored headers, `resultType`.
    Modern,
    /// 2025-11-25: `initialize` handshake, no mirrored headers.
    Legacy,
}

impl Era {
    /// The version string to echo back.
    pub const fn version(self) -> &'static str {
        match self {
            Era::Modern => PROTOCOL_VERSION,
            Era::Legacy => LEGACY_PROTOCOL_VERSION,
        }
    }
}

/// `_meta` key carrying the protocol version.
pub const META_PROTOCOL_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
/// `_meta` key carrying client identity.
pub const META_CLIENT_INFO: &str = "io.modelcontextprotocol/clientInfo";
/// `_meta` key carrying client capabilities.
pub const META_CLIENT_CAPABILITIES: &str = "io.modelcontextprotocol/clientCapabilities";
/// `_meta` key carrying server identity on results.
pub const META_SERVER_INFO: &str = "io.modelcontextprotocol/serverInfo";

/// Header mirroring the JSON-RPC `method`.
pub const HEADER_METHOD: &str = "mcp-method";
/// Header mirroring `params.name` or `params.uri`.
pub const HEADER_NAME: &str = "mcp-name";
/// Header carrying the protocol version.
pub const HEADER_PROTOCOL_VERSION: &str = "mcp-protocol-version";

/// The Base64 sentinel wrapping a header value that is not plain ASCII.
const B64_PREFIX: &str = "=?base64?";
/// The closing half of the sentinel.
const B64_SUFFIX: &str = "?=";

/// JSON-RPC and MCP error codes.
///
/// The MCP-specific numbers were renumbered late in the revision: the
/// `-3200{1,3,4}` values that appeared in drafts are wrong, and `-32020`
/// upwards is the range the specification reserves for itself.
pub mod codes {
    /// Malformed JSON.
    pub const PARSE_ERROR: i32 = -32700;
    /// Not a valid JSON-RPC request.
    pub const INVALID_REQUEST: i32 = -32600;
    /// Unknown method. Served with HTTP 404, which distinguishes a modern
    /// server from a legacy one that does not host the MCP endpoint at all.
    pub const METHOD_NOT_FOUND: i32 = -32601;
    /// Bad arguments. Also used for "resource not found", which was moved
    /// here from `-32002` to match JSON-RPC.
    pub const INVALID_PARAMS: i32 = -32602;
    /// Something failed inside the server.
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Headers disagree with the body, or a required header is missing.
    pub const HEADER_MISMATCH: i32 = -32020;
    /// The client did not declare a capability the server needs.
    pub const MISSING_REQUIRED_CLIENT_CAPABILITY: i32 = -32021;
    /// The requested protocol version is not served.
    pub const UNSUPPORTED_PROTOCOL_VERSION: i32 = -32022;
    /// Keel's own: an update lost an optimistic-concurrency race. Inside the
    /// implementation-defined range, which is where a server's own errors
    /// belong.
    pub const CONFLICT: i32 = -32001;
}

/// An incoming JSON-RPC request.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    /// Must be `"2.0"`.
    pub jsonrpc: String,
    /// Absent for a notification.
    #[serde(default)]
    pub id: Option<Value>,
    /// The method name.
    pub method: String,
    /// Method arguments.
    #[serde(default)]
    pub params: Value,
}

impl Request {
    /// Whether this is a notification rather than a request.
    ///
    /// Notifications get `202 Accepted` and no body. This revision defines no
    /// client-to-server notifications in the core protocol, so in practice
    /// this is only reached by a non-conforming client — but answering
    /// correctly costs one branch.
    pub fn is_notification(&self) -> bool {
        self.id.is_none()
    }

    /// The value `Mcp-Name` must match, if the method requires one.
    ///
    /// `params.name` for `tools/call` and `prompts/get`, `params.uri` for
    /// `resources/read`. Any other method has no name to mirror.
    pub fn expected_name(&self) -> Option<String> {
        match self.method.as_str() {
            "tools/call" | "prompts/get" => self
                .params
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_owned),
            "resources/read" => self
                .params
                .get("uri")
                .and_then(Value::as_str)
                .map(str::to_owned),
            _ => None,
        }
    }

    /// The protocol version the body declares.
    pub fn declared_version(&self) -> Option<String> {
        self.params
            .get("_meta")
            .and_then(|m| m.get(META_PROTOCOL_VERSION))
            .and_then(Value::as_str)
            .map(str::to_owned)
    }

    /// The client's self-reported identity, for logging.
    pub fn client_info(&self) -> Option<Value> {
        self.params
            .get("_meta")
            .and_then(|m| m.get(META_CLIENT_INFO))
            .cloned()
    }

    /// The `arguments` object of a `tools/call`.
    pub fn arguments(&self) -> &Value {
        self.params.get("arguments").unwrap_or(&Value::Null)
    }

    /// The tool name of a `tools/call`.
    pub fn tool_name(&self) -> Option<&str> {
        self.params.get("name").and_then(Value::as_str)
    }
}

/// A JSON-RPC error, ready to serialise.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RpcError {
    /// The numeric code.
    pub code: i32,
    /// A human- and model-readable message.
    pub message: String,
    /// Structured detail. Carries the 409 payload for [`codes::CONFLICT`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl RpcError {
    /// An error with no structured data.
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        RpcError {
            code,
            message: message.into(),
            data: None,
        }
    }

    /// Attach structured detail.
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    /// The HTTP status this error should be served with.
    ///
    /// The mapping matters more than it looks: a client uses `400` plus a
    /// recognised modern error code to tell a 2026-07-28 server apart from a
    /// legacy one, and `404` with `-32601` to tell "no such method" apart from
    /// "no MCP endpoint here".
    pub fn http_status(&self) -> u16 {
        match self.code {
            codes::METHOD_NOT_FOUND => 404,
            codes::HEADER_MISMATCH
            | codes::UNSUPPORTED_PROTOCOL_VERSION
            | codes::MISSING_REQUIRED_CLIENT_CAPABILITY
            | codes::PARSE_ERROR
            | codes::INVALID_REQUEST
            | codes::INVALID_PARAMS => 400,
            codes::CONFLICT => 409,
            _ => 500,
        }
    }
}

/// A JSON-RPC response envelope.
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    /// Always `"2.0"`.
    pub jsonrpc: &'static str,
    /// Echoes the request id.
    pub id: Value,
    /// Present on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    /// Present on failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// A successful response.
    ///
    /// For [`Era::Modern`] this stamps `resultType: "complete"` and the server
    /// identity — `resultType` is **required** there, and omitting it makes a
    /// conforming client treat the result as coming from an older server. A
    /// `Legacy` client predates both fields, so they are left off rather than
    /// sent as noise it has to ignore.
    pub fn ok(id: Value, mut result: Value, era: Era) -> Self {
        if era == Era::Modern
            && let Some(obj) = result.as_object_mut()
        {
            obj.entry("resultType").or_insert(json!("complete"));
            let meta = obj.entry("_meta").or_insert(json!({}));
            if let Some(m) = meta.as_object_mut() {
                m.insert(META_SERVER_INFO.to_owned(), server_info());
            }
        }
        Response {
            jsonrpc: "2.0",
            id,
            result: Some(result),
            error: None,
        }
    }

    /// A failed response.
    pub fn err(id: Value, error: RpcError) -> Self {
        Response {
            jsonrpc: "2.0",
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// This server's identity, returned on every result.
pub fn server_info() -> Value {
    json!({
        "name": "keel",
        "version": env!("CARGO_PKG_VERSION"),
        "title": "Keel",
    })
}

/// Decode a header value that may carry the Base64 sentinel.
///
/// Tool names and resource URIs are only *SHOULD*-constrained to header-safe
/// characters, so a client must Base64-wrap anything else — including a plain
/// ASCII value that happens to look like the sentinel. A server comparing the
/// header to the body has to decode first or it will reject valid requests.
pub fn decode_header_value(raw: &str) -> String {
    let Some(inner) = raw
        .strip_prefix(B64_PREFIX)
        .and_then(|r| r.strip_suffix(B64_SUFFIX))
    else {
        return raw.to_owned();
    };
    match base64_decode(inner) {
        Some(bytes) => String::from_utf8(bytes).unwrap_or_else(|_| raw.to_owned()),
        None => raw.to_owned(),
    }
}

/// Minimal standard-alphabet Base64 decoder.
///
/// Hand-written rather than pulled in as a dependency: this is the only
/// Base64 in the codebase, it decodes a header at most a few dozen bytes long,
/// and a decoder is easier to read than a supply-chain entry to justify.
fn base64_decode(s: &str) -> Option<Vec<u8>> {
    fn sextet(c: u8) -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        })
    }

    let bytes: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    let unpadded: Vec<u8> = bytes.iter().copied().take_while(|b| *b != b'=').collect();
    if unpadded.len() != bytes.iter().filter(|b| **b != b'=').count() {
        return None;
    }

    let mut out = Vec::with_capacity(unpadded.len() * 3 / 4);
    for chunk in unpadded.chunks(4) {
        let mut acc = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            acc |= sextet(*c)? << (18 - 6 * i);
        }
        let produced = match chunk.len() {
            4 => 3,
            3 => 2,
            2 => 1,
            _ => return None,
        };
        for i in 0..produced {
            out.push(((acc >> (16 - 8 * i)) & 0xff) as u8);
        }
    }
    Some(out)
}

/// The outcome of validating headers against the body.
#[derive(Debug, Clone, PartialEq)]
pub enum HeaderCheck {
    /// Everything matches. Carries the revision the request belongs to.
    Ok(Era),
    /// Reject with this error and HTTP 400.
    Reject(RpcError),
}

/// Validate the mirrored headers against the request body.
///
/// The specification is explicit about why this matters: an intermediary may
/// route on the header while the server executes on the body, and a mismatch
/// between the two is a security problem, not a formatting one.
pub fn check_headers(
    request: &Request,
    method_header: Option<&str>,
    name_header: Option<&str>,
    version_header: Option<&str>,
) -> HeaderCheck {
    // Which revision is this? The header is authoritative when present. An
    // `initialize` request declares it in the body instead, because the header
    // did not exist when that method did. Absent everywhere means a client
    // older than the header itself, which is legacy by definition.
    let declared = version_header
        .map(str::to_owned)
        .or_else(|| {
            request
                .params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .or_else(|| request.declared_version());

    let era = match declared.as_deref() {
        Some(PROTOCOL_VERSION) => Era::Modern,
        // Every revision from the header's introduction up to the current one
        // is served as legacy: the tool surface is identical and the wire
        // differences are confined to the handshake and the result envelope.
        Some("2025-11-25" | "2025-06-18" | "2025-03-26") | None => Era::Legacy,
        Some(other) => {
            return HeaderCheck::Reject(
                RpcError::new(
                    codes::UNSUPPORTED_PROTOCOL_VERSION,
                    format!(
                        "this server speaks {}, not {other}",
                        SUPPORTED_VERSIONS.join(" and ")
                    ),
                )
                .with_data(json!({ "supported": SUPPORTED_VERSIONS })),
            );
        }
    };

    // The mirrored headers are required only by the current revision. A legacy
    // client sends none of them, and demanding them is precisely how this
    // daemon locked out the client it exists to serve.
    if era == Era::Legacy {
        return HeaderCheck::Ok(era);
    }

    if let Some(body_version) = request.declared_version()
        && Some(body_version.as_str()) != version_header
    {
        return HeaderCheck::Reject(RpcError::new(
            codes::HEADER_MISMATCH,
            format!(
                "MCP-Protocol-Version header value `{}` does not match the body's \
                 {META_PROTOCOL_VERSION} value `{body_version}`",
                version_header.unwrap_or("(absent)")
            ),
        ));
    }

    match method_header {
        None => {
            return HeaderCheck::Reject(RpcError::new(
                codes::HEADER_MISMATCH,
                "missing required header `Mcp-Method`",
            ));
        }
        Some(m) if m != request.method => {
            return HeaderCheck::Reject(RpcError::new(
                codes::HEADER_MISMATCH,
                format!(
                    "Mcp-Method header value `{m}` does not match body method `{}`",
                    request.method
                ),
            ));
        }
        Some(_) => {}
    }

    if let Some(expected) = request.expected_name() {
        match name_header.map(decode_header_value) {
            None => {
                return HeaderCheck::Reject(RpcError::new(
                    codes::HEADER_MISMATCH,
                    format!(
                        "missing required header `Mcp-Name` — {} requires it",
                        request.method
                    ),
                ));
            }
            Some(got) if got != expected => {
                return HeaderCheck::Reject(RpcError::new(
                    codes::HEADER_MISMATCH,
                    format!("Mcp-Name header value `{got}` does not match body value `{expected}`"),
                ));
            }
            Some(_) => {}
        }
    }

    HeaderCheck::Ok(Era::Modern)
}

/// The `initialize` result a 2025-11-25 client expects.
pub fn initialize_result() -> Value {
    json!({
        "protocolVersion": LEGACY_PROTOCOL_VERSION,
        "capabilities": { "tools": { "listChanged": false } },
        "serverInfo": server_info(),
        "instructions":
            "Keel stores everything about a software project except the code. Call \
             `keel_context` first to orient. Pass a stable `session_id` on every call so writes \
             are attributed to this conversation. Before creating a project, call \
             `keel_projects` and confirm with the human."
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn request(method: &str, params: Value) -> Request {
        Request {
            jsonrpc: "2.0".to_owned(),
            id: Some(json!(1)),
            method: method.to_owned(),
            params,
        }
    }

    fn call(tool: &str) -> Request {
        request(
            "tools/call",
            json!({
                "name": tool,
                "arguments": {},
                "_meta": { META_PROTOCOL_VERSION: PROTOCOL_VERSION }
            }),
        )
    }

    #[test]
    fn matching_headers_pass() {
        let r = call("keel_context");
        assert_eq!(
            check_headers(
                &r,
                Some("tools/call"),
                Some("keel_context"),
                Some(PROTOCOL_VERSION)
            ),
            HeaderCheck::Ok(Era::Modern)
        );
    }

    #[test]
    fn a_legacy_client_is_served_without_mirrored_headers() {
        // Claude Code 2.1.185 sends none of them. Requiring them locked out
        // the client this product exists to serve — see DECISIONS B-17.
        let r = call("keel_context");
        assert_eq!(
            check_headers(&r, None, None, Some(LEGACY_PROTOCOL_VERSION)),
            HeaderCheck::Ok(Era::Legacy)
        );
    }

    #[test]
    fn an_initialize_request_declares_its_version_in_the_body() {
        // `initialize` predates the MCP-Protocol-Version header, so the only
        // place the version appears is `params.protocolVersion`.
        let r = request(
            "initialize",
            json!({"protocolVersion": "2025-11-25", "capabilities": {}}),
        );
        assert_eq!(
            check_headers(&r, None, None, None),
            HeaderCheck::Ok(Era::Legacy)
        );
    }

    #[test]
    fn a_missing_version_everywhere_is_treated_as_legacy() {
        // A client older than the header itself. Rejecting it would be
        // technically defensible and practically useless.
        let r = request("tools/list", json!({}));
        assert_eq!(
            check_headers(&r, None, None, None),
            HeaderCheck::Ok(Era::Legacy)
        );
    }

    #[test]
    fn an_unsupported_version_lists_what_is_supported() {
        let r = call("keel_context");
        match check_headers(
            &r,
            Some("tools/call"),
            Some("keel_context"),
            // 2024-11-05 is the HTTP+SSE era, which this daemon does not serve.
            // Note 2025-06-18 and 2025-03-26 *are* served — they differ from
            // 2025-11-25 only in ways the tool surface does not touch.
            Some("2024-11-05"),
        ) {
            HeaderCheck::Reject(e) => {
                assert_eq!(e.code, codes::UNSUPPORTED_PROTOCOL_VERSION);
                let data = e.data.unwrap();
                assert_eq!(data["supported"][0], PROTOCOL_VERSION);
                assert_eq!(data["supported"][1], LEGACY_PROTOCOL_VERSION);
            }
            HeaderCheck::Ok(_) => panic!("should have been rejected"),
        }
    }

    #[test]
    fn a_header_that_disagrees_with_the_body_is_rejected() {
        // The security case: a load balancer routes on the header while the
        // server executes on the body.
        let r = call("keel_create");
        match check_headers(
            &r,
            Some("tools/call"),
            Some("keel_context"),
            Some(PROTOCOL_VERSION),
        ) {
            HeaderCheck::Reject(e) => {
                assert_eq!(e.code, codes::HEADER_MISMATCH);
                assert!(e.message.contains("keel_create"), "{}", e.message);
            }
            HeaderCheck::Ok(_) => panic!("should have been rejected"),
        }
    }

    #[test]
    fn a_method_header_that_disagrees_is_rejected() {
        let r = call("keel_context");
        match check_headers(
            &r,
            Some("tools/list"),
            Some("keel_context"),
            Some(PROTOCOL_VERSION),
        ) {
            HeaderCheck::Reject(e) => assert_eq!(e.code, codes::HEADER_MISMATCH),
            HeaderCheck::Ok(_) => panic!("should have been rejected"),
        }
    }

    #[test]
    fn a_body_version_that_disagrees_with_the_header_is_rejected() {
        let r = request(
            "tools/list",
            json!({ "_meta": { META_PROTOCOL_VERSION: "2025-11-25" } }),
        );
        match check_headers(&r, Some("tools/list"), None, Some(PROTOCOL_VERSION)) {
            HeaderCheck::Reject(e) => {
                assert_eq!(e.code, codes::HEADER_MISMATCH);
                assert!(e.message.contains("2025-11-25"), "{}", e.message);
            }
            HeaderCheck::Ok(_) => panic!("should have been rejected"),
        }
    }

    #[test]
    fn methods_without_a_name_do_not_require_the_header() {
        let r = request("tools/list", json!({}));
        assert_eq!(
            check_headers(&r, Some("tools/list"), None, Some(PROTOCOL_VERSION)),
            HeaderCheck::Ok(Era::Modern)
        );
    }

    #[test]
    fn a_base64_wrapped_name_is_decoded_before_comparison() {
        // "keel_context" base64-encoded.
        let encoded = "=?base64?a2VlbF9jb250ZXh0?=";
        assert_eq!(decode_header_value(encoded), "keel_context");

        let r = call("keel_context");
        assert_eq!(
            check_headers(
                &r,
                Some("tools/call"),
                Some(encoded),
                Some(PROTOCOL_VERSION)
            ),
            HeaderCheck::Ok(Era::Modern)
        );
    }

    #[test]
    fn base64_decodes_padded_and_unpadded_input() {
        assert_eq!(decode_header_value("=?base64?aGk=?="), "hi");
        assert_eq!(decode_header_value("=?base64?aGVsbG8=?="), "hello");
        assert_eq!(
            decode_header_value("=?base64?SGVsbG8sIOS4lueVjA==?="),
            "Hello, 世界"
        );
    }

    #[test]
    fn a_plain_value_passes_through_untouched() {
        assert_eq!(decode_header_value("keel_search"), "keel_search");
        // Malformed sentinel: return it verbatim rather than guessing.
        assert_eq!(decode_header_value("=?base64?!!!?="), "=?base64?!!!?=");
    }

    #[test]
    fn a_modern_result_carries_result_type_and_server_info() {
        let r = Response::ok(json!(1), json!({"content": []}), Era::Modern);
        let result = r.result.unwrap();
        assert_eq!(
            result["resultType"], "complete",
            "required in this revision; omitting it makes clients treat us as an older server"
        );
        assert_eq!(result["_meta"][META_SERVER_INFO]["name"], "keel");
    }

    #[test]
    fn a_legacy_result_carries_neither() {
        // Both fields postdate 2025-11-25. Sending them is not harmful, but a
        // response should not contain fields the client's revision cannot
        // explain.
        let r = Response::ok(json!(1), json!({"content": []}), Era::Legacy);
        let result = r.result.unwrap();
        assert!(result.get("resultType").is_none());
        assert!(result.get("_meta").is_none());
    }

    #[test]
    fn the_initialize_result_answers_the_legacy_handshake() {
        let r = initialize_result();
        assert_eq!(r["protocolVersion"], LEGACY_PROTOCOL_VERSION);
        assert_eq!(r["serverInfo"]["name"], "keel");
        assert!(r["capabilities"]["tools"].is_object());
    }

    #[test]
    fn error_codes_map_to_the_right_http_status() {
        assert_eq!(
            RpcError::new(codes::METHOD_NOT_FOUND, "").http_status(),
            404
        );
        assert_eq!(RpcError::new(codes::HEADER_MISMATCH, "").http_status(), 400);
        assert_eq!(
            RpcError::new(codes::UNSUPPORTED_PROTOCOL_VERSION, "").http_status(),
            400
        );
        assert_eq!(RpcError::new(codes::CONFLICT, "").http_status(), 409);
        assert_eq!(RpcError::new(codes::INTERNAL_ERROR, "").http_status(), 500);
    }

    #[test]
    fn the_renumbered_codes_are_used_not_the_draft_ones() {
        // The draft values -32001/-32003/-32004 were renumbered before the
        // revision shipped. Using them would make a conforming client
        // misinterpret every error.
        assert_eq!(codes::HEADER_MISMATCH, -32020);
        assert_eq!(codes::MISSING_REQUIRED_CLIENT_CAPABILITY, -32021);
        assert_eq!(codes::UNSUPPORTED_PROTOCOL_VERSION, -32022);
    }

    #[test]
    fn a_request_without_an_id_is_a_notification() {
        let r = Request {
            jsonrpc: "2.0".to_owned(),
            id: None,
            method: "whatever".to_owned(),
            params: json!({}),
        };
        assert!(r.is_notification());
        assert!(!call("keel_get").is_notification());
    }
}
