//! Mapping between the thirteen structs and their SQLite rows.
//!
//! The column lists themselves — [`TableSpec`], [`Col`] and [`spec_for`] — live
//! here, and there is exactly one of each. Thirteen column orders maintained in
//! two places would be thirteen chances for the copies to disagree about what a
//! row is, and the disagreement would only ever show up as data sitting in the
//! wrong fields.
//!
//! Two rules keep the mapping honest:
//!
//! 1. **Column order is declared once**, in [`TableSpec`], and both the
//!    `SELECT` list and the `INSERT` statement are generated from it. A column
//!    added to one and forgotten in the other is not possible.
//! 2. **Reads address columns by name**, never by index. An offset that drifts
//!    by one produces a row where every field holds its neighbour's value, and
//!    the types are similar enough that it would sometimes even parse.
//!
//! # Lists and timestamps
//!
//! **Lists.** SQLite has no array type, so a list column simply *is* JSON text.
//! That makes the select list plain column names and the insert plain
//! placeholders — the conversion happens in Rust, at [`arr`] and [`get_arr`],
//! where it can be tested without a database.
//!
//! **Timestamps.** These are TEXT, and the format is load-bearing rather than
//! cosmetic. See [`TIMESTAMP_FORMAT`].

use crate::{
    Actor, Artifact, ArtifactKind, Audit, BlobId, CloseReason, Decision, DecisionStatus, Design,
    DesignState, Entity, EntityId, EntityType, Environment, EnvironmentStatus, Error, Feedback,
    FeedbackKind, Link, LinkId, Metric, MetricDirection, MetricObservation, Milestone,
    MilestoneKind, MilestoneStatus, Project, ProjectStatus, Question, QuestionKind, QuestionStatus,
    Relation, Result, RiskSeverity, Sentiment, Spec, SpecKind, SpecStatus, Surface, Task, TaskKind,
    TaskPriority, TaskStatus,
};
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::Row;
use rusqlite::types::Value;

