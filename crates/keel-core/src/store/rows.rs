//! Mapping between the thirteen structs and their DuckDB rows.
//!
//! Two rules keep this honest:
//!
//! 1. **Column order is declared once**, in [`TableSpec`], and both the
//!    `SELECT` list and the `INSERT` statement are generated from it. A column
//!    added to one and forgotten in the other is not possible.
//! 2. **Reads address columns by name**, never by index. An offset that drifts
//!    by one produces a row where every field holds its neighbour's value, and
//!    the types are similar enough that it would sometimes even parse.
//!
//! List columns travel as JSON in both directions — `json_extract_string(?,
//! '$[*]')` on the way in, `to_json(col)` on the way out. That was chosen over
//! a delimiter split because repo URLs and labels can contain almost anything,
//! and a separator that is safe today is a corruption bug later.

use crate::{
    Actor, Artifact, ArtifactKind, Audit, BlobId, Decision, DecisionStatus, Design, DesignState,
    Entity, EntityId, EntityType, Environment, EnvironmentStatus, Error, Feedback, FeedbackKind,
    Metric, MetricDirection, MetricObservation, Milestone, MilestoneKind, MilestoneStatus, Project,
    ProjectStatus, Question, QuestionKind, QuestionStatus, Result, RiskSeverity, Sentiment, Spec,
    SpecKind, SpecStatus, Surface, Task, TaskKind, TaskPriority, TaskStatus,
};
use chrono::{DateTime, NaiveDate, Utc};
use duckdb::Row;
use duckdb::types::{TimeUnit, Value};

/// One column in a table.
#[derive(Debug, Clone, Copy)]
pub enum Col {
    /// An ordinary scalar column.
    Plain(&'static str),
    /// A `VARCHAR[]` column, which round-trips as JSON.
    Array(&'static str),
}

impl Col {
    const fn name(self) -> &'static str {
        match self {
            Col::Plain(n) | Col::Array(n) => n,
        }
    }
}

/// The type-specific columns of one table, excluding the audit block.
#[derive(Debug, Clone, Copy)]
pub struct TableSpec {
    /// The table name.
    pub table: &'static str,
    /// The columns, in the order params are supplied.
    pub cols: &'static [Col],
}

/// The audit columns, in a fixed order shared by every entity table.
const AUDIT_COLS: &[&str] = &[
    "created_at",
    "updated_at",
    "version",
    "created_by",
    "updated_by",
    "session_id",
    "surface",
    "archived_at",
];

impl TableSpec {
    /// The `SELECT` list, with list columns rendered as JSON.
    pub fn select_list(&self) -> String {
        let mut parts: Vec<String> = self
            .cols
            .iter()
            .map(|c| match c {
                Col::Plain(n) => (*n).to_owned(),
                Col::Array(n) => format!("to_json({n}) AS {n}"),
            })
            .collect();
        parts.extend(AUDIT_COLS.iter().map(|c| (*c).to_owned()));
        parts.join(", ")
    }

    /// `SELECT … FROM table`.
    pub fn select_from(&self) -> String {
        format!("SELECT {} FROM {}", self.select_list(), self.table)
    }

