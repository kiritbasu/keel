//! The Keel daemon.
//!
//! Two surfaces on one port:
//!
//! - `POST /mcp` — the MCP endpoint, stateless Streamable HTTP (2026-07-28).
//! - `/api/*` — Keel's own REST and SSE, for the desktop app. Deliberately the
//!   same shape it would have if the daemon were remote, so the eventual web
//!   build is the same bundle with a different base URL.
//!
//! # The single write path
//!
//! One [`keel_core::DuckStore`] behind one mutex, for the whole process (D-5).
//! That is not a performance decision — DuckDB permits concurrent writer
//! threads — it is the design rule that makes the seven-step write path
//! (validate → resolve links → embed → write entity → append revision → append
//! event → regenerate mirror) atomic from a caller's point of view. Six of
//! those steps have nothing to do with locking.
//!
//! The lock is `std::sync::Mutex`, held across synchronous work and never
//! across an `.await`. At one user and a few thousand rows this is correct and
//! obvious; a connection pool or an async mutex would be machinery in search of
//! a measurement (`product/CLAUDE.md`, scale discipline).

pub mod http;
pub mod ratelimit;
pub mod state;

pub use http::router;
pub use state::AppState;
