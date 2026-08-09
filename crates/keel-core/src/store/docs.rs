//! Document revisions, blobs, and hybrid search.
//!
//! The prose half of the store. Revisions are append-only rows in the single
//! Lance `documents` dataset (D-2), and search fuses two indexes by reciprocal
//! rank:
//!
//! - **DuckDB `fts_entities`** — BM25 over *every* searchable artifact, prose
//!   included. Titles and bodies of the current revision are joined in from
//!   Lance so one index covers the whole corpus.
//! - **Lance `documents`** — vector search over the embeddings.
//!
//! Together these satisfy REQ-4's "every artifact type that carries text".
//! Metrics and observations are excluded by design — they are numeric, and
//! reaching them is a filter rather than a query.
//!
//! # Why BM25 is DuckDB's job and not Lance's
//!
//! SPEC §5 put both halves inside `lance_hybrid_search`. Its keyword half
//! turned out not to be characterisable: on an un-indexed dataset, multi-term
//! queries match inconsistently — `"onboarding metering"` returns a document
//! containing only *metering*, while `"onboarding slow"` returns nothing at all
//! despite a document containing *onboarding*. The extension's documentation
//! shows only single-word examples and documents no way to build the index that
//! would presumably fix it.
//!
//! A search that returns plausible-but-wrong results is the same failure class
//! as an inverted graph traversal, so it gets the same answer: do it somewhere
//! the semantics are known. DuckDB's FTS extension is a real BM25 index with
//! documented behaviour. Lance keeps the job it is uniquely good at — the
//! vector index and the multimodal blobs — and `keel-core` was always doing the
//! cross-index fusion anyway. See DECISIONS B-12.
//!
//! # The stale-index trap
//!
//! DuckDB's FTS index is a *snapshot*: it does not track inserts. An index
//! built before a task was created simply will not return that task, and the
//! caller sees a confident empty result rather than an error. That is the same
//! failure shape as an inverted graph traversal, so it gets the same treatment
//! — [`DuckStore::refresh_entity_index`] rebuilds whenever the event log has
//! moved since the last build, and search always calls it first.

use super::rows::spec_for;
use super::{Blob, DocumentStore, Page, SearchHit, SearchQuery, SearchSource};
use crate::{
    Actor, BlobId, DocId, DocStatus, Document, DocumentDiff, DuckStore, EntityId, EntityType,
    Error, Result, Surface,
};
use chrono::{DateTime, Utc};
use duckdb::types::{TimeUnit, Value};
use duckdb::{Row, params_from_iter};

/// The reciprocal-rank-fusion constant.
///
/// 60 is the value from the original RRF paper and the usual default. It
/// controls how quickly a result's contribution decays with rank; at this
/// corpus size the choice barely matters, and picking the well-known value
/// means nobody has to wonder why it is 37.
const RRF_K: f64 = 60.0;

/// The columns of `lancedb.documents`, in insert order.
const DOC_COLS: &str = "doc_id, entity_type, entity_id, project_id, version, parent_version, \
                        title, body, body_hash, media_ref, status, author, session_id, surface, \
                        created_at, embedding, embedding_model, embedding_version";

impl DuckStore {
    /// Render an embedding as a SQL array literal.
    ///
    /// Interpolated rather than bound because DuckDB's parameter binding has
    /// no `FLOAT[384]` variant. Safe: every element is an `f32` this process
    /// produced and formats as a plain number, so there is no string from any
    /// caller anywhere in the result.
    fn embedding_literal(embedding: Option<&Vec<f32>>) -> String {
        match embedding {
            None => "NULL".to_owned(),
            Some(v) => {
                let parts: Vec<String> = v.iter().map(|x| format!("{x:?}")).collect();
                format!("[{}]::FLOAT[{}]", parts.join(","), v.len())
            }
        }
    }

    /// The highest revision number recorded for an entity, or zero.
    fn max_version(&self, entity_id: &EntityId) -> Result<i32> {
        let n: Option<i32> = self
            .connection()
            .query_row(
                "SELECT max(version) FROM lancedb.documents WHERE entity_id = ?",
                params_from_iter(vec![Value::Text(entity_id.as_str().to_owned())]),
                |r| r.get(0),
            )
            .map_err(Error::storage(format!(
                "find the latest revision of {entity_id}"
            )))?;
        Ok(n.unwrap_or(0))
    }

