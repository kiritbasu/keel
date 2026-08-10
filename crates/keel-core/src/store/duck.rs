//! The DuckDB + Lance store.
//!
//! One connection, one write handle, both engines. The Lance datasets are
//! `ATTACH`ed as a namespace so a single SQL statement can join a task to the
//! spec revision that motivated it (DECISIONS B-2).
//!
//! # Where the SQL is safe and where it is interpolated
//!
//! Every caller-supplied value is bound as a parameter. Three things are
//! interpolated into SQL text instead, and all three come from closed Rust
//! enums that no caller can influence: table names (from [`EntityType`]),
//! relation names (from [`Relation`]) and the traversal direction's column
//! names (from [`Direction`]). Interpolating them is what lets the graph
//! traversal be one query rather than nine, and there is no path by which a
//! string from an agent reaches any of them.

use super::rows::{from_row, insert_params, spec_for};
use super::schema::{MIGRATION_TABLE, migrations};
use super::{
    Created, EntityQuery, EntityStore, GraphStore, Neighbour, Page, patch::apply_changes,
    patch::is_status_change,
};
use crate::{
    Action, Actor, Audit, Cursor, DEFAULT_DEPTH, Direction, Entity, EntityId, EntityType, Error,
    Event, EventId, Link, LinkId, MAX_DEPTH, NewEvent, NewLink, NewNote, Note, NoteId,
    ProjectScope, Provenance, Relation, Result, Surface,
};
use chrono::{DateTime, Utc};
use duckdb::types::{TimeUnit, Value};
use duckdb::{Connection, Row, params_from_iter};
use std::path::{Path, PathBuf};

/// The default cap on any list that does not specify one.
///
/// Not a silent truncation: every [`Page`] reports its total, so a caller that
/// hits this cap is told so.
pub const DEFAULT_LIST_LIMIT: usize = 200;

/// The DuckDB-and-Lance implementation of all three storage traits.
pub struct DuckStore {
    conn: Connection,
    root: PathBuf,
    embedder: Option<std::sync::Arc<dyn crate::Embedder>>,
}

impl std::fmt::Debug for DuckStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DuckStore")
            .field("root", &self.root)
            .finish()
    }
}

impl DuckStore {
    /// Open, or create, a store rooted at `root`.
    ///
    /// The path is passed in rather than read from the environment, because
    /// `keel-core` never reads environment variables — that boundary is what
    /// lets a test point at a temporary directory without a process-wide
    /// side effect.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let lance_dir = root.join("lance");
        std::fs::create_dir_all(&lance_dir).map_err(Error::io(format!(
            "create the store directory at {}",
            root.display()
        )))?;

        let db_path = root.join("keel.duckdb");
        let conn = Connection::open(&db_path).map_err(Error::storage(format!(
            "open the DuckDB database at {}",
            db_path.display()
        )))?;

