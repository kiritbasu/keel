//! Keel's domain core: types, validation, storage and provenance.
//!
//! This crate is deliberately inert. It never opens a network socket, never
//! reads an environment variable, and knows nothing about MCP. Everything it
//! needs is passed in by a caller. That boundary is what makes the CLI, the
//! daemon and any future surface cheap to add — and it is the reason a change
//! to the transport can never quietly change the data model.
//!
//! # Layout
//!
//! - [`entity`] / [`types`] / [`enums`] — the thirteen artifact types and the
//!   closed value sets they carry.
//! - [`id`] — type-prefixed ULIDs.
//! - [`audit`] — the provenance block every row carries.
//! - [`link`] — typed edges and, crucially, their direction.
//! - [`event`] — the append-only mutation log.
//! - [`document`] — prose revisions, which live in Lance rather than DuckDB.
//!
//! # The one thing to read first
//!
//! [`link`]. Graph direction is the most dangerous bug class here, because an
//! inverted traversal returns an empty result that is indistinguishable from a
//! legitimate "nothing is linked". Everything else fails loudly.

pub mod audit;
pub mod document;
pub mod embed;
pub mod entity;
pub mod enums;
pub mod error;
pub mod event;
pub mod id;
pub mod link;
pub mod store;
pub mod types;

pub use audit::{Audit, Provenance};
pub use document::{
    DocStatus, Document, DocumentDiff, EMBEDDING_DIM, EMBEDDING_MODEL, EMBEDDING_VERSION, body_hash,
};
pub use embed::{Embedder, FastEmbedder, HashEmbedder};
pub use entity::{Actor, EntityType, ProjectScope, Surface};
pub use enums::{
    ArtifactKind, DecisionStatus, DesignState, EnvironmentStatus, FeedbackKind, MetricDirection,
    MilestoneKind, MilestoneStatus, ProjectStatus, QuestionKind, QuestionStatus, RiskSeverity,
    Sentiment, SpecKind, SpecStatus, TaskKind, TaskPriority, TaskStatus,
};
pub use error::{Error, Result};
pub use event::{Action, Cursor, Event, NewEvent};
pub use id::{BlobId, DocId, EntityId, EventId, LinkId};
pub use link::{DEFAULT_DEPTH, Direction, Link, MAX_DEPTH, NewLink, Relation};
pub use store::{
    Blob, Created, DocumentStore, DuckStore, EntityQuery, EntityStore, GraphStore, Neighbour, Page,
    SearchHit, SearchQuery, SearchSource,
};
pub use types::{
    Artifact, Decision, Design, Entity, Environment, Feedback, Metric, MetricObservation,
    Milestone, Project, Question, Spec, Task, Term, derive_idempotency_key,
};