    /// Rebuild the entity keyword index if the event log has moved.
    ///
    /// Cheap to call: it compares one id and usually does nothing. The rebuild
    /// itself is a full pass over the non-prose entity tables, which is
    /// milliseconds at this scale and is not worth making incremental until a
    /// measurement says otherwise.
    pub fn refresh_entity_index(&self) -> Result<()> {
        let latest: Option<String> = self
            .connection()
            .query_row("SELECT max(id) FROM events", [], |r| r.get(0))
            .map_err(Error::storage("read the newest event id"))?;
        let latest = latest.unwrap_or_default();

        let indexed: Option<String> = self
            .connection()
            .query_row(
                "SELECT last_event_id FROM _keel_fts_state WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .ok()
            .flatten();

        if indexed.as_deref() == Some(latest.as_str()) {
            return Ok(());
        }

        // Every searchable type, prose included: the current revision's title
        // and body are joined in from the Lance dataset so one BM25 index
        // covers the whole corpus (DECISIONS B-12).
        //
        // These run as three separate statements, not one batch, and that is
        // load-bearing: `create_fts_index` is a macro that queries the target
        // table, and it cannot see a table created earlier in the same batch.
        // Batching them fails with "Table with name fts_entities does not
        // exist", which reads like a typo and is not one.
        let rebuild = "CREATE OR REPLACE TABLE fts_entities AS
             SELECT id, 'project' AS entity_type, id AS project_id, name AS label,
                    COALESCE(description, '') AS body FROM projects WHERE archived_at IS NULL
             UNION ALL
             SELECT id, 'milestone', project_id, name, COALESCE(summary, '')
                    FROM milestones WHERE archived_at IS NULL
             UNION ALL
             SELECT id, 'task', project_id, title, COALESCE(body, '')
                    FROM tasks WHERE archived_at IS NULL
             UNION ALL
             SELECT id, 'term', COALESCE(project_id, ''), term, definition
                    FROM terms WHERE archived_at IS NULL
             UNION ALL
             SELECT id, 'environment', project_id, name,
                    COALESCE(url, '') || ' ' || COALESCE(deployed_version, '')
                    FROM environments WHERE archived_at IS NULL
             UNION ALL
             SELECT id, 'artifact', project_id, name, COALESCE(url, '')
                    FROM artifacts WHERE archived_at IS NULL
             UNION ALL
             SELECT d.entity_id, d.entity_type, COALESCE(d.project_id, ''), d.title, d.body
                    FROM lancedb.documents d WHERE d.status = 'current';";

        self.connection()
            .execute_batch(rebuild)
            .map_err(Error::storage("rebuild the entity keyword index"))?;

        self.connection()
            .execute_batch(
                "PRAGMA create_fts_index('fts_entities', 'id', 'label', 'body', overwrite = 1);",
            )
            .map_err(Error::storage("build the BM25 index over the entity table"))?;

        self.connection()
            .execute_batch(&format!(
                "CREATE TABLE IF NOT EXISTS _keel_fts_state \
                   (id INTEGER PRIMARY KEY, last_event_id VARCHAR);
                 DELETE FROM _keel_fts_state WHERE id = 1;
                 INSERT INTO _keel_fts_state VALUES (1, '{}');",
                latest.replace('\'', "''")
            ))
            .map_err(Error::storage("record the entity index watermark"))
    }

    /// Semantic search over the Lance `documents` dataset.
    ///
    /// Vector only. The keyword half of document search lives in DuckDB — see
    /// [`DuckStore::search_keyword`] and DECISIONS B-12 for why.
    ///
    /// Returns an empty list when there is no embedder, which is the honest
    /// answer: without one there is no semantic half, and the keyword half
    /// still covers everything.
    fn search_semantic(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        let Some(embedder) = self.embedder() else {
            return Ok(Vec::new());
        };
        let vector = embedder.embed_one(&query.text)?;

        let dataset = self.root().join("lance").join("documents.lance");
        let dataset = dataset.display().to_string().replace('\'', "''");

        let mut clauses = vec!["status = 'current'".to_owned()];
        let mut params: Vec<Value> = Vec::new();
        if let Some(p) = &query.project_id {
            clauses.push("project_id = ?".to_owned());
            params.push(Value::Text(p.as_str().to_owned()));
        }
        if !query.entity_types.is_empty() {
            let list: Vec<String> = query
                .entity_types
                .iter()
                .filter(|t| t.has_document())
                .map(|t| format!("'{}'", t.as_str()))
                .collect();
            if list.is_empty() {
                return Ok(Vec::new());
            }
            clauses.push(format!("entity_type IN ({})", list.join(", ")));
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

        // Lower `_distance` is closer, so the ordering is ascending and the
        // score is negated on the way out — RRF only uses rank, but a caller
        // reading `score` should not see "smaller is better" without warning.
        let sql = format!(
            "SELECT entity_type, entity_id, project_id, title, body, _distance
             FROM (SELECT * FROM lance_vector_search('{dataset}', 'embedding', {}, k := {}))
             WHERE {}
             ORDER BY _distance ASC
             LIMIT {}",
            Self::embedding_literal(Some(&vector)),
            query.inner_limit(),
            clauses.join(" AND "),
            query.limit
        );

        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage("prepare the semantic search"))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage("run the semantic search"))?;

        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(Error::storage("read a semantic search hit"))?
        {
            let body: String = row.get("body").unwrap_or_default();
            out.push(SearchHit {
                entity_id: EntityId::parse(
                    &row.get::<_, String>("entity_id")
                        .map_err(Error::storage("read a hit's entity_id"))?,
                )?,
                entity_type: EntityType::parse(
                    &row.get::<_, String>("entity_type")
                        .map_err(Error::storage("read a hit's entity_type"))?,
                )?,
                project_id: match row.get::<_, Option<String>>("project_id").ok().flatten() {
                    Some(p) if !p.is_empty() => EntityId::parse(&p).ok(),
                    _ => None,
                },
                title: row.get::<_, String>("title").unwrap_or_default(),
                excerpt: excerpt(&body, &query.text),
                score: -row
                    .get::<_, Option<f64>>("_distance")
                    .ok()
                    .flatten()
                    .unwrap_or(0.0),
                source: SearchSource::Semantic,
            });
        }
        Ok(out)
    }