        let mut store = DuckStore {
            conn,
            root,
            embedder: None,
        };
        store.load_extensions()?;
        store.attach_lance(&lance_dir)?;
        store.migrate()?;
        Ok(store)
    }

    /// Attach an embedder, enabling the semantic half of hybrid search.
    ///
    /// Optional on purpose. A store with no embedder is still fully usable and
    /// still searchable by keyword — search degrades rather than failing.
    /// Passing it in rather than constructing it here is what keeps `keel-core`
    /// free of decisions about model files and network access.
    pub fn with_embedder(mut self, embedder: std::sync::Arc<dyn crate::Embedder>) -> Self {
        self.embedder = Some(embedder);
        self
    }

    /// The attached embedder, if any.
    pub fn embedder(&self) -> Option<&dyn crate::Embedder> {
        self.embedder.as_deref()
    }

    /// Where the local embedding model is cached (SPEC §11).
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }

    /// Open a store in a temporary directory. Test helper.
    ///
    /// Public because the daemon's integration tests need it too, and a second
    /// copy of the setup dance is a second thing to keep in step.
    pub fn open_in(dir: impl AsRef<Path>) -> Result<Self> {
        Self::open(dir)
    }

    /// The store's root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Flush everything to disk and leave the file consistent.
    ///
    /// Called on shutdown. DuckDB checkpoints on a clean close, but a process
    /// that is killed does not get one — and an interrupted write can leave an
    /// ART index disagreeing with its table. That failure is invisible until
    /// the next `UPDATE`, which then raises a FATAL error that poisons the
    /// connection, so every later query fails with whatever operation happened
    /// to be running. It cost an evening: the store looked corrupt, `fsck`
    /// said clean, and both were true.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("CHECKPOINT;")
            .map_err(Error::storage(
                "checkpoint the database before shutting down",
            ))
    }

    /// Borrow the connection for a read-only query.
    ///
    /// Exposed for `keel-cli fsck` and the backup command, which legitimately
    /// need arbitrary SQL and would otherwise force a trait method per check.
    /// Not a general escape hatch: writes still go through the traits.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    fn load_extensions(&self) -> Result<()> {
        // `lance` is a core extension on 1.5.x; `fts` provides the BM25 index
        // over the non-prose entity types (SPEC §5).
        self.conn
            .execute_batch("INSTALL lance; LOAD lance; INSTALL fts; LOAD fts;")
            .map_err(Error::storage(
                "install and load the DuckDB `lance` and `fts` extensions. The first run \
                 needs network access to extensions.duckdb.org",
            ))
    }

    fn attach_lance(&self, lance_dir: &Path) -> Result<()> {
        // ATTACH takes the *directory* that holds the datasets, not a single
        // dataset path. Attaching `…/documents.lance` would look for
        // `documents.lance/documents.lance` and silently find nothing — the
        // error SPEC §5 originally contained.
        let sql = format!(
            "ATTACH IF NOT EXISTS '{}' AS lancedb (TYPE lance)",
            lance_dir.display()
        );
        self.conn
            .execute_batch(&sql)
            .map_err(Error::storage(format!(
                "attach the Lance datasets at {}",
                lance_dir.display()
            )))
    }

    /// Apply any migrations that have not run yet.
    fn migrate(&mut self) -> Result<()> {
        self.conn
            .execute_batch(MIGRATION_TABLE)
            .map_err(Error::storage("create the migration bookkeeping table"))?;

        let applied: Vec<i32> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM _keel_migrations ORDER BY id")
                .map_err(Error::storage("read the applied migration list"))?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i32>(0))
                .map_err(Error::storage("read the applied migration list"))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Error::storage("read the applied migration list"))?
        };

        for m in migrations() {
            if applied.contains(&m.id) {
                continue;
            }
            tracing::info!(migration = m.id, name = m.name, "applying migration");
            self.conn
                .execute_batch(m.sql)
                .map_err(Error::storage(format!(
                    "apply migration {} ({})",
                    m.id, m.name
                )))?;
            self.conn
                .execute(
                    "INSERT INTO _keel_migrations (id, name, applied_at) VALUES (?, ?, ?)",
                    duckdb::params![m.id, m.name, now_value()],
                )
                .map_err(Error::storage(format!("record migration {}", m.id)))?;
        }
        Ok(())
    }

    /// Find an existing entity by idempotency key, honouring each type's
    /// uniqueness scope.
    fn find_by_key(
        &self,
        entity_type: EntityType,
        project_id: Option<&EntityId>,
        key: &str,
    ) -> Result<Option<Entity>> {
        let spec = spec_for(entity_type);
        let (predicate, params): (&str, Vec<Value>) = match entity_type.project_scope() {
            // Projects are globally unique on their key — they have no parent
            // to scope by.
            ProjectScope::IsTheProject => {
                ("idempotency_key = ?", vec![Value::Text(key.to_owned())])
            }
            // A global term and a project-scoped one of the same name must be
            // able to coexist (Q-4), so the COALESCE here mirrors the index.
            ProjectScope::Optional => (
                "COALESCE(project_id, '') = ? AND idempotency_key = ?",
                vec![
                    Value::Text(project_id.map(EntityId::as_str).unwrap_or("").to_owned()),
                    Value::Text(key.to_owned()),
                ],
            ),
            ProjectScope::Required => (
                "project_id = ? AND idempotency_key = ?",
                vec![
                    Value::Text(project_id.map(EntityId::as_str).unwrap_or("").to_owned()),
                    Value::Text(key.to_owned()),
                ],
            ),
        };
        let sql = format!("{} WHERE {predicate}", spec.select_from());
        self.query_one(entity_type, &sql, params)
    }

    /// A live entity of this type in this project whose label means the same
    /// thing as `label`.
    ///
    /// Only ever consulted on create, and only after the exact key has missed.
    /// Archived rows are excluded here — unlike the exact-key path, which
    /// deliberately matches them: reviving an archived row on a *fuzzy* match
    /// would resurrect something a human chose to put away.
    fn find_by_similar_label(
        &self,
        entity_type: EntityType,
        project_id: Option<&EntityId>,
        label: &str,
    ) -> Result<Option<Entity>> {
        // Three types are excluded, each for its own reason:
        //
        // - `metric_observation`: its label is a note, and two readings of one
        //   metric are emphatically not the same row.
        // - `artifact`: named by URL or filename, where a near-match is a
        //   different file.
        // - `term`: a glossary entry's name *is* its identity, and Q-4 requires
        //   a global term and a project-scoped one of the same name to coexist.
        //   The COALESCE index already expresses that exactly; guessing on top
        //   of it can only be wrong.
        if matches!(
            entity_type,
            EntityType::MetricObservation | EntityType::Artifact | EntityType::Term
        ) {
            return Ok(None);
        }

        let mut query = EntityQuery::default().of_type(entity_type).limited(2_000);
        if let Some(p) = project_id {
            query = EntityQuery::in_project(p.clone())
                .of_type(entity_type)
                .limited(2_000);
        }
        let page = self.list(&query)?;

        let mut best: Option<(f64, Entity)> = None;
        for candidate in page.items {
            if !crate::types::same_thing(candidate.label(), label) {
                continue;
            }
            let score = crate::types::title_similarity(candidate.label(), label);
            if best.as_ref().is_none_or(|(b, _)| score > *b) {
                best = Some((score, candidate));
            }
        }
        Ok(best.map(|(_, e)| e))
    }

    fn query_one(
        &self,
        entity_type: EntityType,
        sql: &str,
        params: Vec<Value>,
    ) -> Result<Option<Entity>> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(Error::storage(format!("prepare a {entity_type} lookup")))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage(format!("run a {entity_type} lookup")))?;
        match rows
            .next()
            .map_err(Error::storage(format!("read a {entity_type} row")))?
        {
            Some(row) => Ok(Some(from_row(entity_type, row)?)),
            None => Ok(None),
        }
    }

    fn query_many(
        &self,
        entity_type: EntityType,
        sql: &str,
        params: Vec<Value>,
    ) -> Result<Vec<Entity>> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(Error::storage(format!("prepare a {entity_type} list")))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage(format!("run a {entity_type} list")))?;
        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(Error::storage(format!("read a {entity_type} row")))?
        {
            out.push(from_row(entity_type, row)?);
        }
        Ok(out)
    }

    fn count(&self, sql: &str, params: Vec<Value>) -> Result<usize> {
        let n: i64 = self
            .conn
            .query_row(sql, params_from_iter(params), |r| r.get(0))
            .map_err(Error::storage("count matching rows"))?;
        Ok(n.max(0) as usize)
    }

    /// Write the full row back, under optimistic concurrency.
    ///
    /// The `WHERE version = ?` is what actually enforces REQ-7. Checking the
    /// version in Rust and then updating unconditionally would leave a window
    /// between the two in which another writer could land — the exact race the
    /// requirement exists to close.
    fn write_back(&self, entity: &Entity, expected_version: i32) -> Result<()> {
        let entity_type = entity.entity_type();
        let spec = spec_for(entity_type);
        let assignments: Vec<String> = spec
            .cols
            .iter()
            .skip(1) // never reassign `id`
            .map(|c| match c {
                super::rows::Col::Plain(n) => format!("{n} = ?"),
                super::rows::Col::Array(n) => format!("{n} = json_extract_string(?, '$[*]')"),
            })
            .chain(
                [
                    "created_at = ?",
                    "updated_at = ?",
                    "version = ?",
                    "created_by = ?",
                    "updated_by = ?",
                    "session_id = ?",
                    "surface = ?",
                    "archived_at = ?",
                ]
                .iter()
                .map(|s| (*s).to_owned()),
            )
            .collect();

        let sql = format!(
            "UPDATE {} SET {} WHERE id = ? AND version = ?",
            spec.table,
            assignments.join(", ")
        );

        // `insert_params` yields id first; drop it and re-append it for the
        // WHERE clause so the SET list and the parameter list stay aligned.
        let mut params = insert_params(entity);
        params.remove(0);
        params.push(Value::Text(entity.id().as_str().to_owned()));
        params.push(Value::Int(expected_version));

        let affected = self
            .conn
            .execute(&sql, params_from_iter(params))
            .map_err(Error::storage(format!("update {}", entity.id())))?;

        if affected == 0 {
            // Either the row moved under us or it never existed. Re-read to
            // tell the caller which, because "stale" and "missing" need
            // different responses from an agent.
            let latest = self.get(entity.id())?;
            return match latest {
                Some(current) => Err(Error::StaleVersion {
                    entity_type,
                    id: entity.id().to_string(),
                    supplied: expected_version,
                    latest: current.audit().version,
                }),
                None => Err(Error::NotFound {
                    entity_type,
                    id: entity.id().to_string(),
                }),
            };
        }
        Ok(())
    }

    /// Append an event without opening its own transaction.
    fn append_event_inner(
        &self,
        event: NewEvent,
        provenance: &Provenance,
        now: DateTime<Utc>,
    ) -> Result<Event> {
        let stored = Event {
            id: EventId::generate(),
            project_id: event.project_id,
            entity_type: event.entity_id.entity_type(),
            entity_id: event.entity_id,
            action: event.action,
            field: event.field,
            before: event.before,
            after: event.after,
            actor: provenance.actor,
            session_id: provenance.session_id.clone(),
            surface: provenance.surface,
            summary: event.summary,
            meta: event.meta,
            created_at: now,
        };

        let params: Vec<Value> = vec![
            Value::Text(stored.id.as_str().to_owned()),
            stored
                .project_id
                .as_ref()
                .map(|p| Value::Text(p.as_str().to_owned()))
                .unwrap_or(Value::Null),
            Value::Text(stored.entity_type.as_str().to_owned()),
            Value::Text(stored.entity_id.as_str().to_owned()),
            Value::Text(stored.action.as_str().to_owned()),
            stored
                .field
                .as_ref()
                .map(|f| Value::Text(f.clone()))
                .unwrap_or(Value::Null),
            json_param(stored.before.as_ref()),
            json_param(stored.after.as_ref()),
            Value::Text(stored.actor.as_str().to_owned()),
            stored
                .session_id
                .as_ref()
                .map(|x| Value::Text(x.clone()))
                .unwrap_or(Value::Null),
            stored
                .surface
                .map(|x| Value::Text(x.as_str().to_owned()))
                .unwrap_or(Value::Null),
            Value::Text(stored.summary.clone()),
            json_param(stored.meta.as_ref()),
            Value::Timestamp(TimeUnit::Microsecond, stored.created_at.timestamp_micros()),
        ];

        self.conn
            .execute(
                "INSERT INTO events (id, project_id, entity_type, entity_id, action, field, \
                 before, after, actor, session_id, surface, summary, meta, created_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params_from_iter(params),
            )
            .map_err(Error::storage(format!(
                "append a `{}` event for {}",
                stored.action, stored.entity_id
            )))?;

        Ok(stored)
    }

    /// Resolve the project an entity belongs to, for event tagging.
    fn project_of(entity: &Entity) -> Option<EntityId> {
        match entity {
            // A project's own events are tagged with itself, so that
            // "everything that happened in project X" includes X's creation.
            Entity::Project(p) => Some(p.id.clone()),
            other => other.project_id().cloned(),
        }
    }

    /// Check that a link's endpoints exist and are not archived.
    ///
    /// This is the foreign key DuckDB cannot declare: `links` is polymorphic
    /// across thirteen tables (SPEC §3.1). Skipping it would let a typo create
    /// an edge to nothing, which a traversal would then silently drop.
    fn require_live(&self, id: &EntityId, role: &str) -> Result<Entity> {
        match self.get(id)? {
            None => Err(Error::Invariant {
                operation: format!("link {role} {id}"),
                problem: format!("no {} exists with id {id}", id.entity_type()),
            }),
            Some(e) if e.audit().is_archived() => Err(Error::Invariant {
                operation: format!("link {role} {id}"),
                problem: format!(
                    "{id} is archived; restore it before linking, or link a live entity"
                ),
            }),
            Some(e) => Ok(e),
        }
    }
}

