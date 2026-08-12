//! The MCP protocol layer: tool schemas, argument decoding, response shaping.
//!
//! Split from `keel-daemon` so the tool surface can be exercised without
//! binding a port — which is what makes the snapshot tests possible, and they
//! are the API contract.
//!
//! Three modules, in the order a request meets them:
//!
//! - [`protocol`] — JSON-RPC and the 2026-07-28 stateless wire contract.
//! - [`tools`] — the thirteen tool definitions. These descriptions *are* the
//!   product; they are the only documentation an agent gets. The count is
//!   asserted in the snapshot suite, because it has been wrong here before:
//!   this line said "nine" through the tenth tool and the thirteenth.
//! - [`dispatch`] — executing a call against `keel-core`, and turning domain
//!   errors into something a model can act on.
//!
//! [`context`] builds the `keel_context` digest, which is important enough to
//! live on its own.

pub mod context;
pub mod dispatch;
pub mod protocol;
pub mod tools;

pub use context::{Depth, Digest};
pub use dispatch::{
    ToolCall, dispatch, dispatch_prepared, entity_json, payload, resolve_project, to_rpc_error,
};
pub use protocol::{
    HeaderCheck, PROTOCOL_VERSION, Request, Response, RpcError, check_headers, codes,
};
pub use tools::{Tool, all as all_tools, discover_result, find as find_tool, list_result};