    /// Keyword search over every searchable artifact, prose included.
    fn search_keyword(&self, query: &SearchQuery) -> Result<Vec<SearchHit>> {
        self.refresh_entity_index()?;

        let mut clauses = vec!["score IS NOT NULL".to_owned()];
        let mut params: Vec<Value> = vec![Value::Text(query.text.clone())];
        if let Some(p) = &query.project_id {
            clauses.push("project_id = ?".to_owned());
            params.push(Value::Text(p.as_str().to_owned()));
        }
        if !query.entity_types.is_empty() {
            let list: Vec<String> = query
                .entity_types
                .iter()
                .filter(|t| t.is_searchable())
                .map(|t| format!("'{}'", t.as_str()))
                .collect();
            if list.is_empty() {
                return Ok(Vec::new());
            }
            clauses.push(format!("entity_type IN ({})", list.join(", ")));
        }

        let sql = format!(
            "SELECT id, entity_type, project_id, label, body, score FROM (
                 SELECT *, fts_main_fts_entities.match_bm25(id, ?) AS score FROM fts_entities
             ) WHERE {} ORDER BY score DESC LIMIT {}",
            clauses.join(" AND "),
            query.limit
        );

        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage("prepare the keyword search"))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage("run the keyword search"))?;

        let mut out = Vec::new();
        while let Some(row) = rows
            .next()
            .map_err(Error::storage("read a keyword search hit"))?
        {
            let body: String = row.get("body").unwrap_or_default();
            let label: String = row.get("label").unwrap_or_default();
            let entity_type = EntityType::parse(
                &row.get::<_, String>("entity_type")
                    .map_err(Error::storage("read a hit's entity_type"))?,
            )?;
            out.push(SearchHit {
                entity_id: EntityId::parse(
                    &row.get::<_, String>("id")
                        .map_err(Error::storage("read a hit's id"))?,
                )?,
                entity_type,
                project_id: match row.get::<_, Option<String>>("project_id").ok().flatten() {
                    Some(p) if !p.is_empty() => EntityId::parse(&p).ok(),
                    _ => None,
                },
                excerpt: excerpt(if body.is_empty() { &label } else { &body }, &query.text),
                title: label,
                score: row
                    .get::<_, Option<f64>>("score")
                    .ok()
                    .flatten()
                    .unwrap_or(0.0),
                source: SearchSource::Keyword,
            });
        }
        Ok(out)
    }
}