/// Bind an optional JSON value.
fn json_param(v: Option<&serde_json::Value>) -> Value {
    v.map(|j| Value::Text(j.to_string())).unwrap_or(Value::Null)
}

/// The current instant as a bindable timestamp.
fn now_value() -> Value {
    Value::Timestamp(TimeUnit::Microsecond, Utc::now().timestamp_micros())
}

/// Render a relation filter as SQL.
///
/// Interpolated rather than bound: the values come from [`Relation`], a closed
/// Rust enum, so no caller string reaches this. An empty list means every
/// stored relation, and `depends_on` can never appear because it is normalised
/// away on write (D-11).
fn rel_filter(rels: &[Relation], alias: &str) -> String {
    let effective: Vec<Relation> = if rels.is_empty() {
        Relation::STORED.to_vec()
    } else {
        rels.iter().copied().filter(|r| r.is_stored()).collect()
    };
    if effective.is_empty() {
        // Only `depends_on` was requested. It is never stored, so the honest
        // answer is "no edges match" rather than "all of them".
        return "FALSE".to_owned();
    }
    let list = effective
        .iter()
        .map(|r| format!("'{}'", r.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    format!("{alias}.rel IN ({list})")
}

impl EntityStore for DuckStore {
    fn create(&mut self, mut entity: Entity, provenance: &Provenance) -> Result<Created> {
        let entity_type = entity.entity_type();
        let project_id = entity.project_id().cloned();

        if let Some(existing) =
            self.find_by_key(entity_type, project_id.as_ref(), entity.idempotency_key())?
        {
            // SPEC §7.2: a repeat call returns the existing entity rather than
            // erroring, so a retrying agent gets a sane result. Note this also
            // returns *archived* matches — deliberately, because silently
            // minting a second row alongside an archived one is how a store
            // fills up with near-duplicates.
            return Ok(Created {
                entity: existing,
                created: false,
            });
        }

        // A near-miss on the title is the same failure the key exists to
        // prevent, one step less exact. The key is a hash, so it cannot see
        // that two titles describe one thing; this can.
        //
        // Unless the caller supplied their own key. That is them saying "these
        // are different things that happen to share a title" — two `Deploy`
        // tasks keyed `deploy-staging` and `deploy-production` — and guessing
        // over an explicit assertion is exactly the false merge that hides
        // work. A derived key carries no such claim, so only then do we look.
        let key_was_derived = entity.idempotency_key()
            == crate::types::derive_idempotency_key(
                project_id.as_ref(),
                entity_type,
                entity.natural_key(),
            );
        if key_was_derived
            && let Some(existing) =
                self.find_by_similar_label(entity_type, project_id.as_ref(), entity.label())?
        {
            tracing::info!(
                existing = %existing.label(),
                attempted = %entity.label(),
                "returning an existing {entity_type} with a near-identical title rather than \
                 creating a second row"
            );
            return Ok(Created {
                entity: existing,
                created: false,
            });
        }

        let now = Utc::now();
        *entity.audit_mut() = Audit::new(provenance, now);

        let spec = spec_for(entity_type);
        self.conn
            .execute(
                &spec.insert_stmt(),
                params_from_iter(insert_params(&entity)),
            )
            .map_err(Error::storage(format!(
                "create the {entity_type} `{}`",
                entity.label()
            )))?;

        let summary = format!("created {entity_type} “{}”", entity.label());
        self.append_event_inner(
            NewEvent::new(entity.id().clone(), Action::Created, summary)
                .in_project(Self::project_of(&entity)),
            provenance,
            now,
        )?;

        Ok(Created {
            entity,
            created: true,
        })
    }

    fn get(&self, id: &EntityId) -> Result<Option<Entity>> {
        let entity_type = id.entity_type();
        let sql = format!("{} WHERE id = ?", spec_for(entity_type).select_from());
        self.query_one(entity_type, &sql, vec![Value::Text(id.as_str().to_owned())])
    }

    fn update(
        &mut self,
        id: &EntityId,
        expected_version: i32,
        changes: &serde_json::Map<String, serde_json::Value>,
        provenance: &Provenance,
    ) -> Result<Entity> {
        let entity_type = id.entity_type();
        let mut entity = self.get(id)?.ok_or_else(|| Error::NotFound {
            entity_type,
            id: id.to_string(),
        })?;

        let current_version = entity.audit().version;
        if current_version != expected_version {
            return Err(Error::StaleVersion {
                entity_type,
                id: id.to_string(),
                supplied: expected_version,
                latest: current_version,
            });
        }

        // An accepted decision is immutable by design (SPEC §3.2): supersede
        // it rather than editing it. Enforced here because the schema cannot
        // express it.
        if let Entity::Decision(d) = &entity
            && d.status == crate::DecisionStatus::Accepted
            && changes.keys().any(|k| k != "status")
        {
            return Err(Error::DecisionImmutable { id: id.to_string() });
        }

        let applied = apply_changes(&mut entity, changes)?;
        if applied.is_empty() {
            return Ok(entity);
        }

        let now = Utc::now();
        let next_version = current_version + 1;
        *entity.audit_mut() = entity.audit().touched(provenance, now, next_version);
        self.write_back(&entity, expected_version)?;

        let project = Self::project_of(&entity);
        let action = if is_status_change(&applied) {
            Action::StatusChanged
        } else {
            Action::Updated
        };

        // One event per field. Verbose, but "what changed" is the question the
        // activity feed exists to answer, and a single event with a bag of
        // fields cannot be filtered by field later.
        for change in &applied {
            let summary = format!(
                "{} {} → {}",
                change.field,
                render(&change.before),
                render(&change.after)
            );
            self.append_event_inner(
                NewEvent::new(entity.id().clone(), action, summary)
                    .in_project(project.clone())
                    .field_change(
                        change.field.clone(),
                        change.before.clone(),
                        change.after.clone(),
                    ),
                provenance,
                now,
            )?;
        }

        Ok(entity)
    }

    fn archive(
        &mut self,
        id: &EntityId,
        expected_version: i32,
        provenance: &Provenance,
    ) -> Result<Entity> {
        let entity_type = id.entity_type();
        let mut entity = self.get(id)?.ok_or_else(|| Error::NotFound {
            entity_type,
            id: id.to_string(),
        })?;

        let current_version = entity.audit().version;
        if current_version != expected_version {
            return Err(Error::StaleVersion {
                entity_type,
                id: id.to_string(),
                supplied: expected_version,
                latest: current_version,
            });
        }
        if entity.audit().is_archived() {
            return Ok(entity);
        }

        let now = Utc::now();
        let mut audit = entity.audit().touched(provenance, now, current_version + 1);
        audit.archived_at = Some(now);
        *entity.audit_mut() = audit;
        self.write_back(&entity, expected_version)?;

        // Archiving a parent archives its links but never its children
        // (SPEC §3.1). Orphaned children surface in `fsck` rather than
        // disappearing, because a cascade is unrecoverable and an orphan is
        // merely untidy.
        self.conn
            .execute(
                "UPDATE links SET archived_at = ?, updated_at = ?, version = version + 1, \
                 updated_by = ? WHERE (from_id = ? OR to_id = ?) AND archived_at IS NULL",
                params_from_iter(vec![
                    Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                    Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                    Value::Text(provenance.actor.as_str().to_owned()),
                    Value::Text(id.as_str().to_owned()),
                    Value::Text(id.as_str().to_owned()),
                ]),
            )
            .map_err(Error::storage(format!("archive the links touching {id}")))?;

        self.append_event_inner(
            NewEvent::new(
                entity.id().clone(),
                Action::Archived,
                format!("archived {entity_type} “{}”", entity.label()),
            )
            .in_project(Self::project_of(&entity)),
            provenance,
            now,
        )?;

        Ok(entity)
    }

    fn list(&self, query: &EntityQuery) -> Result<Page<Entity>> {
        let types: Vec<EntityType> = if query.entity_types.is_empty() {
            EntityType::ALL.to_vec()
        } else {
            query.entity_types.clone()
        };

        let limit = query.limit.unwrap_or(DEFAULT_LIST_LIMIT);
        let mut all = Vec::new();
        let mut total = 0usize;

        for entity_type in types {
            let spec = spec_for(entity_type);
            let mut clauses: Vec<String> = Vec::new();
            let mut params: Vec<Value> = Vec::new();

            if let Some(p) = &query.project_id {
                match entity_type.project_scope() {
                    ProjectScope::IsTheProject => {
                        clauses.push("id = ?".to_owned());
                        params.push(Value::Text(p.as_str().to_owned()));
                    }
                    // A global term belongs to every project's glossary, so a
                    // project-scoped list must include globals as well as
                    // overrides — that is what "project-first resolution"
                    // means in practice (Q-4).
                    ProjectScope::Optional => {
                        clauses.push("(project_id = ? OR project_id IS NULL)".to_owned());
                        params.push(Value::Text(p.as_str().to_owned()));
                    }
                    ProjectScope::Required => {
                        clauses.push("project_id = ?".to_owned());
                        params.push(Value::Text(p.as_str().to_owned()));
                    }
                }
            }

            if !query.include_archived {
                clauses.push("archived_at IS NULL".to_owned());
            }

            if !query.statuses.is_empty() {
                // Four types have no lifecycle at all. Filtering them by
                // status should exclude them, not error — a cross-type query
                // for "everything blocked" is a reasonable thing to ask.
                let status_col = match entity_type {
                    EntityType::Design => Some("state"),
                    EntityType::Term
                    | EntityType::Feedback
                    | EntityType::Metric
                    | EntityType::MetricObservation
                    | EntityType::Artifact => None,
                    _ => Some("status"),
                };
                match status_col {
                    None => continue,
                    Some(col) => {
                        let placeholders = vec!["?"; query.statuses.len()].join(", ");
                        clauses.push(format!("{col} IN ({placeholders})"));
                        params.extend(query.statuses.iter().map(|s| Value::Text(s.clone())));
                    }
                }
            }

            if let Some(since) = query.since {
                clauses.push("created_at >= ?".to_owned());
                params.push(Value::Timestamp(
                    TimeUnit::Microsecond,
                    since.timestamp_micros(),
                ));
            }
            if let Some(until) = query.until {
                clauses.push("created_at < ?".to_owned());
                params.push(Value::Timestamp(
                    TimeUnit::Microsecond,
                    until.timestamp_micros(),
                ));
            }

            let where_clause = if clauses.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", clauses.join(" AND "))
            };

            total += self.count(
                &format!("SELECT count(*) FROM {}{where_clause}", spec.table),
                params.clone(),
            )?;

            let sql = format!(
                "{}{where_clause} ORDER BY id DESC LIMIT {} OFFSET {}",
                spec.select_from(),
                limit,
                query.offset
            );
            all.extend(self.query_many(entity_type, &sql, params)?);
        }

        // Across types, order by id — which is creation order, since ULIDs
        // sort chronologically (B-9).
        all.sort_by(|a, b| b.id().cmp(a.id()));
        all.truncate(limit);
        Ok(Page::new(all, total))
    }

    fn link(&mut self, link: NewLink, provenance: &Provenance) -> Result<Link> {
        let requested_rel = link.rel;
        let (from_id, rel, to_id, anchor, note) = link.normalised()?;

        let from = self.require_live(&from_id, "source")?;
        let to = self.require_live(&to_id, "target")?;

        // Re-creating an existing edge returns it rather than erroring: the
        // unique index would reject it, and an agent re-asserting a true fact
        // should not be punished for it.
        if let Some(existing) = self.find_link(&from_id, rel, &to_id, &anchor, true)? {
            if existing.audit.is_archived() {
                // Un-archive rather than insert a duplicate: the unique index
                // covers archived rows too, so a second insert would fail.
                let now = Utc::now();
                self.conn
                    .execute(
                        "UPDATE links SET archived_at = NULL, updated_at = ?, \
                         version = version + 1, updated_by = ? WHERE id = ?",
                        params_from_iter(vec![
                            Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                            Value::Text(provenance.actor.as_str().to_owned()),
                            Value::Text(existing.id.as_str().to_owned()),
                        ]),
                    )
                    .map_err(Error::storage(format!("restore the link {}", existing.id)))?;
                return self
                    .find_link(&from_id, rel, &to_id, &anchor, true)?
                    .ok_or_else(|| Error::Invariant {
                        operation: format!("restore the link {}", existing.id),
                        problem: "the link vanished between restoring and re-reading it".to_owned(),
                    });
            }
            return Ok(existing);
        }

        let now = Utc::now();
        let project_id = from.project_id().or_else(|| to.project_id()).cloned();
        let stored = Link {
            id: LinkId::generate(),
            project_id,
            from_type: from_id.entity_type(),
            from_id: from_id.clone(),
            rel,
            to_type: to_id.entity_type(),
            to_id: to_id.clone(),
            anchor: anchor.clone(),
            note,
            audit: Audit::new(provenance, now),
        };

        self.conn
            .execute(
                "INSERT INTO links (id, project_id, from_type, from_id, rel, to_type, to_id, \
                 anchor, note, created_at, updated_at, version, created_by, updated_by, \
                 session_id, surface, archived_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params_from_iter(link_params(&stored)),
            )
            .map_err(Error::storage(format!(
                "create the link {from_id} {rel} {to_id}"
            )))?;

        // Summaries name the artifacts, not their ids. This text is what the
        // activity feed and the Sunday-review digest actually show a human, and
        // "linked tsk_01KZK163THQG7 references fbk_01KZK16505G3J" is not a
        // sentence anyone can read. The ids are still on the event.
        //
        // The direction stated is the *stored* one. When a caller asked for
        // `depends_on`, saying so as well is what stops the next reader
        // thinking the endpoints were recorded backwards.
        let from_label = truncate(from.label(), 60);
        let to_label = truncate(to.label(), 60);
        let summary = if requested_rel == Relation::DependsOn {
            format!(
                "“{to_label}” depends on “{from_label}” (stored as “{from_label}” blocks                  “{to_label}”)"
            )
        } else {
            format!("“{from_label}” {rel} “{to_label}”")
        };
        self.append_event_inner(
            NewEvent::new(from_id, Action::Linked, summary)
                .in_project(stored.project_id.clone())
                .with_meta(serde_json::json!({
                    "rel": rel.as_str(),
                    "to_id": to_id.as_str(),
                    "anchor": anchor,
                })),
            provenance,
            now,
        )?;

        Ok(stored)
    }

    fn unlink(
        &mut self,
        from_id: &EntityId,
        rel: Relation,
        to_id: &EntityId,
        anchor: &str,
        provenance: &Provenance,
    ) -> Result<Link> {
        let (from, rel, to) = Relation::normalise(from_id.clone(), rel, to_id.clone());

        let existing = self
            .find_link(&from, rel, &to, anchor, false)?
            .ok_or_else(|| Error::Invariant {
                operation: format!("remove the link {from} {rel} {to}"),
                problem: "no live link matches those endpoints, relation and anchor".to_owned(),
            })?;

        let now = Utc::now();
        self.conn
            .execute(
                "UPDATE links SET archived_at = ?, updated_at = ?, version = version + 1, \
                 updated_by = ? WHERE id = ?",
                params_from_iter(vec![
                    Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                    Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                    Value::Text(provenance.actor.as_str().to_owned()),
                    Value::Text(existing.id.as_str().to_owned()),
                ]),
            )
            .map_err(Error::storage(format!("archive the link {}", existing.id)))?;

        let label_of = |id: &EntityId| {
            self.get(id)
                .ok()
                .flatten()
                .map(|e| truncate(e.label(), 60))
                .unwrap_or_else(|| id.to_string())
        };
        let summary = format!("unlinked “{}” {rel} “{}”", label_of(&from), label_of(&to));
        self.append_event_inner(
            NewEvent::new(from.clone(), Action::Linked, summary)
                .in_project(existing.project_id.clone())
                .with_meta(serde_json::json!({ "removed": true, "rel": rel.as_str() })),
            provenance,
            now,
        )?;

        let mut archived = existing;
        archived.audit.archived_at = Some(now);
        Ok(archived)
    }

    fn append_event(&mut self, event: NewEvent, provenance: &Provenance) -> Result<Event> {
        self.append_event_inner(event, provenance, Utc::now())
    }

    fn events(
        &self,
        cursor: &Cursor,
        project_id: Option<&EntityId>,
        limit: usize,
    ) -> Result<Page<Event>> {
        let mut clauses: Vec<String> = Vec::new();
        let mut params: Vec<Value> = Vec::new();

        match cursor {
            Cursor::After(id) => {
                clauses.push("id > ?".to_owned());
                params.push(Value::Text(id.as_str().to_owned()));
            }
            Cursor::Since(t) => {
                clauses.push("created_at >= ?".to_owned());
                params.push(Value::Timestamp(
                    TimeUnit::Microsecond,
                    t.timestamp_micros(),
                ));
            }
            Cursor::Beginning => {}
        }
        if let Some(p) = project_id {
            clauses.push("project_id = ?".to_owned());
            params.push(Value::Text(p.as_str().to_owned()));
        }

        let where_clause = if clauses.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", clauses.join(" AND "))
        };

        let total = self.count(
            &format!("SELECT count(*) FROM events{where_clause}"),
            params.clone(),
        )?;

        // Ascending by id: a cursor-following caller needs the *oldest*
        // unseen events first, otherwise a limit silently skips the middle of
        // the range.
        let sql = format!(
            "SELECT id, project_id, entity_type, entity_id, action, field, \
             CAST(before AS VARCHAR) AS before, CAST(after AS VARCHAR) AS after, \
             actor, session_id, surface, summary, CAST(meta AS VARCHAR) AS meta, created_at \
             FROM events{where_clause} ORDER BY id ASC LIMIT {limit}"
        );

        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(Error::storage("prepare an event query"))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage("run an event query"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(Error::storage("read an event row"))? {
            out.push(read_event(row)?);
        }
        Ok(Page::new(out, total))
    }

    fn add_note(&mut self, note: NewNote, provenance: &Provenance) -> Result<Note> {
        note.validate()?;

        // The subject must exist. `v_entities` is why this is one query rather
        // than a match over thirteen tables — resolving an id without knowing
        // its type is exactly what the view was built for.
        let Some((entity_type, project_id, archived)) = self.resolve_vertex(&note.entity_id)?
        else {
            return Err(Error::NotFound {
                entity_type: EntityType::Task,
                id: format!(
                    "{} — cannot annotate a row that does not exist",
                    note.entity_id
                ),
            });
        };
        if archived {
            return Err(Error::Invalid {
                entity_type,
                field: "entity_id".to_owned(),
                problem: format!("{} is archived", note.entity_id),
                expected: "a live row — annotating an archived one writes commentary that \
                           nothing will ever show. Restore it, or note the row that replaced it"
                    .to_owned(),
            });
        }

        let stored = Note {
            id: NoteId::generate(),
            project_id,
            entity_type,
            entity_id: note.entity_id,
            body: note.body,
            author: note.author,
            // Provenance wins over anything the caller put on the note, for the
            // same reason it does on every other write: one source of truth for
            // who is acting, decided at the boundary and not per call.
            session_id: note.session_id.or_else(|| provenance.session_id.clone()),
            surface: note.surface.or(provenance.surface),
            created_at: Utc::now(),
            archived_at: None,
        };

        self.conn
            .execute(
                "INSERT INTO notes (id, project_id, entity_type, entity_id, body, author, \
                 session_id, surface, created_at, archived_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL)",
                params_from_iter(vec![
                    Value::Text(stored.id.as_str().to_owned()),
                    stored
                        .project_id
                        .as_ref()
                        .map(|p| Value::Text(p.as_str().to_owned()))
                        .unwrap_or(Value::Null),
                    Value::Text(stored.entity_type.as_str().to_owned()),
                    Value::Text(stored.entity_id.as_str().to_owned()),
                    Value::Text(stored.body.clone()),
                    Value::Text(stored.author.as_str().to_owned()),
                    stored
                        .session_id
                        .clone()
                        .map(Value::Text)
                        .unwrap_or(Value::Null),
                    stored
                        .surface
                        .map(|s| Value::Text(s.as_str().to_owned()))
                        .unwrap_or(Value::Null),
                    Value::Timestamp(TimeUnit::Microsecond, stored.created_at.timestamp_micros()),
                ]),
            )
            .map_err(Error::storage(format!(
                "append a note to {}",
                stored.entity_id
            )))?;

        Ok(stored)
    }

    fn notes_for(&self, entity_id: &EntityId, include_retracted: bool) -> Result<Vec<Note>> {
        let filter = if include_retracted {
            ""
        } else {
            " AND archived_at IS NULL"
        };
        self.query_notes(
            &format!("{NOTE_COLUMNS} FROM notes WHERE entity_id = ?{filter} ORDER BY id ASC"),
            vec![Value::Text(entity_id.as_str().to_owned())],
        )
    }

    fn notes_in_project(&self, project_id: &EntityId) -> Result<Vec<Note>> {
        self.query_notes(
            &format!(
                "{NOTE_COLUMNS} FROM notes WHERE project_id = ? AND archived_at IS NULL \
                 ORDER BY id ASC"
            ),
            vec![Value::Text(project_id.as_str().to_owned())],
        )
    }

    fn retract_note(&mut self, id: &NoteId, provenance: &Provenance) -> Result<Note> {
        let _ = provenance;
        let now = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE notes SET archived_at = ? WHERE id = ? AND archived_at IS NULL",
                params_from_iter(vec![
                    Value::Timestamp(TimeUnit::Microsecond, now.timestamp_micros()),
                    Value::Text(id.as_str().to_owned()),
                ]),
            )
            .map_err(Error::storage(format!("retract note {id}")))?;
        if changed == 0 {
            // Either it never existed or it is already retracted. Both are the
            // caller believing something false about the store, and both are
            // worth saying out loud rather than returning a silent success.
            return Err(Error::NotFound {
                entity_type: EntityType::Task,
                id: format!("{id} — no live note with this id"),
            });
        }
        self.query_notes(
            &format!("{NOTE_COLUMNS} FROM notes WHERE id = ?"),
            vec![Value::Text(id.as_str().to_owned())],
        )?
        .pop()
        .ok_or_else(|| Error::NotFound {
            entity_type: EntityType::Task,
            id: id.to_string(),
        })
    }
}