/// One column in a table.
#[derive(Debug, Clone, Copy)]
pub enum Col {
    /// An ordinary scalar column.
    Plain(&'static str),
    /// A list column, stored and read back as JSON text.
    Array(&'static str),
}

/// The type-specific columns of one table, excluding the audit block.
#[derive(Debug, Clone, Copy)]
pub struct TableSpec {
    /// The table name.
    pub table: &'static str,
    /// The columns, in the order params are supplied.
    pub cols: &'static [Col],
}

/// How a `DateTime<Utc>` is spelled in a TEXT column.
///
/// **The fixed six fractional digits are the whole point.** Every `ORDER BY
/// created_at` in this codebase is a string comparison once the column is TEXT,
/// and it is only correct while lexicographic order matches chronological
/// order. Chrono's variable-width `%f` breaks that: `…:36.5Z` sorts *after*
/// `…:36.524Z`, because the comparison reaches `Z` against `2` and `Z` is the
/// larger byte. The activity feed would show the later event first, the digest
/// would page over a gap, and nothing would look broken.
///
/// Six digits and not nine because that is the precision SPEC §3.1 stores, and
/// the precision of the rows the earlier DuckDB store held — which is what is
/// in these files. Nine would invent three zeroes and imply an accuracy the
/// source never had.
pub(super) const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%.6fZ";

/// How a `NaiveDate` is spelled in a TEXT column.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// The audit columns, in a fixed order shared by every entity table.
///
/// The order is the contract, not the names. It is asserted against the
/// parameter count for all thirteen types in the tests below, so a divergence
/// fails there rather than silently binding `updated_by` into `created_by`.
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

/// The column name behind either variant.
///
/// One match, so that nothing else in the file has to destructure a [`Col`]
/// just to learn what the column is called.
const fn col_name(col: Col) -> &'static str {
    match col {
        Col::Plain(n) | Col::Array(n) => n,
    }
}

/// The `SELECT` list: the type's columns, then the audit block.
///
/// Bare column names throughout, including the list columns — under SQLite they
/// already hold JSON text, so there is nothing for the engine to convert.
pub fn select_list(spec: &TableSpec) -> String {
    spec.cols
        .iter()
        .map(|c| col_name(*c))
        .chain(AUDIT_COLS.iter().copied())
        .collect::<Vec<&str>>()
        .join(", ")
}

/// `SELECT … FROM table`, the statement every read starts from.
pub fn select_from(spec: &TableSpec) -> String {
    format!("SELECT {} FROM {}", select_list(spec), spec.table)
}

/// The `INSERT` statement, with numbered placeholders.
///
/// `?1..?N` rather than bare `?` because a numbered placeholder makes the
/// binding order visible in the statement itself — the same reason the read
/// side addresses columns by name.
pub fn insert_stmt(spec: &TableSpec) -> String {
    let names: Vec<&str> = spec
        .cols
        .iter()
        .map(|c| col_name(*c))
        .chain(AUDIT_COLS.iter().copied())
        .collect();
    let placeholders: Vec<String> = (1..=names.len()).map(|i| format!("?{i}")).collect();
    format!(
        "INSERT INTO {} ({}) VALUES ({})",
        spec.table,
        names.join(", "),
        placeholders.join(", ")
    )
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

/// Now, in the one format this store sorts correctly.
///
/// Exists so that a field written by hand — `shipped_at`, set alongside a
/// status — is spelled the same way as one written by the binding helpers. A
/// timestamp in a different shape sorts wrongly against its neighbours, which
/// is the whole argument in [`TIMESTAMP_FORMAT`].
pub(crate) fn now_rfc3339() -> String {
    Utc::now().format(TIMESTAMP_FORMAT).to_string()
}

/// Bind a timestamp in the one format this store sorts correctly.
pub(super) fn ts(v: DateTime<Utc>) -> Value {
    Value::Text(v.format(TIMESTAMP_FORMAT).to_string())
}

/// Bind an optional timestamp.
pub(super) fn ots(v: Option<DateTime<Utc>>) -> Value {
    v.map(ts).unwrap_or(Value::Null)
}

/// Bind an optional date.
fn odate(v: Option<NaiveDate>) -> Value {
    v.map(|d| Value::Text(d.format(DATE_FORMAT).to_string()))
        .unwrap_or(Value::Null)
}

/// Bind a list column as JSON text.
///
/// JSON rather than a delimited string because repo URLs and labels can contain
/// almost anything, and a separator that is safe today is a corruption bug
/// later. Serialising a `Vec<String>` cannot actually fail, and an empty array
/// is the only sensible thing to store if it somehow did.
fn arr(v: &[String]) -> Value {
    Value::Text(serde_json::to_string(v).unwrap_or_else(|_| "[]".to_owned()))
}

/// Bind an i32 into SQLite's single 64-bit integer type.
fn i(v: i32) -> Value {
    Value::Integer(i64::from(v))
}

/// Bind an optional i32.
fn oi(v: Option<i32>) -> Value {
    v.map(i).unwrap_or(Value::Null)
}

/// Bind an f64.
fn d(v: f64) -> Value {
    Value::Real(v)
}

/// Bind an optional f64.
fn od(v: Option<f64>) -> Value {
    v.map(d).unwrap_or(Value::Null)
}

/// Bind a boolean. SQLite has no boolean type; 0 and 1 is the convention the
/// engine's own `true` and `false` literals compile to.
fn b(v: bool) -> Value {
    Value::Integer(i64::from(v))
}

/// The eight audit params, in `AUDIT_COLS` order.
fn audit_params(a: &Audit) -> Vec<Value> {
    vec![
        ts(a.created_at),
        ts(a.updated_at),
        i(a.version),
        s(a.created_by.as_str()),
        s(a.updated_by.as_str()),
        os(a.session_id.clone()),
        os(a.surface.map(|x| x.as_str())),
        ots(a.archived_at),
    ]
}

// --- Reading helpers -----------------------------------------------------

/// Wrap a SQLite read failure with the column that caused it.
///
/// A bare `rusqlite::Error` says "invalid column type" and nothing about which
/// column or which table, which is unactionable in a store with thirteen of
/// them.
pub(super) fn col_err(table: &str, column: &str) -> impl FnOnce(rusqlite::Error) -> Error {
    let context = format!("read column `{column}` of `{table}`");
    move |source| Error::Storage { context, source }
}

/// Report a stored value that the engine handed over intact but that this
/// module cannot make sense of.
///
/// [`Error::Invariant`] rather than [`Error::Storage`] because there is no
/// `rusqlite::Error` to wrap — SQLite did its job and returned the text. What
/// failed is Specline's own rule about what may be in that column, which is exactly
/// what `Invariant` is for.
fn malformed(table: &str, column: &str, raw: &str, expected: &str) -> Error {
    Error::Invariant {
        operation: format!("read column `{column}` of `{table}`"),
        problem: format!("`{raw}` is not {expected}"),
    }
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

/// Turn stored text into a timestamp.
///
/// Accepts more than it writes, and the leniency has one named cause: the
/// migration that moved these stores off DuckDB copied timestamps in DuckDB's
/// own rendering — RFC 3339 with a variable-width fraction, and sometimes a
/// space in place of the `T`. That migration has been and gone, but the values
/// it wrote are sitting in real stores on disk today, so refusing them would
/// make those rows unreadable. Everything Specline writes itself is
/// [`TIMESTAMP_FORMAT`].
pub(crate) fn parse_ts(table: &str, col: &str, raw: &str) -> Result<DateTime<Utc>> {
    if let Ok(v) = DateTime::parse_from_rfc3339(raw) {
        return Ok(v.with_timezone(&Utc));
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(v) = NaiveDateTime::parse_from_str(raw, format) {
            return Ok(v.and_utc());
        }
    }
    Err(malformed(
        table,
        col,
        raw,
        "a UTC timestamp of the form 2026-08-11T09:14:36.524000Z",
    ))
}

/// Read a required timestamp column.
pub(super) fn get_ts(row: &Row<'_>, table: &str, col: &str) -> Result<DateTime<Utc>> {
    parse_ts(table, col, &get_s(row, table, col)?)
}

/// Read an optional timestamp column.
pub(super) fn get_ots(row: &Row<'_>, table: &str, col: &str) -> Result<Option<DateTime<Utc>>> {
    match get_os(row, table, col)? {
        Some(raw) => Ok(Some(parse_ts(table, col, &raw)?)),
        None => Ok(None),
    }
}

/// Read an optional date column.
///
/// A full timestamp is accepted and truncated for the same reason [`parse_ts`]
/// is lenient: the migration off DuckDB rendered some `DATE` values as
/// timestamps, and those rows are still here.
fn get_odate(row: &Row<'_>, table: &str, col: &str) -> Result<Option<NaiveDate>> {
    let Some(raw) = get_os(row, table, col)? else {
        return Ok(None);
    };
    if let Ok(v) = NaiveDate::parse_from_str(&raw, DATE_FORMAT) {
        return Ok(Some(v));
    }
    if let Ok(v) = parse_ts(table, col, &raw) {
        return Ok(Some(v.date_naive()));
    }
    Err(malformed(table, col, &raw, "a date of the form 2026-08-11"))
}

/// Read a required i32 column.
///
/// SQLite hands back an `i64` from any INTEGER column, so the narrowing happens
/// here rather than at thirteen call sites. A value that does not fit is a
/// corrupted row, not a number to wrap around.
fn get_i(row: &Row<'_>, table: &str, col: &str) -> Result<i32> {
    let wide: i64 = row.get::<_, i64>(col).map_err(col_err(table, col))?;
    i32::try_from(wide).map_err(|_| malformed(table, col, &wide.to_string(), "a 32-bit integer"))
}

/// Read an optional i32 column.
fn get_oi(row: &Row<'_>, table: &str, col: &str) -> Result<Option<i32>> {
    let Some(wide) = row
        .get::<_, Option<i64>>(col)
        .map_err(col_err(table, col))?
    else {
        return Ok(None);
    };
    i32::try_from(wide)
        .map(Some)
        .map_err(|_| malformed(table, col, &wide.to_string(), "a 32-bit integer"))
}

/// Read a readable-identifier number, treating NULL as "not yet assigned".
///
/// Deliberately lenient, and the leniency is the point — it was learned in the
/// DuckDB reader this one replaced, where a single NULL `number` once made
/// *every* row of that type unreadable in a project, including the idempotency
/// lookup, so
/// `specline_create` failed too. A migration adds the column before the binary that
/// populates it can be running, so there is always a window in which a writer
/// inserts a row without one. A single unnumbered row should cost that row's
/// label, not the whole table.
///
/// Zero already means "not yet assigned" everywhere else in the codebase, and
/// the write paths assign a real number to anything holding it.
fn get_number(row: &Row<'_>, table: &str, col: &str) -> Result<i32> {
    Ok(get_oi(row, table, col)?.unwrap_or(0))
}

/// Read a required f64 column.
fn get_d(row: &Row<'_>, table: &str, col: &str) -> Result<f64> {
    row.get::<_, f64>(col).map_err(col_err(table, col))
}

/// Read an optional f64 column.
fn get_od(row: &Row<'_>, table: &str, col: &str) -> Result<Option<f64>> {
    row.get::<_, Option<f64>>(col).map_err(col_err(table, col))
}

/// Read a boolean stored as an integer.
fn get_ob(row: &Row<'_>, table: &str, col: &str) -> Result<Option<bool>> {
    Ok(row
        .get::<_, Option<i64>>(col)
        .map_err(col_err(table, col))?
        .map(|v| v != 0))
}

/// Read a list column back from its JSON text.
///
/// A NULL, an empty string or unparseable JSON all become an empty list rather
/// than an error. Two of those are real: rows written before a list column
/// existed hold NULL, and a hand-run `specline import` can leave an empty string. A
/// malformed array should not make an otherwise-good row unreadable, and `fsck`
/// is where that gets reported.
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

/// Read the audit block that every entity table carries.
///
/// Public because the link, note and event readers need the same eight columns
/// without going through [`from_row`], which only knows about the thirteen
/// entity types.
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

/// Every column of `links`, in the order [`read_link`] expects.
///
/// One string rather than three, because the reader and the query have to agree
/// and there was nothing making them: the list and the mapping were both
/// hand-copied into two modules, so adding a link column meant four edits and a
/// miss failed at runtime on whichever path had been forgotten. A `SELECT *`
/// would have the same effect and hide the ordering, which is worth being able
/// to read.
pub const LINK_COLUMNS: &str = "SELECT id, project_id, from_type, from_id, rel, to_type, to_id, \
     anchor, note, created_at, updated_at, version, created_by, updated_by, session_id, surface, \
     archived_at FROM links";

/// Rebuild an edge from a `links` row.
///
/// Lives here rather than beside either of its callers because both of them
/// needed it and each had grown its own copy — identical in behaviour, and
/// differing only in how they spelt the error, which is exactly the kind of
/// duplication that stays in step right up until it does not.
pub fn read_link(row: &Row<'_>) -> Result<Link> {
    Ok(Link {
        id: LinkId::parse(&get_s(row, "links", "id")?)?,
        project_id: get_oid(row, "links", "project_id")?,
        from_type: EntityType::parse(&get_s(row, "links", "from_type")?)?,
        from_id: EntityId::parse(&get_s(row, "links", "from_id")?)?,
        rel: Relation::parse(&get_s(row, "links", "rel")?)?,
        to_type: EntityType::parse(&get_s(row, "links", "to_type")?)?,
        to_id: EntityId::parse(&get_s(row, "links", "to_id")?)?,
        anchor: get_os(row, "links", "anchor")?.unwrap_or_default(),
        note: get_os(row, "links", "note")?,
        audit: read_audit(row, "links")?,
    })
}

/// The insert params for an entity, in `spec_for(..).cols` order followed by
/// the audit block.
///
/// The ordering is the contract: a value out of place binds into its
/// neighbour's column, and where the two are both TEXT nothing rejects it. The
/// test below checks the count for all thirteen types, which catches the case
/// that actually happens — a column added to `spec_for` and forgotten here.
pub fn insert_params(entity: &Entity) -> Vec<Value> {
    let mut p: Vec<Value> = match entity {
        Entity::Project(e) => vec![
            s(e.id.as_str()),
            s(&e.slug),
            s(&e.key),
            s(&e.name),
            os(e.description.clone()),
            s(e.status.as_str()),
            arr(&e.repo_urls),
            os(e.root_path.clone()),
            os(e.status_path.clone()),
            os(e.decisions_path.clone()),
            arr(&e.aliases),
            os(e.milestone_noun.clone()),
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
            i(e.number),
            os(e.milestone_id.as_ref().map(EntityId::as_str)),
            s(e.kind.as_str()),
            s(&e.title),
            os(e.body.clone()),
            os(e.summary.clone()),
            s(e.status.as_str()),
            s(e.priority.as_str()),
            arr(&e.labels),
            arr(&e.external_refs),
            os(e.parent_id.as_ref().map(EntityId::as_str)),
            d(e.rank),
            ots(e.closed_at),
            os(e.claimed_by.clone()),
            ots(e.claimed_at),
            os(e.close_reason.map(|r| r.as_str())),
            os(e.close_message.clone()),
            arr(&e.evidence),
            s(&e.idempotency_key),
        ],
        Entity::Spec(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            s(e.kind.as_str()),
            s(&e.title),
            s(e.status.as_str()),
            i(e.current_doc_version),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Decision(e) => vec![
            s(e.id.as_str()),
            s(e.project_id.as_str()),
            i(e.number),
            s(&e.title),
            s(e.status.as_str()),
            ots(e.decided_at),
            i(e.current_doc_version),
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
            i(e.current_doc_version),
            os(e.mirror_path.clone()),
            s(&e.idempotency_key),
        ],
        Entity::Term(e) => vec![
            s(e.id.as_str()),
            os(e.project_id.as_ref().map(EntityId::as_str)),
            s(&e.term),
            s(&e.definition),
            arr(&e.aliases),
            os(e.means.map(|t| t.as_str())),
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
            b(e.triaged),
            i(e.current_doc_version),
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
            i(e.current_doc_version),
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
            d(e.value),
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

/// Rebuild an entity from a row produced by [`select_from`].
///
/// Every field is addressed by name, so a `SELECT` that gains or loses a column
/// changes nothing here — which is the property that makes the select list
/// generated rather than written out.
pub fn from_row(entity_type: EntityType, row: &Row<'_>) -> Result<Entity> {
    let t = spec_for(entity_type).table;
    let audit = read_audit(row, t)?;

    let entity = match entity_type {
        EntityType::Project => Entity::Project(Project {
            id: get_id(row, t, "id")?,
            slug: get_s(row, t, "slug")?,
            key: get_s(row, t, "key")?,
            name: get_s(row, t, "name")?,
            description: get_os(row, t, "description")?,
            status: ProjectStatus::parse(&get_s(row, t, "status")?)?,
            repo_urls: get_arr(row, t, "repo_urls")?,
            root_path: get_os(row, t, "root_path")?,
            status_path: get_os(row, t, "status_path")?,
            decisions_path: get_os(row, t, "decisions_path")?,
            aliases: get_arr(row, t, "aliases")?,
            milestone_noun: get_os(row, t, "milestone_noun")?,
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
            target_date: get_odate(row, t, "target_date")?,
            shipped_at: get_ots(row, t, "shipped_at")?,
            version_string: get_os(row, t, "version_string")?,
            sort_order: get_oi(row, t, "sort_order")?,
            idempotency_key: get_s(row, t, "idempotency_key")?,
            audit,
        }),
        EntityType::Task => Entity::Task(Task {
            id: get_id(row, t, "id")?,
            project_id: get_id(row, t, "project_id")?,
            number: get_number(row, t, "number")?,
            milestone_id: get_oid(row, t, "milestone_id")?,
            kind: TaskKind::parse(&get_s(row, t, "kind")?)?,
            title: get_s(row, t, "title")?,
            body: get_os(row, t, "body")?,
            summary: get_os(row, t, "summary")?,
            status: TaskStatus::parse(&get_s(row, t, "status")?)?,
            priority: match get_os(row, t, "priority")? {
                Some(p) => TaskPriority::parse(&p)?,
                None => TaskPriority::default(),
            },
            labels: get_arr(row, t, "labels")?,
            external_refs: get_arr(row, t, "external_refs")?,
            parent_id: get_oid(row, t, "parent_id")?,
            // A row with no rank sorts as if it were first rather than
            // failing the read; `fsck` reports it, because the store never
            // writes one.
            rank: get_od(row, t, "rank")?.unwrap_or_default(),
            closed_at: get_ots(row, t, "closed_at")?,
            claimed_by: get_os(row, t, "claimed_by")?,
            claimed_at: get_ots(row, t, "claimed_at")?,
            close_reason: match get_os(row, t, "close_reason")? {
                Some(r) => Some(CloseReason::parse(&r)?),
                None => None,
            },
            close_message: get_os(row, t, "close_message")?,
            evidence: get_arr(row, t, "evidence")?,
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
            number: get_number(row, t, "number")?,
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
            means: match get_os(row, t, "means")? {
                Some(name) => Some(EntityType::parse(&name)?),
                None => None,
            },
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
            triaged: get_ob(row, t, "triaged")?.unwrap_or(false),
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
            value: get_d(row, t, "value")?,
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
                Plain("key"),
                Plain("name"),
                Plain("description"),
                Plain("status"),
                Array("repo_urls"),
                Plain("root_path"),
                Plain("status_path"),
                Plain("decisions_path"),
                Array("aliases"),
                Plain("milestone_noun"),
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
                Plain("number"),
                Plain("milestone_id"),
                Plain("kind"),
                Plain("title"),
                Plain("body"),
                Plain("summary"),
                Plain("status"),
                Plain("priority"),
                Array("labels"),
                Array("external_refs"),
                Plain("parent_id"),
                Plain("rank"),
                Plain("closed_at"),
                Plain("claimed_by"),
                Plain("claimed_at"),
                Plain("close_reason"),
                Plain("close_message"),
                Array("evidence"),
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
                Plain("number"),
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
                Plain("means"),
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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::store::schema;
    use rusqlite::Connection;

    /// A connection holding the whole schema and nothing else.
    ///
    /// Built from the migration rather than from `Store` so these tests
    /// exercise the mapping and not the store's opening sequence, which keeps a
    /// failure here about the mapping and nothing else.
    fn schema_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(schema::migrations()[0].sql).unwrap();
        conn
    }

    /// Insert an entity and read it back through the generated statements.
    fn round_trip(conn: &Connection, entity: &Entity) -> Result<Entity> {
        let spec = spec_for(entity.entity_type());
        let params = insert_params(entity);
        conn.execute(&insert_stmt(&spec), rusqlite::params_from_iter(params))
            .unwrap();
        conn.query_row(&select_from(&spec), [], |row| {
            Ok(from_row(entity.entity_type(), row))
        })
        .unwrap()
    }

    /// Timestamps whose sub-second digits SQLite can hold exactly.
    ///
    /// `Utc::now()` is nanosecond-precise and the column is not, so a round trip
    /// of an un-truncated timestamp fails for a reason that has nothing to do
    /// with the mapping under test.
    fn stored_now() -> DateTime<Utc> {
        DateTime::from_timestamp_micros(Utc::now().timestamp_micros()).unwrap_or_default()
    }

    /// Give an entity an audit block a round trip can compare against.
    fn settle(audit: &mut Audit) {
        let now = stored_now();
        audit.created_at = now;
        audit.updated_at = now;
    }

    #[test]
    fn insert_params_match_the_declared_column_count() {
        // The one invariant a mismatched INSERT would violate, and it is
        // invisible until a read: a missing param shifts every later value one
        // column to the left, and where both are TEXT nothing rejects it.
        let p = EntityId::generate(EntityType::Project);
        let m = EntityId::generate(EntityType::Metric);
        let entities: Vec<Entity> = vec![
            Project::new("k", "Specline").into(),
            Milestone::new(p.clone(), "P0", "The first phase, for a store test.").into(),
            Task::new(p.clone(), "t", "A row this test needs in the store.").into(),
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
                insert_stmt(&spec).matches('?').count(),
                expected,
                "{} has the wrong number of placeholders",
                e.entity_type()
            );
        }
    }

    #[test]
    fn every_table_selects_the_audit_block() {
        for t in EntityType::ALL {
            let list = select_list(&spec_for(t));
            for col in AUDIT_COLS {
                assert!(list.contains(col), "{t} does not select `{col}`");
            }
        }
    }

    /// The column is already JSON text, so the select list needs no `to_json`
    /// wrapper and the insert no `json_extract_string`. Asserting the absence
    /// is worth a test: those wrappers are how the DuckDB store handled list
    /// columns, and reintroducing one produces a statement that fails only at
    /// runtime, and only for the five types that have a list column.
    #[test]
    fn list_columns_are_selected_bare() {
        let list = select_list(&spec_for(EntityType::Task));
        assert!(list.contains("labels"));
        assert!(!list.contains("to_json"));
        assert!(!insert_stmt(&spec_for(EntityType::Task)).contains("json_extract_string"));
    }

    /// The reason [`TIMESTAMP_FORMAT`] pins six fractional digits. With a
    /// variable-width fraction the earlier timestamp renders as `…36.5Z` and
    /// the later as `…36.524Z`, and `Z` is a larger byte than `2` — so the
    /// string comparison every `ORDER BY created_at` now performs puts them the
    /// wrong way round.
    #[test]
    fn timestamps_sort_the_same_way_as_strings() {
        let earlier = DateTime::from_timestamp_micros(1_775_000_000_500_000).unwrap();
        let later = DateTime::from_timestamp_micros(1_775_000_000_500_001).unwrap();
        assert!(earlier < later, "the fixture is backwards");

        let a = earlier.format(TIMESTAMP_FORMAT).to_string();
        let b = later.format(TIMESTAMP_FORMAT).to_string();
        assert!(a < b, "{a} should sort before {b}");
        assert_eq!(a.len(), b.len(), "the format must be fixed width");

        // And the pair the naive format actually gets wrong, so the test fails
        // if someone "simplifies" the format string back to `%.f`.
        let naive_a = earlier.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string();
        let naive_b = later.format("%Y-%m-%dT%H:%M:%S%.fZ").to_string();
        assert!(
            naive_a > naive_b,
            "the variable-width format is supposed to be the broken one"
        );
    }

    #[test]
    fn timestamps_survive_a_round_trip_through_text() {
        let now = stored_now();
        let Value::Text(raw) = ts(now) else {
            panic!("a timestamp must bind as text");
        };
        assert_eq!(parse_ts("tasks", "created_at", &raw).unwrap(), now);
    }

    /// The migration off DuckDB wrote rows in DuckDB's rendering and they are
    /// still in the stores on disk, so the reader has to accept a plain RFC
    /// 3339 string as well as the one this store writes.
    #[test]
    fn migrated_timestamps_are_accepted() {
        for raw in [
            "2026-08-11T09:14:36Z",
            "2026-08-11T09:14:36.524Z",
            "2026-08-11T09:14:36+00:00",
            "2026-08-11 09:14:36.524",
        ] {
            assert!(
                parse_ts("tasks", "created_at", raw).is_ok(),
                "{raw} should have parsed"
            );
        }
    }

    #[test]
    fn a_timestamp_that_is_not_one_names_its_column() {
        let err = parse_ts("tasks", "closed_at", "last thursday").unwrap_err();
        let message = err.to_string();
        assert!(message.contains("closed_at"), "unhelpful error: {message}");
        assert!(message.contains("tasks"), "unhelpful error: {message}");
    }

    /// Labels and repo URLs can contain a comma, a quote, or both — which is
    /// why these columns are JSON and not a delimited string. The empty case is
    /// here too because it is the one that used to come back as `[""]`.
    #[test]
    fn list_columns_round_trip_through_json() {
        let conn = schema_conn();

        let mut project = Project::new("awkward", "Awkward");
        settle(&mut project.audit);
        project.repo_urls = vec![
            "https://example.com/a,b".to_owned(),
            "he said \"no\"".to_owned(),
            String::new(),
        ];
        project.aliases = Vec::new();

        let back = round_trip(&conn, &project.clone().into()).unwrap();
        let Entity::Project(back) = back else {
            panic!("a project came back as something else");
        };
        assert_eq!(back.repo_urls, project.repo_urls);
        assert!(back.aliases.is_empty(), "an empty list must stay empty");
    }

    /// A NULL list column is what every row written before that column existed
    /// holds, and an empty string is what a hand-run import can leave. Neither
    /// may make the row unreadable.
    #[test]
    fn a_null_or_empty_list_column_reads_as_empty() {
        let conn = schema_conn();
        let mut project = Project::new("bare", "Bare");
        settle(&mut project.audit);
        let entity: Entity = project.into();
        round_trip(&conn, &entity).unwrap();

        for stored in [Value::Null, Value::Text(String::new())] {
            conn.execute("UPDATE projects SET repo_urls = ?1, aliases = ?1", [stored])
                .unwrap();
            let spec = spec_for(EntityType::Project);
            let back = conn
                .query_row(&select_from(&spec), [], |row| {
                    Ok(from_row(EntityType::Project, row))
                })
                .unwrap()
                .unwrap();
            let Entity::Project(back) = back else {
                panic!("a project came back as something else");
            };
            assert!(back.repo_urls.is_empty());
            assert!(back.aliases.is_empty());
        }
    }

    #[test]
    fn a_project_round_trips() {
        let conn = schema_conn();
        let mut project = Project::new("specline", "Specline");
        settle(&mut project.audit);
        project.description = Some("The store this test is testing.".to_owned());
        project.milestone_noun = Some("phase".to_owned());

        let entity: Entity = project.into();
        assert_eq!(round_trip(&conn, &entity).unwrap(), entity);
    }

    #[test]
    fn a_task_round_trips() {
        let conn = schema_conn();
        let project = EntityId::generate(EntityType::Project);
        let mut task = Task::new(
            project,
            "Write the row mapper",
            "The SQLite store has no way to turn a row into an entity yet. \
             Done when all thirteen types round-trip.",
        );
        settle(&mut task.audit);
        task.number = 131;
        task.rank = 1024.5;
        task.labels = vec!["phase-9".to_owned(), "store".to_owned()];
        task.status = TaskStatus::InProgress;
        task.claimed_by = Some("claude".to_owned());
        task.claimed_at = Some(stored_now());

        let entity: Entity = task.into();
        assert_eq!(round_trip(&conn, &entity).unwrap(), entity);
    }

    #[test]
    fn a_milestone_round_trips() {
        let conn = schema_conn();
        let project = EntityId::generate(EntityType::Project);
        let mut milestone = Milestone::new(project, "Phase 9", "One database, not three.");
        settle(&mut milestone.audit);
        milestone.target_date = NaiveDate::from_ymd_opt(2026, 9, 1);
        milestone.sort_order = Some(9);

        let entity: Entity = milestone.into();
        assert_eq!(round_trip(&conn, &entity).unwrap(), entity);
    }

    /// A value outside a closed enum has to fail as an error naming the column,
    /// not as a panic. `STRICT` cannot catch this one — the column is TEXT and
    /// `not_a_status` is perfectly good text — so the reader is the only place
    /// it can be caught at all.
    #[test]
    fn an_unknown_enum_value_is_an_error_that_names_its_column() {
        let conn = schema_conn();
        let project = EntityId::generate(EntityType::Project);
        let mut task = Task::new(project, "t", "A row this test needs in the store.");
        settle(&mut task.audit);
        round_trip(&conn, &task.into()).unwrap();

        conn.execute("UPDATE tasks SET status = 'not_a_status'", [])
            .unwrap();

        let spec = spec_for(EntityType::Task);
        let result = conn
            .query_row(&select_from(&spec), [], |row| {
                Ok(from_row(EntityType::Task, row))
            })
            .unwrap();

        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("`not_a_status` should not have parsed"),
        };
        let message = err.to_string();
        assert!(message.contains("status"), "unhelpful error: {message}");
        assert!(
            message.contains("not_a_status"),
            "unhelpful error: {message}"
        );
    }

    /// Every one of the thirteen, written and read back through the real
    /// schema.
    ///
    /// The three tests above cover the interesting fields; this one covers the
    /// column *lists*, which is the failure that would otherwise wait for the
    /// store. A name in `spec_for` that no table has is a "no such column"
    /// error, and there are thirteen chances to have one.
    #[test]
    fn every_type_round_trips_against_the_real_schema() {
        let conn = schema_conn();
        let project = EntityId::generate(EntityType::Project);
        let metric = EntityId::generate(EntityType::Metric);

        let mut entities: Vec<Entity> = vec![
            Project::new("k", "Specline").into(),
            Milestone::new(project.clone(), "P0", "The first phase, for a store test.").into(),
            Task::new(
                project.clone(),
                "t",
                "A row this test needs in the store. Done when it reads back.",
            )
            .into(),
            Spec::new(project.clone(), "s").into(),
            Decision::new(project.clone(), "d").into(),
            Question::new(project.clone(), "q").into(),
            crate::Term::new(Some(project.clone()), "specline", "This store.").into(),
            Feedback::new(project.clone(), "f").into(),
            Design::new(project.clone(), "d").into(),
            Environment::new(project.clone(), "prod").into(),
            Metric::new(project.clone(), "m").into(),
            MetricObservation::new(metric, project.clone(), 1.5, stored_now()).into(),
            Artifact::new(project, "a").into(),
        ];
        assert_eq!(entities.len(), EntityType::ALL.len());

        for entity in &mut entities {
            settle(entity.audit_mut());
            assert_eq!(
                &round_trip(&conn, entity).unwrap(),
                entity,
                "{} did not survive a round trip",
                entity.entity_type()
            );
        }
    }

    /// The generated statement has to name the table the entity type says it
    /// lives in, or a read goes to the wrong table entirely.
    #[test]
    fn statements_name_the_table_the_entity_type_does() {
        for t in EntityType::ALL {
            let spec = spec_for(t);
            assert_eq!(spec.table, t.table());
            assert!(select_from(&spec).contains(t.table()));
            assert!(insert_stmt(&spec).contains(t.table()));
        }
    }

    /// Numbered placeholders run `?1..?N` with none skipped. Binding is
    /// positional either way, but a gap would bind every later value one column
    /// early and SQLite would not complain.
    #[test]
    fn placeholders_are_numbered_from_one_without_gaps() {
        for t in EntityType::ALL {
            let spec = spec_for(t);
            let stmt = insert_stmt(&spec);
            let count = spec.cols.len() + AUDIT_COLS.len();
            for n in 1..=count {
                assert!(stmt.contains(&format!("?{n}")), "{t} is missing ?{n}");
            }
            assert!(!stmt.contains(&format!("?{}", count + 1)));
        }
    }
}