/// Fuse two ranked lists by reciprocal rank.
///
/// Raw scores from BM25 and from a vector index are not comparable — they are
/// not even on the same scale — so fusing on *rank* rather than score is the
/// only defensible way to merge them. A document found by both indexes gets
/// both contributions, which is exactly the behaviour wanted: agreement
/// between an independent keyword match and an independent semantic match is
/// the strongest signal available here.
fn reciprocal_rank_fusion(lists: Vec<Vec<SearchHit>>, limit: usize) -> Vec<SearchHit> {
    let mut fused: Vec<SearchHit> = Vec::new();

    for list in lists {
        for (rank, hit) in list.into_iter().enumerate() {
            let contribution = 1.0 / (RRF_K + rank as f64 + 1.0);
            match fused.iter_mut().find(|h| h.entity_id == hit.entity_id) {
                Some(existing) => {
                    existing.score += contribution;
                    if existing.source != hit.source {
                        existing.source = SearchSource::Both;
                    }
                }
                None => {
                    let mut hit = hit;
                    hit.score = contribution;
                    fused.push(hit);
                }
            }
        }
    }

    fused.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            // Ties break by id so results are stable between calls — an
            // unstable order makes snapshot tests flap and makes a human
            // wonder whether the data changed.
            .then_with(|| a.entity_id.cmp(&b.entity_id))
    });
    fused.truncate(limit);
    fused
}

/// A short window of `body` around the first query term that appears in it.
fn excerpt(body: &str, query: &str) -> String {
    const WIDTH: usize = 240;
    let lower = body.to_lowercase();
    let start = query
        .split_whitespace()
        .filter_map(|term| lower.find(&term.to_lowercase()))
        .min()
        .unwrap_or(0);

    // Back up to a character boundary and a little context.
    let mut begin = start.saturating_sub(60);
    while begin > 0 && !body.is_char_boundary(begin) {
        begin -= 1;
    }
    let mut end = (begin + WIDTH).min(body.len());
    while end < body.len() && !body.is_char_boundary(end) {
        end += 1;
    }

    let mut out = String::new();
    if begin > 0 {
        out.push('…');
    }
    out.push_str(body[begin..end].trim());
    if end < body.len() {
        out.push('…');
    }
    out
}

/// Rebuild a document from a row.
fn read_document(row: &Row<'_>) -> Result<Document> {
    let e = |c: &'static str| Error::storage(format!("read column `{c}` of `documents`"));
    Ok(Document {
        doc_id: DocId::parse(&row.get::<_, String>("doc_id").map_err(e("doc_id"))?)?,
        entity_type: EntityType::parse(
            &row.get::<_, String>("entity_type")
                .map_err(e("entity_type"))?,
        )?,
        entity_id: EntityId::parse(&row.get::<_, String>("entity_id").map_err(e("entity_id"))?)?,
        project_id: match row
            .get::<_, Option<String>>("project_id")
            .map_err(e("project_id"))?
        {
            Some(p) if !p.is_empty() => Some(EntityId::parse(&p)?),
            _ => None,
        },
        version: row.get::<_, i32>("version").map_err(e("version"))?,
        parent_version: row
            .get::<_, Option<i32>>("parent_version")
            .map_err(e("parent_version"))?,
        title: row.get::<_, String>("title").map_err(e("title"))?,
        body: row.get::<_, String>("body").map_err(e("body"))?,
        body_hash: row.get::<_, String>("body_hash").map_err(e("body_hash"))?,
        media_ref: row
            .get::<_, Option<String>>("media_ref")
            .map_err(e("media_ref"))?,
        status: DocStatus::parse(&row.get::<_, String>("status").map_err(e("status"))?)?,
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
        // The vector is deliberately not read back. It is 384 floats per row
        // that no caller has ever needed, and reading it would make listing a
        // document's history cost more than the history is worth.
        embedding: None,
        embedding_model: row
            .get::<_, Option<String>>("embedding_model")
            .map_err(e("embedding_model"))?
            .unwrap_or_default(),
        embedding_version: row
            .get::<_, Option<i32>>("embedding_version")
            .map_err(e("embedding_version"))?
            .unwrap_or(0),
    })
}