impl DuckStore {
    /// Resolve any id to its type, project and archived state in one query.
    ///
    /// This is what `v_entities` is for: the caller has an id and no idea which
    /// of thirteen tables it lives in, and a `match` over all thirteen would
    /// have to be updated every time a type is added.
    fn resolve_vertex(
        &self,
        id: &EntityId,
    ) -> Result<Option<(EntityType, Option<EntityId>, bool)>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT entity_type, project_id, archived_at FROM v_entities WHERE id = ? LIMIT 1",
            )
            .map_err(Error::storage("prepare a vertex lookup"))?;
        let mut rows = stmt
            .query(params_from_iter(vec![Value::Text(id.as_str().to_owned())]))
            .map_err(Error::storage("run a vertex lookup"))?;
        let Some(row) = rows.next().map_err(Error::storage("read a vertex row"))? else {
            return Ok(None);
        };
        let e = |c: &'static str| Error::storage(format!("read column `{c}` of `v_entities`"));
        let entity_type = EntityType::parse(
            &row.get::<_, String>("entity_type")
                .map_err(e("entity_type"))?,
        )?;
        let project_id = match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) => Some(EntityId::parse(&p)?),
            None => None,
        };
        let archived = row
            .get::<_, Option<DateTime<Utc>>>("archived_at")
            .map_err(e("archived_at"))?
            .is_some();
        Ok(Some((entity_type, project_id, archived)))
    }

    /// Run a note query and read the rows.
    fn query_notes(&self, sql: &str, params: Vec<Value>) -> Result<Vec<Note>> {
        let mut stmt = self
            .conn
            .prepare(sql)
            .map_err(Error::storage("prepare a note query"))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage("run a note query"))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(Error::storage("read a note row"))? {
            out.push(read_note(row)?);
        }
        Ok(out)
    }

    /// Find one edge by its unique key.
    fn find_link(
        &self,
        from_id: &EntityId,
        rel: Relation,
        to_id: &EntityId,
        anchor: &str,
        include_archived: bool,
    ) -> Result<Option<Link>> {
        let archived = if include_archived {
            ""
        } else {
            " AND archived_at IS NULL"
        };
        let sql = format!(
            "SELECT id, project_id, from_type, from_id, rel, to_type, to_id, anchor, note, \
             created_at, updated_at, version, created_by, updated_by, session_id, surface, \
             archived_at FROM links \
             WHERE from_id = ? AND rel = ? AND to_id = ? AND anchor = ?{archived}"
        );
        let mut stmt = self
            .conn
            .prepare(&sql)
            .map_err(Error::storage("prepare a link lookup"))?;
        let mut rows = stmt
            .query(params_from_iter(vec![
                Value::Text(from_id.as_str().to_owned()),
                Value::Text(rel.as_str().to_owned()),
                Value::Text(to_id.as_str().to_owned()),
                Value::Text(anchor.to_owned()),
            ]))
            .map_err(Error::storage("run a link lookup"))?;
        match rows.next().map_err(Error::storage("read a link row"))? {
            Some(row) => Ok(Some(read_link(row)?)),
            None => Ok(None),
        }
    }
}

