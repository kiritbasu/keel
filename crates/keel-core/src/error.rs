//! Error types.
//!
//! Two audiences read these strings and neither is a human staring at a stack
//! trace. A *model* reads validation errors over MCP and has to work out what
//! to send instead — so `Invalid` carries the field, what was wrong with it,
//! and what would have been accepted, rather than a bare "bad request". A
//! *future maintainer* reads storage errors and needs to know what the caller
//! was attempting, not merely which SQL statement failed — so `Storage` wraps
//! its source with a description of the operation.

use crate::EntityType;

/// The result type every fallible `keel-core` operation returns.
pub type Result<T> = std::result::Result<T, Error>;

/// Everything that can go wrong inside `keel-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A caller-supplied value cannot be stored.
    ///
    /// The three parts are deliberate: `field` says where to look, `problem`
    /// says what is wrong, `expected` says what to send instead. An agent that
    /// reads only the last part should still be able to retry successfully.
    #[error("invalid {entity_type} field `{field}`: {problem}. Expected: {expected}")]
    Invalid {
        /// The type whose field failed validation.
        entity_type: EntityType,
        /// The field name, matching the MCP argument name the caller used.
        field: String,
        /// What was wrong with the supplied value.
        problem: String,
        /// What a valid value looks like. Enumerate the options when there is
        /// a closed set — a model will pick one.
        expected: String,
    },

    /// An identifier was not shaped like a Keel ULID.
    #[error("malformed id `{supplied}`: {problem}. Expected: {expected}")]
    MalformedId {
        /// The identifier as supplied.
        supplied: String,
        /// Why it could not be parsed.
        problem: String,
        /// The shape a valid identifier takes.
        expected: String,
    },

    /// No live entity exists with that identifier.
    ///
    /// Archived entities are *found*, not missing — soft delete means the row
    /// is still there. Callers that must exclude archived rows check
    /// `archived_at` themselves, so that "it was archived" stays
    /// distinguishable from "it never existed".
    #[error("no {entity_type} with id `{id}`")]
    NotFound {
        /// The type that was searched.
        entity_type: EntityType,
        /// The identifier that matched nothing.
        id: String,
    },

    /// An update carried a `version` that is no longer current.
    ///
    /// This is not a failure so much as a merge request. The daemon turns it
    /// into the 409 payload from SPEC §7.3, attaching the current state and
    /// the events since the caller's read so the agent can usually resolve it
    /// without asking anyone.
    #[error(
        "stale update to {entity_type} `{id}`: you supplied version {supplied}, \
         but the current version is {latest}. Re-read the entity and retry."
    )]
    StaleVersion {
        /// The type being updated.
        entity_type: EntityType,
        /// The entity being updated.
        id: String,
        /// The version the caller believed was current.
        supplied: i32,
        /// The version that actually is current.
        latest: i32,
    },

    /// A write would have broken an invariant that spans the two engines, or
    /// one the schema cannot express.
    ///
    /// Referential integrity is application-level here (SPEC §3.1): `links` is
    /// polymorphic across thirteen tables and `documents` lives in Lance where
    /// DuckDB cannot see it. This is the error that stands in for the foreign
    /// key that cannot be declared.
    #[error("{operation} would break an invariant: {problem}")]
    Invariant {
        /// What the caller was trying to do.
        operation: String,
        /// The invariant that would have been violated.
        problem: String,
    },

    /// An accepted decision was edited rather than superseded.
    ///
    /// Called out separately from `Invariant` because the remedy is specific
    /// and an agent should be told it: create a new decision and link it with
    /// `supersedes`.
    #[error(
        "decision `{id}` is accepted and its content is immutable. \
         Create a new decision and link it to this one with rel `supersedes`."
    )]
    DecisionImmutable {
        /// The accepted decision that was targeted.
        id: String,
    },

    /// The storage engine failed.
    #[error("{context}")]
    Storage {
        /// What the caller was attempting, in the imperative — "create task",
        /// "traverse links outbound from tsk_01H8…". Never just the SQL.
        context: String,
        /// The underlying engine error.
        #[source]
        source: duckdb::Error,
    },

    /// Reading or writing the store's directory failed.
    #[error("{context}")]
    Io {
        /// What the caller was attempting.
        context: String,
        /// The underlying filesystem error.
        #[source]
        source: std::io::Error,
    },

    /// A JSON value could not be encoded or decoded.
    #[error("{context}")]
    Json {
        /// What the caller was attempting.
        context: String,
        /// The underlying serialisation error.
        #[source]
        source: serde_json::Error,
    },

    /// The embedding model could not be loaded or could not embed the input.
    ///
    /// Kept as a distinct variant because the first-run failure mode — model
    /// download, no network — is recoverable and looks nothing like a bug.
    #[error("{context}: {reason}")]
    Embedding {
        /// What the caller was attempting.
        context: String,
        /// The reason, as reported by the embedding backend. A plain string
        /// rather than a `#[source]` because `fastembed` surfaces failures as
        /// `anyhow::Error`, which `keel-core` must not depend on.
        reason: String,
    },
}

impl Error {
    /// Wrap a DuckDB error with what the caller was trying to do.
    ///
    /// Exists so call sites read `.map_err(Error::storage("create task"))`
    /// rather than building the struct inline thirty times.
    pub fn storage(context: impl Into<String>) -> impl FnOnce(duckdb::Error) -> Self {
        let context = context.into();
        move |source| Error::Storage { context, source }
    }

    /// Wrap an I/O error with what the caller was trying to do.
    pub fn io(context: impl Into<String>) -> impl FnOnce(std::io::Error) -> Self {
        let context = context.into();
        move |source| Error::Io { context, source }
    }

    /// Wrap a serialisation error with what the caller was trying to do.
    pub fn json(context: impl Into<String>) -> impl FnOnce(serde_json::Error) -> Self {
        let context = context.into();
        move |source| Error::Json { context, source }
    }

    /// Build a validation error for a field of `entity_type`.
    pub fn invalid(
        entity_type: EntityType,
        field: impl Into<String>,
        problem: impl Into<String>,
        expected: impl Into<String>,
    ) -> Self {
        Error::Invalid {
            entity_type,
            field: field.into(),
            problem: problem.into(),
            expected: expected.into(),
        }
    }

    /// Whether this error means "retry with fresher state", which the MCP
    /// layer renders as 409 rather than 400.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::StaleVersion { .. })
    }

    /// Whether this error is the caller's fault — a 4xx rather than a 5xx.
    ///
    /// The daemon needs this to choose a status code, and getting it wrong in
    /// the lenient direction means agents retry things that will never work.
    pub fn is_caller_error(&self) -> bool {
        matches!(
            self,
            Error::Invalid { .. }
                | Error::MalformedId { .. }
                | Error::NotFound { .. }
                | Error::StaleVersion { .. }
                | Error::Invariant { .. }
                | Error::DecisionImmutable { .. }
        )
    }
}
