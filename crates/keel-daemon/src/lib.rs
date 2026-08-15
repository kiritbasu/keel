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
//! One [`keel_core::Store`] behind one mutex, for the whole process (D-5).
//! That is not a performance decision — SQLite in WAL mode would permit a
//! second process to open this file and write to it — it is the design rule
//! that makes the write path
//! (validate → resolve links → embed → write entity → append revision → append
//! event) atomic from a caller's point of view. Most of those steps have
//! nothing to do with locking, which is the actual argument for the rule.
//!
//! Generating the mirror is **not** one of those steps, though this said it was
//! for a long time. It is a separate command, run deliberately — `keel
//! generate`, or `POST /api/generate` — and it is not part of a write and never
//! was. The distinction matters more than a tidy list: a reader who believes
//! files are rewritten on every write concludes the repository is always
//! current, which is exactly the belief `keel generate --check` and the
//! pre-commit hook exist to correct.
//!
//! The lock is `std::sync::Mutex`, held across synchronous work and never
//! across an `.await`. At one user and a few thousand rows this is correct and
//! obvious; a connection pool or an async mutex would be machinery in search of
//! a measurement (`product/CLAUDE.md`, scale discipline).

pub mod http;
pub mod ratelimit;
pub mod run;
pub mod site;
pub mod state;

pub use http::{TOKEN_HEADER, router};
pub use run::run;
pub use state::AppState;