impl GraphStore for DuckStore {
    fn neighbours(
        &self,
        root: &EntityId,
        direction: Direction,
        rels: &[Relation],
        depth: u8,
    ) -> Result<Vec<Neighbour>> {
        let depth = depth.clamp(1, MAX_DEPTH);

        if direction == Direction::Both {
            // Run each way and merge, keeping the shallower reach when both
            // find the same node. Expressing `Both` as a single query would
            // need the union inside the recursive term, where a node reached
            // outbound could then be walked inbound — which is not "both
            // directions", it is an undirected walk, and it would return
            // things that are not related to the root in any stated way.
            let mut out = self.neighbours(root, Direction::Outbound, rels, depth)?;
            for n in self.neighbours(root, Direction::Inbound, rels, depth)? {
                match out.iter_mut().find(|e| e.id == n.id) {
                    Some(existing) if existing.depth > n.depth => *existing = n,
                    Some(_) => {}
                    None => out.push(n),
                }
            }
            out.sort_by_key(|n| (n.depth, n.id.as_str().to_owned()));
            return Ok(out);
        }

        // Outbound: match `from_id`, yield `to_id`. Inbound: the reverse.
        // SPEC §3.3 is the authority for which one a caller wants; getting it
        // backwards returns an empty set that looks like "nothing is linked".
        let (match_col, yield_col, yield_type) = match direction {
            Direction::Outbound => ("from_id", "to_id", "to_type"),
            _ => ("to_id", "from_id", "from_type"),
        };
        let filter = rel_filter(rels, "l");

        let sql = format!(
            "WITH RECURSIVE walk AS (
                 SELECT l.{yield_col} AS id, l.{yield_type} AS entity_type, l.rel, l.anchor,
                        1 AS depth, list_value(?, l.{yield_col}) AS path
                 FROM links l
                 WHERE l.{match_col} = ? AND l.archived_at IS NULL AND {filter}
               UNION ALL
                 SELECT l.{yield_col}, l.{yield_type}, l.rel, l.anchor,
                        w.depth + 1, list_append(w.path, l.{yield_col})
                 FROM links l
                 JOIN walk w ON l.{match_col} = w.id
                 WHERE l.archived_at IS NULL AND {filter}
                   AND w.depth < ?
                   AND NOT list_contains(w.path, l.{yield_col})
             )
             SELECT id, entity_type, rel, anchor, depth, to_json(path) AS path FROM walk
             ORDER BY depth, id"
        );