/// The `SELECT` list for reading documents back.
const DOC_SELECT: &str = "SELECT doc_id, entity_type, entity_id, project_id, version, \
                          parent_version, title, body, body_hash, media_ref, status, author, \
                          session_id, surface, created_at, embedding_model, embedding_version \
                          FROM lancedb.documents";

impl DocumentStore for DuckStore {
    fn write_revision(&mut self, mut document: Document) -> Result<Document> {
        // The cross-engine invariant: Lance cannot enforce that `entity_id`
        // points at a real DuckDB row, so `keel-core` does it here. Without
        // this, a typo produces a document nothing can ever reach.
        let spec = spec_for(document.entity_type);
        let exists: i64 = self
            .connection()
            .query_row(
                &format!("SELECT count(*) FROM {} WHERE id = ?", spec.table),
                params_from_iter(vec![Value::Text(document.entity_id.as_str().to_owned())]),
                |r| r.get(0),
            )
            .map_err(Error::storage(format!(
                "check that {} exists before writing its document",
                document.entity_id
            )))?;
        if exists == 0 {
            return Err(Error::Invariant {
                operation: format!("write a revision of {}", document.entity_id),
                problem: format!(
                    "no {} exists with that id; create the entity before writing its body",
                    document.entity_type
                ),
            });
        }

        // Identical content is not a new revision. The mirror hook in §8.1
        // regenerates a file and re-reads it, so without this every no-op save
        // would grow the history by one.
        if let Some(current) = self.revision(&document.entity_id, None)?
            && current.body_hash == document.body_hash
        {
            return Ok(current);
        }

        let previous = self.max_version(&document.entity_id)?;
        document.version = previous + 1;
        document.parent_version = if previous == 0 { None } else { Some(previous) };
        document.status = DocStatus::Current;

        if document.embedding.is_none()
            && let Some(embedder) = self.embedder()
        {
            match embedder.embed_one(&document.searchable_text()) {
                Ok(v) => {
                    document.embedding = Some(v);
                    document.embedding_model = embedder.model_name().to_owned();
                }
                // A failed embed must not lose the write. The document stays
                // readable and keyword-searchable; a later re-embed pass picks
                // it up, which is what `embedding_version` is for.
                Err(e) => tracing::warn!(
                    entity_id = %document.entity_id,
                    error = %e,
                    "embedding failed; storing the revision without a vector"
                ),
            }
        }

        // Demote the previous current revision first, so there is never a
        // window with two.
        self.connection()
            .execute(
                "UPDATE lancedb.documents SET status = 'superseded' \
                 WHERE entity_id = ? AND status = 'current'",
                params_from_iter(vec![Value::Text(document.entity_id.as_str().to_owned())]),
            )
            .map_err(Error::storage(format!(
                "supersede the previous revision of {}",
                document.entity_id
            )))?;

        let sql = format!(
            "INSERT INTO lancedb.documents ({DOC_COLS}) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, {}, ?, ?)",
            Self::embedding_literal(document.embedding.as_ref())
        );

        let params: Vec<Value> = vec![
            Value::Text(document.doc_id.as_str().to_owned()),
            Value::Text(document.entity_type.as_str().to_owned()),
            Value::Text(document.entity_id.as_str().to_owned()),
            document
                .project_id
                .as_ref()
                .map(|p| Value::Text(p.as_str().to_owned()))
                .unwrap_or(Value::Null),
            Value::Int(document.version),
            document
                .parent_version
                .map(Value::Int)
                .unwrap_or(Value::Null),
            Value::Text(document.title.clone()),
            Value::Text(document.body.clone()),
            Value::Text(document.body_hash.clone()),
            document
                .media_ref
                .as_ref()
                .map(|m| Value::Text(m.clone()))
                .unwrap_or(Value::Null),
            Value::Text(document.status.as_str().to_owned()),
            Value::Text(document.author.as_str().to_owned()),
            document
                .session_id
                .as_ref()
                .map(|s| Value::Text(s.clone()))
                .unwrap_or(Value::Null),
            document
                .surface
                .map(|s| Value::Text(s.as_str().to_owned()))
                .unwrap_or(Value::Null),
            Value::Timestamp(
                TimeUnit::Microsecond,
                document.created_at.timestamp_micros(),
            ),
            Value::Text(document.embedding_model.clone()),
            Value::Int(document.embedding_version),
        ];

        self.connection()
            .execute(&sql, params_from_iter(params))
            .map_err(Error::storage(format!(
                "write revision {} of {}",
                document.version, document.entity_id
            )))?;

        // Advance the header's pointer so the relational and columnar halves
        // agree. `fsck` checks exactly this.
        self.connection()
            .execute(
                &format!(
                    "UPDATE {} SET current_doc_version = ? WHERE id = ?",
                    spec.table
                ),
                params_from_iter(vec![
                    Value::Int(document.version),
                    Value::Text(document.entity_id.as_str().to_owned()),
                ]),
            )
            .map_err(Error::storage(format!(
                "advance current_doc_version on {}",
                document.entity_id
            )))?;

        Ok(document)
    }