    /// The `INSERT` statement, with list columns rebuilt from JSON.
    pub fn insert_stmt(&self) -> String {
        let names: Vec<&str> = self
            .cols
            .iter()
            .map(|c| c.name())
            .chain(AUDIT_COLS.iter().copied())
            .collect();
        let placeholders: Vec<String> = self
            .cols
            .iter()
            .map(|c| match c {
                Col::Plain(_) => "?".to_owned(),
                Col::Array(_) => "json_extract_string(?, '$[*]')".to_owned(),
            })
            .chain(AUDIT_COLS.iter().map(|_| "?".to_owned()))
            .collect();
        format!(
            "INSERT INTO {} ({}) VALUES ({})",
            self.table,
            names.join(", "),
            placeholders.join(", ")
        )
    }
}

// --- Binding helpers -----------------------------------------------------

/// Bind a string.
fn s(v: impl Into<String>) -> Value {
    Value::Text(v.into())
}

/// Bind an optional string.
fn os(v: Option<impl Into<String>>) -> Value {
    v.map(|x| Value::Text(x.into())).unwrap_or(Value::Null)
}

/// Bind a timestamp at microsecond precision, per SPEC §3.1.
fn ts(v: DateTime<Utc>) -> Value {
    Value::Timestamp(TimeUnit::Microsecond, v.timestamp_micros())
}

/// Bind an optional timestamp.
fn ots(v: Option<DateTime<Utc>>) -> Value {
    v.map(ts).unwrap_or(Value::Null)
}

/// Bind an optional date.
fn odate(v: Option<NaiveDate>) -> Value {
    v.map(|d| {
        Value::Date32(
            (d - NaiveDate::from_ymd_opt(1970, 1, 1).unwrap_or_default()).num_days() as i32,
        )
    })
    .unwrap_or(Value::Null)
}

/// Bind a list column as JSON text.
fn arr(v: &[String]) -> Value {
    Value::Text(serde_json::to_string(v).unwrap_or_else(|_| "[]".to_owned()))
}

/// Bind an optional i32.
fn oi(v: Option<i32>) -> Value {
    v.map(Value::Int).unwrap_or(Value::Null)
}

/// Bind an optional f64.
fn od(v: Option<f64>) -> Value {
    v.map(Value::Double).unwrap_or(Value::Null)
}

/// The eight audit params, in `AUDIT_COLS` order.
fn audit_params(a: &Audit) -> Vec<Value> {
    vec![
        ts(a.created_at),
        ts(a.updated_at),
        Value::Int(a.version),
        s(a.created_by.as_str()),
        s(a.updated_by.as_str()),
        os(a.session_id.clone()),
        os(a.surface.map(|x| x.as_str())),
        ots(a.archived_at),
    ]
}

// --- Reading helpers -----------------------------------------------------

/// Wrap a DuckDB read failure with the column that caused it.
fn col_err(table: &str, column: &str) -> impl FnOnce(duckdb::Error) -> Error {
    let context = format!("read column `{column}` of `{table}`");
    move |source| Error::Storage { context, source }
}

/// Read a required string column.
fn get_s(row: &Row<'_>, table: &str, col: &str) -> Result<String> {
    row.get::<_, String>(col).map_err(col_err(table, col))
}

/// Read an optional string column.
fn get_os(row: &Row<'_>, table: &str, col: &str) -> Result<Option<String>> {
    row.get::<_, Option<String>>(col)
        .map_err(col_err(table, col))
}

/// Read a required timestamp column.
fn get_ts(row: &Row<'_>, table: &str, col: &str) -> Result<DateTime<Utc>> {
    row.get::<_, DateTime<Utc>>(col)
        .map_err(col_err(table, col))
}

/// Read an optional timestamp column.
fn get_ots(row: &Row<'_>, table: &str, col: &str) -> Result<Option<DateTime<Utc>>> {
    row.get::<_, Option<DateTime<Utc>>>(col)
        .map_err(col_err(table, col))
}

/// Read a required i32 column.
fn get_i(row: &Row<'_>, table: &str, col: &str) -> Result<i32> {
    row.get::<_, i32>(col).map_err(col_err(table, col))
}

/// Read an optional i32 column.
fn get_oi(row: &Row<'_>, table: &str, col: &str) -> Result<Option<i32>> {
    row.get::<_, Option<i32>>(col).map_err(col_err(table, col))
}

/// Read an optional f64 column.
fn get_od(row: &Row<'_>, table: &str, col: &str) -> Result<Option<f64>> {
    row.get::<_, Option<f64>>(col).map_err(col_err(table, col))
}

/// Read a list column back from its JSON rendering.
///
/// A null or unparseable value becomes an empty list rather than an error: a
/// malformed array should not make an otherwise-good row unreadable, and
/// `fsck` is where that gets reported.
fn get_arr(row: &Row<'_>, table: &str, col: &str) -> Result<Vec<String>> {
    let raw = get_os(row, table, col)?;
    Ok(raw
        .and_then(|j| serde_json::from_str::<Vec<String>>(&j).ok())
        .unwrap_or_default())
}

/// Read an entity id column.
fn get_id(row: &Row<'_>, table: &str, col: &str) -> Result<EntityId> {
    EntityId::parse(&get_s(row, table, col)?)
}

/// Read an optional entity id column.
fn get_oid(row: &Row<'_>, table: &str, col: &str) -> Result<Option<EntityId>> {
    match get_os(row, table, col)? {
        Some(v) => Ok(Some(EntityId::parse(&v)?)),
        None => Ok(None),
    }
}

/// Read the audit block.
pub fn read_audit(row: &Row<'_>, table: &str) -> Result<Audit> {
    Ok(Audit {
        created_at: get_ts(row, table, "created_at")?,
        updated_at: get_ts(row, table, "updated_at")?,
        version: get_i(row, table, "version")?,
        created_by: Actor::parse(&get_s(row, table, "created_by")?)?,
        updated_by: Actor::parse(&get_s(row, table, "updated_by")?)?,
        session_id: get_os(row, table, "session_id")?,
        surface: match get_os(row, table, "surface")? {
            Some(v) => Some(Surface::parse(&v)?),
            None => None,
        },
        archived_at: get_ots(row, table, "archived_at")?,
    })
}

// --- Per-type specs ------------------------------------------------------

/// The table spec for a type.
pub fn spec_for(entity_type: EntityType) -> TableSpec {
    use Col::{Array, Plain};
    match entity_type {
        EntityType::Project => TableSpec {
            table: "projects",
            cols: &[
                Plain("id"),
                Plain("slug"),
                Plain("name"),
                Plain("description"),
                Plain("status"),
                Array("repo_urls"),
                Plain("root_path"),
                Array("aliases"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Milestone => TableSpec {
            table: "milestones",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("kind"),
                Plain("name"),
                Plain("summary"),
                Plain("status"),
                Plain("target_date"),
                Plain("shipped_at"),
                Plain("version_string"),
                Plain("sort_order"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Task => TableSpec {
            table: "tasks",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("milestone_id"),
                Plain("kind"),
                Plain("title"),
                Plain("body"),
                Plain("status"),
                Plain("priority"),
                Array("labels"),
                Plain("external_ref"),
                Plain("closed_at"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Spec => TableSpec {
            table: "specs",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("kind"),
                Plain("title"),
                Plain("status"),
                Plain("current_doc_version"),
                Plain("mirror_path"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Decision => TableSpec {
            table: "decisions",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("title"),
                Plain("status"),
                Plain("decided_at"),
                Plain("current_doc_version"),
                Plain("mirror_path"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Question => TableSpec {
            table: "questions",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("kind"),
                Plain("title"),
                Plain("status"),
                Plain("severity"),
                Plain("resolved_at"),
                Plain("current_doc_version"),
                Plain("mirror_path"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Term => TableSpec {
            table: "terms",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("term"),
                Plain("definition"),
                Array("aliases"),
                Plain("mirror_path"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Feedback => TableSpec {
            table: "feedback",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("kind"),
                Plain("source"),
                Plain("contact"),
                Plain("sentiment"),
                Plain("occurred_at"),
                Plain("triaged"),
                Plain("current_doc_version"),
                Plain("summary"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Design => TableSpec {
            table: "design_artifacts",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("name"),
                Plain("state"),
                Plain("figma_ref"),
                Plain("blob_id"),
                Plain("current_doc_version"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Environment => TableSpec {
            table: "environments",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("name"),
                Plain("url"),
                Plain("deployed_version"),
                Plain("deployed_commit"),
                Plain("status"),
                Plain("last_deployed_at"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Metric => TableSpec {
            table: "metrics",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("name"),
                Plain("unit"),
                Plain("target_value"),
                Plain("direction"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::MetricObservation => TableSpec {
            table: "metric_observations",
            cols: &[
                Plain("id"),
                Plain("metric_id"),
                Plain("project_id"),
                Plain("value"),
                Plain("observed_at"),
                Plain("note"),
                Plain("idempotency_key"),
            ],
        },
        EntityType::Artifact => TableSpec {
            table: "artifacts",
            cols: &[
                Plain("id"),
                Plain("project_id"),
                Plain("name"),
                Plain("kind"),
                Plain("url"),
                Plain("blob_id"),
                Plain("idempotency_key"),
            ],
        },
    }
}

/// The insert params for an entity, in `spec_for(..).cols` order followed by
/// the audit block.
pub fn insert_params(entity: &Entity) -> Vec<Value> {
    let mut p: Vec<Value> = match entity {
        Entity::Project(e) => vec![
            s(e.id.as_str()),
            s(&e.slug),
            s(&e.name),
            os(e.description.clone()),
            s(e.status.as_str()),
            arr(&e.repo_urls),
            os(e.root_path.clone()),
            arr(&e.aliases),
            s(&e.idempotency_key),
        ],
        Entity::Milestone(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(e.kind.as_str()),
            s(&e.name),
            os(e.summary.clone()),
            s(e.status.as_str()),
            odate(e.target_date),
            ots(e.shipped_at),
            os(e.version_string.clone()),
            oi(e.sort_order),
            s(&e.idempotency_key),
        ],
        Entity::Task(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            os(e.milestone_id.as_ref().map(EntityId::as_str)),
            s(e.kind.as_str()),
            s(&e.title),
            os(e.body.clone()),
            s(e.status.as_str()),
            s(e.priority.as_str()),
            arr(&e.labels),
            os(e.external_ref.clone()),
            ots(e.closed_at),
            s(&e.idempotency_key),
        ],
        Entity::Spec(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(e.kind.as_str()),
            s(&e.title),
            s(e.status.as_str()),
            Value::Int(e.current_doc_version),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Decision(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(&e.title),
            s(e.status.as_str()),
            ots(e.decided_at),
            Value::Int(e.current_doc_version),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Question(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(e.kind.as_str()),
            s(&e.title),
            s(e.status.as_str()),
            os(e.severity.map(|x| x.as_str())),
            ots(e.resolved_at),
            Value::Int(e.current_doc_version),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Term(e) => vec![
            s(e.id.as_str()),
            os(e.project_id.as_ref().map(EntityId::as_str)),
            s(&e.term),
            s(&e.definition),
            arr(&e.aliases),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Feedback(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(e.kind.as_str()),
            os(e.source.clone()),
            os(e.contact.clone()),
            os(e.sentiment.map(|x| x.as_str())),
            ots(e.occurred_at),
            Value::Boolean(e.triaged),
            Value::Int(e.current_doc_version),
            s(&e.summary),
            s(&e.idempotency_key),
        ],
        Entity::Design(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(&e.name),
            s(e.state.as_str()),
            os(e.figma_ref.clone()),
            os(e.blob_id.as_ref().map(BlobId::as_str)),
            Value::Int(e.current_doc_version),
            s(&e.idempotency_key),
        ],
        Entity::Environment(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(&e.name),
            os(e.url.clone()),
            os(e.deployed_version.clone()),
            os(e.deployed_commit.clone()),
            s(e.status.as_str()),
            ots(e.last_deployed_at),
            s(&e.idempotency_key),
        ],
        Entity::Metric(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(&e.name),
            os(e.unit.clone()),
            od(e.target_value),
            s(e.direction.as_str()),
            s(&e.idempotency_key),
        ],
        Entity::MetricObservation(e) => vec![
            s(e.id.as_str()),
            s(e.metric_id.as_str()),
            s(e.project_id.as_str()),
            Value::Double(e.value),
            ts(e.observed_at),
            os(e.note.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Artifact(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(&e.name),
            s(e.kind.as_str()),
            os(e.url.clone()),
            os(e.blob_id.as_ref().map(BlobId::as_str)),
            s(&e.idempotency_key),
        ],
    };
    p.extend(audit_params(entity.audit()));
    p
}

/// Rebuild an entity from a row produced by `spec_for(t).select_from()`.
pub fn from_row(entity_type: EntityType, row: &Row<'_>) -> Result<Entity> {
    let t = spec_for(entity_type).table;
    let audit = read_audit(row, t)?;

    let entity = match entity_type {
        EntityType::Project => Entity::Project(Project {
            id: get_id(row, t, "id")?,
            slug: get_s(row, t, "slug")?,
            name: get_s(row, t, "name")?,
            description: get_os(row, t, "description")?,
            status: ProjectStatus::parse(&get_s(row, t, "status")?)?,
            repo_urls: get_arr(row, t, "repo_urls")?,
            root_path: get_os(row, t, "root_path")?,
            aliases: get_arr(row, t, "aliases")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Milestone => Entity::Milestone(Milestone {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            kind: MilestoneKind::parse(&get_s(row, t, "kind")?)?,
            name: get_s(row, t, "name")?,
            summary: get_os(row, t, "summary")?,
            status: MilestoneStatus::parse(&get_s(row, t, "status")?)?,
            target_date: row
                .get::<_, Option<NaiveDate>>("target_date")
                .map_err(col_err(t, "target_date"))?,
            shipped_at: get_ots(row, t, "shipped_at")?,
            version_string: get_os(row, t, "version_string")?,
            sort_order: get_oi(row, t, "sort_order")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Task => Entity::Task(Task {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            milestone_id: get_oid(row, t, "milestone_id")?,
            kind: TaskKind::parse(&get_s(row, t, "kind")?)?,
            title: get_s(row, t, "title")?,
            body: get_os(row, t, "body")?,
            status: TaskStatus::parse(&get_s(row, t, "status")?)?,
            priority: match get_os(row, t, "priority")? {
                Some(p) => TaskPriority::parse(&p)?,
                None => TaskPriority::default(),
            },
            labels: get_arr(row, t, "labels")?,
            external_ref: get_os(row, t, "external_ref")?,
            closed_at: get_ots(row, t, "closed_at")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Spec => Entity::Spec(Spec {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            kind: SpecKind::parse(&get_s(row, t, "kind")?)?,
            title: get_s(row, t, "title")?,
            status: SpecStatus::parse(&get_s(row, t, "status")?)?,
            current_doc_version: get_i(row, t, "current_doc_version")?,
            mirror_path: get_os(row, t, "mirror_path")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Decision => Entity::Decision(Decision {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            title: get_s(row, t, "title")?,
            status: DecisionStatus::parse(&get_s(row, t, "status")?)?,
            decided_at: get_ots(row, t, "decided_at")?,
            current_doc_version: get_i(row, t, "current_doc_version")?,
            mirror_path: get_os(row, t, "mirror_path")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Question => Entity::Question(Question {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            kind: QuestionKind::parse(&get_s(row, t, "kind")?)?,
            title: get_s(row, t, "title")?,
            status: QuestionStatus::parse(&get_s(row, t, "status")?)?,
            severity: match get_os(row, t, "severity")? {
                Some(v) => Some(RiskSeverity::parse(&v)?),
                None => None,
            },
            resolved_at: get_ots(row, t, "resolved_at")?,
            current_doc_version: get_i(row, t, "current_doc_version")?,
            mirror_path: get_os(row, t, "mirror_path")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Term => Entity::Term(crate::Term {
            id: get_id(row, t, "id")?,
            project_id: get_oid(row, t, "project_id")?,
            term: get_s(row, t, "term")?,
            definition: get_s(row, t, "definition")?,
            aliases: get_arr(row, t, "aliases")?,
            mirror_path: get_os(row, t, "mirror_path")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Feedback => Entity::Feedback(Feedback {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            kind: FeedbackKind::parse(&get_s(row, t, "kind")?)?,
            source: get_os(row, t, "source")?,
            contact: get_os(row, t, "contact")?,
            sentiment: match get_os(row, t, "sentiment")? {
                Some(v) => Some(Sentiment::parse(&v)?),
                None => None,
            },
            occurred_at: get_ots(row, t, "occurred_at")?,
            triaged: row
                .get::<_, Option<bool>>("triaged")
                .map_err(col_err(t, "triaged"))?
                .unwrap_or(false),
            current_doc_version: get_i(row, t, "current_doc_version")?,
            summary: get_s(row, t, "summary")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Design => Entity::Design(Design {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            name: get_s(row, t, "name")?,
            state: DesignState::parse(&get_s(row, t, "state")?)?,
            figma_ref: get_os(row, t, "figma_ref")?,
            blob_id: match get_os(row, t, "blob_id")? {
                Some(v) => Some(BlobId::parse(&v)?),
                None => None,
            },
            current_doc_version: get_i(row, t, "current_doc_version")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Environment => Entity::Environment(Environment {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            name: get_s(row, t, "name")?,
            url: get_os(row, t, "url")?,
            deployed_version: get_os(row, t, "deployed_version")?,
            deployed_commit: get_os(row, t, "deployed_commit")?,
            status: match get_os(row, t, "status")? {
                Some(v) => EnvironmentStatus::parse(&v)?,
                None => EnvironmentStatus::default(),
            },
            last_deployed_at: get_ots(row, t, "last_deployed_at")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Metric => Entity::Metric(Metric {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            name: get_s(row, t, "name")?,
            unit: get_os(row, t, "unit")?,
            target_value: get_od(row, t, "target_value")?,
            direction: match get_os(row, t, "direction")? {
                Some(v) => MetricDirection::parse(&v)?,
                None => MetricDirection::default(),
            },
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::MetricObservation => Entity::MetricObservation(MetricObservation {
            id: get_id(row, t, "id")?,
            metric_id: get_id(row, t, "metric_id")?,
            project_id: get_id(row, t, "project_id")?,
            value: row.get::<_, f64>("value").map_err(col_err(t, "value"))?,
            observed_at: get_ts(row, t, "observed_at")?,
            note: get_os(row, t, "note")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Artifact => Entity::Artifact(Artifact {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            name: get_s(row, t, "name")?,
            kind: match get_os(row, t, "kind")? {
                Some(v) => ArtifactKind::parse(&v)?,
                None => ArtifactKind::default(),
            },
            url: get_os(row, t, "url")?,
            blob_id: match get_os(row, t, "blob_id")? {
                Some(v) => Some(BlobId::parse(&v)?),
                None => None,
            },
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
    };
    Ok(entity)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn insert_params_match_the_declared_column_count() {
        // The one invariant that a mismatched INSERT would violate. Checked
        // for all thirteen so a column added to `spec_for` without a matching
        // param fails here rather than at runtime with a binding error.
        let p = EntityId::generate(EntityType::Project);
        let m = EntityId::generate(EntityType::Metric);
        let entities: Vec<Entity> = vec![
            Project::new("k", "Keel").into(),
            Milestone::new(p.clone(), "P0").into(),
            Task::new(p.clone(), "t").into(),
            Spec::new(p.clone(), "s").into(),
            Decision::new(p.clone(), "d").into(),
            Question::new(p.clone(), "q").into(),
            crate::Term::new(Some(p.clone()), "t", "d").into(),
            Feedback::new(p.clone(), "f").into(),
            Design::new(p.clone(), "d").into(),
            Environment::new(p.clone(), "prod").into(),
            Metric::new(p.clone(), "m").into(),
            MetricObservation::new(m, p.clone(), 1.0, Utc::now()).into(),
            Artifact::new(p, "a").into(),
        ];
        assert_eq!(entities.len(), 13);

        for e in entities {
            let spec = spec_for(e.entity_type());
            let expected = spec.cols.len() + AUDIT_COLS.len();
            assert_eq!(
                insert_params(&e).len(),
                expected,
                "{} binds the wrong number of params",
                e.entity_type()
            );
            assert_eq!(
                spec.insert_stmt().matches('?').count(),
                expected,
                "{} has the wrong number of placeholders",
                e.entity_type()
            );
        }
    }

    #[test]
    fn array_columns_use_json_on_both_sides() {
        let spec = spec_for(EntityType::Task);
        assert!(spec.select_list().contains("to_json(labels) AS labels"));
        assert!(
            spec.insert_stmt()
                .contains("json_extract_string(?, '$[*]')")
        );
    }

    #[test]
    fn every_table_selects_the_audit_block() {
        for t in EntityType::ALL {
            let list = spec_for(t).select_list();
            for col in AUDIT_COLS {
                assert!(list.contains(col), "{t} does not select `{col}`");
            }
        }
    }

    #[test]
    fn table_names_agree_with_the_entity_type() {
        for t in EntityType::ALL {
            assert_eq!(spec_for(t).table, t.table());
        }
    }
}