        let mut stmt = self.conn.prepare(&sql).map_err(Error::storage(format!(
            "prepare a {direction} traversal from {root}"
        )))?;
        let mut rows = stmt
            .query(params_from_iter(vec![
                Value::Text(root.as_str().to_owned()),
                Value::Text(root.as_str().to_owned()),
                Value::Int(i32::from(depth)),
            ]))
            .map_err(Error::storage(format!(
                "run a {direction} traversal from {root}"
            )))?;

        let mut out: Vec<Neighbour> = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(Error::storage("read a traversal row"))?
        {
            let id = EntityId::parse(
                &row.get::<_, String>("id")
                    .map_err(Error::storage("read traversal id"))?,
            )?;
            let entity_type = EntityType::parse(
                &row.get::<_, String>("entity_type")
                    .map_err(Error::storage("read traversal entity_type"))?,
            )?;
            let rel = Relation::parse(
                &row.get::<_, String>("rel")
                    .map_err(Error::storage("read traversal rel"))?,
            )?;
            let anchor = row
                .get::<_, Option<String>>("anchor")
                .map_err(Error::storage("read traversal anchor"))?
                .unwrap_or_default();
            let depth_val = row
                .get::<_, i32>("depth")
                .map_err(Error::storage("read traversal depth"))?;
            let path_json = row
                .get::<_, Option<String>>("path")
                .map_err(Error::storage("read traversal path"))?
                .unwrap_or_else(|| "[]".to_owned());
            let path = serde_json::from_str::<Vec<String>>(&path_json)
                .unwrap_or_default()
                .iter()
                .filter_map(|s| EntityId::parse(s).ok())
                .collect();

            // A node reachable by two paths appears twice; keep the shorter.
            let neighbour = Neighbour {
                id,
                entity_type,
                rel,
                anchor,
                depth: depth_val.clamp(0, i32::from(u8::MAX)) as u8,
                path,
            };
            match out.iter().position(|n| n.id == neighbour.id) {
                Some(i) if out[i].depth > neighbour.depth => out[i] = neighbour,
                Some(_) => {}
                None => out.push(neighbour),
            }
        }
        Ok(out)
    }

    fn links_of(&self, id: &EntityId, direction: Direction) -> Result<Vec<Link>> {
        let clause = match direction {
            Direction::Outbound => "from_id = ?",
            Direction::Inbound => "to_id = ?",
            Direction::Both => "from_id = ? OR to_id = ?",
        };
        let mut params = vec![Value::Text(id.as_str().to_owned())];
        if direction == Direction::Both {
            params.push(Value::Text(id.as_str().to_owned()));
        }

        let sql = format!(
            "SELECT id, project_id, from_type, from_id, rel, to_type, to_id, anchor, note, \
             created_at, updated_at, version, created_by, updated_by, session_id, surface, \
             archived_at FROM links WHERE ({clause}) AND archived_at IS NULL ORDER BY id"
        );
        let mut stmt = self.conn.prepare(&sql).map_err(Error::storage(format!(
            "prepare the {direction} links of {id}"
        )))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage(format!("run the {direction} links of {id}")))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(Error::storage("read a link row"))? {
            out.push(read_link(row)?);
        }
        Ok(out)
    }
}