    fn revision(&self, entity_id: &EntityId, version: Option<i32>) -> Result<Option<Document>> {
        let (clause, params): (&str, Vec<Value>) = match version {
            Some(v) => (
                "WHERE entity_id = ? AND version = ?",
                vec![Value::Text(entity_id.as_str().to_owned()), Value::Int(v)],
            ),
            None => (
                "WHERE entity_id = ? ORDER BY version DESC LIMIT 1",
                vec![Value::Text(entity_id.as_str().to_owned())],
            ),
        };
        let sql = format!("{DOC_SELECT} {clause}");
        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage(format!(
                "prepare a revision read for {entity_id}"
            )))?;
        let mut rows = stmt
            .query(params_from_iter(params))
            .map_err(Error::storage(format!("read a revision of {entity_id}")))?;
        match rows.next().map_err(Error::storage("read a document row"))? {
            Some(row) => Ok(Some(read_document(row)?)),
            None => Ok(None),
        }
    }

    fn revisions(&self, entity_id: &EntityId) -> Result<Vec<Document>> {
        let sql = format!("{DOC_SELECT} WHERE entity_id = ? ORDER BY version ASC");
        let mut stmt = self
            .connection()
            .prepare(&sql)
            .map_err(Error::storage(format!(
                "prepare a history read for {entity_id}"
            )))?;
        let mut rows = stmt
            .query(params_from_iter(vec![Value::Text(
                entity_id.as_str().to_owned(),
            )]))
            .map_err(Error::storage(format!("read the history of {entity_id}")))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(Error::storage("read a document row"))? {
            out.push(read_document(row)?);
        }
        Ok(out)
    }

    fn diff(&self, entity_id: &EntityId, from: i32, to: i32) -> Result<DocumentDiff> {
        let fetch = |v: i32| -> Result<Document> {
            self.revision(entity_id, Some(v))?
                .ok_or_else(|| Error::Invalid {
                    entity_type: entity_id.entity_type(),
                    field: "version".to_owned(),
                    problem: format!("{entity_id} has no revision {v}"),
                    expected:
                        "a revision number returned by keel_get, or omit it for the current one"
                            .to_owned(),
                })
        };
        let a = fetch(from)?;
        let b = fetch(to)?;

        let diff = similar::TextDiff::from_lines(&a.body, &b.body);
        let mut unified = String::new();
        let (mut added, mut removed) = (0usize, 0usize);
        for change in diff.iter_all_changes() {
            let sign = match change.tag() {
                similar::ChangeTag::Delete => {
                    removed += 1;
                    '-'
                }
                similar::ChangeTag::Insert => {
                    added += 1;
                    '+'
                }
                similar::ChangeTag::Equal => ' ',
            };
            unified.push(sign);
            unified.push_str(change.value());
            if !change.value().ends_with('\n') {
                unified.push('\n');
            }
        }

        Ok(DocumentDiff {
            entity_id: entity_id.clone(),
            from_version: from,
            to_version: to,
            unified,
            added,
            removed,
        })
    }

    fn search(&self, query: &SearchQuery) -> Result<Page<SearchHit>> {
        if query.text.trim().is_empty() {
            return Err(Error::Invalid {
                entity_type: EntityType::Artifact,
                field: "query".to_owned(),
                problem: "the search text is empty".to_owned(),
                expected: "some words to search for; to list entities without searching, \
                           use keel_get or keel_context instead"
                    .to_owned(),
            });
        }

        // Any one index can legitimately fail — an empty Lance dataset has no
        // FTS index to consult, for instance — and one failing must not take
        // out the others. A search that returns part of the story is far more
        // useful than one that returns an error.
        let mut lists = Vec::new();
        lists.push(self.search_keyword(query).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "keyword search failed; returning semantic hits only");
            Vec::new()
        }));
        lists.push(self.search_semantic(query).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "semantic search failed; returning keyword hits only");
            Vec::new()
        }));

        // `total` counts distinct artifacts, not raw hits: the same document
        // legitimately appears in both the title and body lists, and reporting
        // it twice would make `truncated` lie.
        let distinct: std::collections::HashSet<_> = lists
            .iter()
            .flatten()
            .map(|h| h.entity_id.clone())
            .collect();
        let total = distinct.len();
        let fused = reciprocal_rank_fusion(lists, query.limit);
        Ok(Page {
            truncated: total > fused.len(),
            total,
            items: fused,
        })
    }

    fn put_blob(&mut self, blob: Blob) -> Result<BlobId> {
        self.connection()
            .execute(
                "INSERT INTO lancedb.blobs (blob_id, entity_id, project_id, media_type, \
                 byte_length, sha256, bytes, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                params_from_iter(vec![
                    Value::Text(blob.blob_id.as_str().to_owned()),
                    blob.entity_id
                        .as_ref()
                        .map(|e| Value::Text(e.as_str().to_owned()))
                        .unwrap_or(Value::Null),
                    blob.project_id
                        .as_ref()
                        .map(|p| Value::Text(p.as_str().to_owned()))
                        .unwrap_or(Value::Null),
                    Value::Text(blob.media_type.clone()),
                    Value::BigInt(blob.bytes.len() as i64),
                    Value::Text(blob.sha256.clone()),
                    Value::Blob(blob.bytes.clone()),
                    Value::Timestamp(TimeUnit::Microsecond, blob.created_at.timestamp_micros()),
                ]),
            )
            .map_err(Error::storage(format!("store the blob {}", blob.blob_id)))?;
        Ok(blob.blob_id)
    }

    fn get_blob(&self, blob_id: &BlobId) -> Result<Option<Blob>> {
        let mut stmt = self
            .connection()
            .prepare(
                "SELECT blob_id, entity_id, project_id, media_type, sha256, bytes, created_at \
                 FROM lancedb.blobs WHERE blob_id = ?",
            )
            .map_err(Error::storage("prepare a blob read"))?;
        let mut rows = stmt
            .query(params_from_iter(vec![Value::Text(
                blob_id.as_str().to_owned(),
            )]))
            .map_err(Error::storage(format!("read the blob {blob_id}")))?;

        let e = |c: &'static str| Error::storage(format!("read column `{c}` of `blobs`"));
        match rows.next().map_err(Error::storage("read a blob row"))? {
            Some(row) => Ok(Some(Blob {
                blob_id: BlobId::parse(&row.get::<_, String>("blob_id").map_err(e("blob_id"))?)?,
                entity_id: match row.get::<_, Option<String>>("entity_id").ok().flatten() {
                    Some(x) => Some(EntityId::parse(&x)?),
                    None => None,
                },
                project_id: match row.get::<_, Option<String>>("project_id").ok().flatten() {
                    Some(x) => Some(EntityId::parse(&x)?),
                    None => None,
                },
                media_type: row
                    .get::<_, String>("media_type")
                    .map_err(e("media_type"))?,
                sha256: row.get::<_, String>("sha256").map_err(e("sha256"))?,
                bytes: row.get::<_, Vec<u8>>("bytes").map_err(e("bytes"))?,
                created_at: row
                    .get::<_, DateTime<Utc>>("created_at")
                    .map_err(e("created_at"))?,
            })),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn hit(id: &str, source: SearchSource) -> SearchHit {
        SearchHit {
            entity_id: EntityId::parse(id).unwrap(),
            entity_type: EntityType::Task,
            project_id: None,
            title: id.to_owned(),
            excerpt: String::new(),
            score: 0.0,
            source,
        }
    }

    #[test]
    fn fusion_rewards_agreement_between_the_two_indexes() {
        let a = EntityId::generate(EntityType::Task);
        let b = EntityId::generate(EntityType::Task);

        // `a` is second in one list and first in the other; `b` is first in
        // one list only. Agreement should carry `a` to the top.
        let docs = vec![
            hit(b.as_str(), SearchSource::Semantic),
            hit(a.as_str(), SearchSource::Semantic),
        ];
        let ents = vec![hit(a.as_str(), SearchSource::Keyword)];

        let fused = reciprocal_rank_fusion(vec![docs, ents], 10);
        assert_eq!(fused[0].entity_id, a);
        assert_eq!(
            fused[0].source,
            SearchSource::Both,
            "a hit found by both indexes should say so"
        );
        assert_eq!(fused[1].entity_id, b);
    }

    #[test]
    fn fusion_is_stable_for_equal_scores() {
        let a = EntityId::generate(EntityType::Task);
        let b = EntityId::generate(EntityType::Task);
        let one = reciprocal_rank_fusion(
            vec![
                vec![hit(a.as_str(), SearchSource::Semantic)],
                vec![hit(b.as_str(), SearchSource::Keyword)],
            ],
            10,
        );
        let two = reciprocal_rank_fusion(
            vec![
                vec![hit(a.as_str(), SearchSource::Semantic)],
                vec![hit(b.as_str(), SearchSource::Keyword)],
            ],
            10,
        );
        assert_eq!(
            one.iter().map(|h| h.entity_id.clone()).collect::<Vec<_>>(),
            two.iter().map(|h| h.entity_id.clone()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn fusion_respects_the_limit() {
        let hits: Vec<SearchHit> = (0..10)
            .map(|_| {
                hit(
                    EntityId::generate(EntityType::Task).as_str(),
                    SearchSource::Semantic,
                )
            })
            .collect();
        assert_eq!(reciprocal_rank_fusion(vec![hits], 3).len(), 3);
    }

    #[test]
    fn an_excerpt_centres_on_the_match() {
        let body = "The quick brown fox jumps over the lazy dog. ".repeat(20);
        let e = excerpt(&body, "lazy");
        assert!(e.contains("lazy"), "{e}");
        assert!(e.len() <= 260, "excerpt should be bounded: {}", e.len());
    }

    #[test]
    fn an_excerpt_of_short_text_is_the_whole_text_without_ellipses() {
        assert_eq!(
            excerpt("Onboarding is slow", "onboarding"),
            "Onboarding is slow"
        );
    }

    #[test]
    fn an_excerpt_never_splits_a_multibyte_character() {
        let body = "héllo wörld ".repeat(50);
        let e = excerpt(&body, "wörld");
        assert!(e.contains("wörld"), "{e}");
    }

    #[test]
    fn an_embedding_literal_is_a_typed_array() {
        let lit = DuckStore::embedding_literal(Some(&vec![0.5, -0.25]));
        assert!(lit.starts_with('['), "{lit}");
        assert!(lit.ends_with("::FLOAT[2]"), "{lit}");
        assert_eq!(DuckStore::embedding_literal(None), "NULL");
    }
}