/// The seventeen link params, in insert order.
fn link_params(l: &Link) -> Vec<Value> {
    vec![
        Value::Text(l.id.as_str().to_owned()),
        l.project_id
            .as_ref()
            .map(|p| Value::Text(p.as_str().to_owned()))
            .unwrap_or(Value::Null),
        Value::Text(l.from_type.as_str().to_owned()),
        Value::Text(l.from_id.as_str().to_owned()),
        Value::Text(l.rel.as_str().to_owned()),
        Value::Text(l.to_type.as_str().to_owned()),
        Value::Text(l.to_id.as_str().to_owned()),
        Value::Text(l.anchor.clone()),
        l.note
            .as_ref()
            .map(|n| Value::Text(n.clone()))
            .unwrap_or(Value::Null),
        Value::Timestamp(TimeUnit::Microsecond, l.audit.created_at.timestamp_micros()),
        Value::Timestamp(TimeUnit::Microsecond, l.audit.updated_at.timestamp_micros()),
        Value::Int(l.audit.version),
        Value::Text(l.audit.created_by.as_str().to_owned()),
        Value::Text(l.audit.updated_by.as_str().to_owned()),
        l.audit
            .session_id
            .as_ref()
            .map(|x| Value::Text(x.clone()))
            .unwrap_or(Value::Null),
        l.audit
            .surface
            .map(|x| Value::Text(x.as_str().to_owned()))
            .unwrap_or(Value::Null),
        l.audit
            .archived_at
            .map(|t| Value::Timestamp(TimeUnit::Microsecond, t.timestamp_micros()))
            .unwrap_or(Value::Null),
    ]
}

/// Rebuild a link from a row.
fn read_link(row: &Row<'_>) -> Result<Link> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of `links`"));
    Ok(Link {
        id: LinkId::parse(&row.get::<_, String>("id").map_err(e("id"))?)?,
        project_id: match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) => Some(EntityId::parse(&p)?),
            None => None,
        },
        from_type: EntityType::parse(&row.get::<_, String>("from_type").map_err(e("from_type"))?)?,
        from_id: EntityId::parse(&row.get::<_, String>("from_id").map_err(e("from_id"))?)?,
        rel: Relation::parse(&row.get::<_, String>("rel").map_err(e("rel"))?)?,
        to_type: EntityType::parse(&row.get::<_, String>("to_type").map_err(e("to_type"))?)?,
        to_id: EntityId::parse(&row.get::<_, String>("to_id").map_err(e("to_id"))?)?,
        anchor: row
            .get::<_, Option<String>>("anchor")
            .map_err(e("anchor"))?
            .unwrap_or_default(),
        note: row.get::<_, Option<String>>("note").map_err(e("note"))?,
        audit: super::rows::read_audit(row, "links")?,
    })
}

/// Rebuild an event from a row.
/// The note projection, named once so the three note queries cannot drift.
const NOTE_COLUMNS: &str = "SELECT id, project_id, entity_type, entity_id, body, author, \
                            session_id, surface, created_at, archived_at";

fn read_note(row: &Row<'_>) -> Result<Note> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of `notes`"));
    Ok(Note {
        id: NoteId::parse(&row.get::<_, String>("id").map_err(e("id"))?)?,
        project_id: match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) => Some(EntityId::parse(&p)?),
            None => None,
        },
        entity_type: EntityType::parse(
            &row.get::<_, String>("entity_type")
                .map_err(e("entity_type"))?,
        )?,
        entity_id: EntityId::parse(&row.get::<_, String>("entity_id").map_err(e("entity_id"))?)?,
        body: row.get::<_, String>("body").map_err(e("body"))?,
        author: Actor::parse(&row.get::<_, String>("author").map_err(e("author"))?)?,
        session_id: row
            .get::<_, Option<String>>("session_id")
            .map_err(e("session_id"))?,
        surface: match row
            .get::<_, Option<String>>("surface")
            .map_err(e("surface"))?
        {
            Some(s) => Some(Surface::parse(&s)?),
            None => None,
        },
        created_at: row
            .get::<_, DateTime<Utc>>("created_at")
            .map_err(e("created_at"))?,
        archived_at: row
            .get::<_, Option<DateTime<Utc>>>("archived_at")
            .map_err(e("archived_at"))?,
    })
}

fn read_event(row: &Row<'_>) -> Result<Event> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of `events`"));
    let json = |v: Option<String>| v.and_then(|s| serde_json::from_str(&s).ok());
    Ok(Event {
        id: EventId::parse(&row.get::<_, String>("id").map_err(e("id"))?)?,
        project_id: match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) => Some(EntityId::parse(&p)?),
            None => None,
        },
        entity_type: EntityType::parse(
            &row.get::<_, String>("entity_type")
                .map_err(e("entity_type"))?,
        )?,
        entity_id: EntityId::parse(&row.get::<_, String>("entity_id").map_err(e("entity_id"))?)?,
        action: Action::parse(&row.get::<_, String>("action").map_err(e("action"))?)?,
        field: row.get::<_, Option<String>>("field").map_err(e("field"))?,
        before: json(
            row.get::<_, Option<String>>("before")
                .map_err(e("before"))?,
        ),
        after: json(row.get::<_, Option<String>>("after").map_err(e("after"))?),
        actor: Actor::parse(&row.get::<_, String>("actor").map_err(e("actor"))?)?,
        session_id: row
            .get::<_, Option<String>>("session_id")
            .map_err(e("session_id"))?,
        surface: match row
            .get::<_, Option<String>>("surface")
            .map_err(e("surface"))?
        {
            Some(s) => Some(Surface::parse(&s)?),
            None => None,
        },
        summary: row
            .get::<_, Option<String>>("summary")
            .map_err(e("summary"))?
            .unwrap_or_default(),
        meta: json(row.get::<_, Option<String>>("meta").map_err(e("meta"))?),
        created_at: row
            .get::<_, DateTime<Utc>>("created_at")
            .map_err(e("created_at"))?,
    })
}

/// Shorten a label for a one-line summary, on a word boundary where possible.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let cut: String = text.chars().take(max).collect();
    match cut.rsplit_once(' ') {
        Some((head, _)) if head.len() > max / 2 => format!("{head}…"),
        _ => format!("{cut}…"),
    }
}

/// Render a JSON value for an event summary, without the quotes a raw
/// `to_string` would add around every string.
fn render(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "none".to_owned(),
        other => other.to_string(),
    }
}

/// The default traversal depth, re-exported so callers do not reach into
/// `link` for it when configuring a store query.
pub const DEFAULT_TRAVERSAL_DEPTH: u8 = DEFAULT_DEPTH;
