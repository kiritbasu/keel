//! Executing a tool call against `keel-core`.
//!
//! This is the only place that knows both the MCP argument shapes and the
//! domain API. `keel-core` stays ignorant of MCP, and the daemon stays
//! ignorant of what the tools mean — it owns the socket and the lock, nothing
//! more.
//!
//! # Errors are part of the surface
//!
//! A validation error here is read by a model that has to work out what to
//! send instead. `keel-core` already builds those messages carefully; this
//! layer's job is to not flatten them into "bad request", and to map a stale
//! update onto the 409 payload from SPEC §7.3 — current state plus the events
//! since the caller's read — so an agent can usually merge rather than give up.

use crate::context;
use crate::protocol::{RpcError, codes};
use keel_core::{
    Actor, Cursor, Direction, DocumentStore, Entity, EntityId, EntityQuery, EntityStore,
    EntityType, Error, EventId, GraphStore, NewLink, Provenance, Relation, SearchQuery, Surface,
};
use keel_core::{
    Artifact, Decision, Design, DuckStore, Environment, Feedback, Metric, MetricObservation,
    Milestone, Project, Question, Spec, Task, Term,
};
use serde_json::{Map, Value, json};

/// One tool invocation.
pub struct ToolCall<'a> {
    /// The tool name.
    pub name: &'a str,
    /// The `arguments` object.
    pub arguments: &'a Value,
}

/// Turn a domain error into a JSON-RPC error, preserving what the caller needs.
pub fn to_rpc_error(store: &DuckStore, err: Error) -> RpcError {
    match &err {
        Error::StaleVersion { id, latest, .. } => {
            // SPEC §7.3. Returning the current state and the events since the
            // caller's read is what lets an agent resolve the conflict itself
            // rather than clobbering or giving up.
            let current_state = EntityId::parse(id)
                .ok()
                .and_then(|i| store.get(&i).ok().flatten())
                .map(|e| entity_json(&e));

            let events_since = EntityId::parse(id)
                .ok()
                .and_then(|i| {
                    store
                        .events(&Cursor::Beginning, None, 500)
                        .ok()
                        .map(|page| (i, page))
                })
                .map(|(i, page)| {
                    page.items
                        .into_iter()
                        .filter(|e| e.entity_id == i)
                        .rev()
                        .take(20)
                        .collect::<Vec<_>>()
                })
                .and_then(|evts| serde_json::to_value(evts).ok());

            RpcError::new(codes::CONFLICT, err.to_string()).with_data(json!({
                "latest_version": latest,
                "current_state": current_state,
                "events_since": events_since,
            }))
        }
        e if e.is_caller_error() => RpcError::new(codes::INVALID_PARAMS, err.to_string()),
        _ => RpcError::new(codes::INTERNAL_ERROR, err.to_string()),
    }
}

/// A missing or malformed argument.
fn bad_arg(field: &str, problem: &str, expected: &str) -> RpcError {
    RpcError::new(
        codes::INVALID_PARAMS,
        format!("argument `{field}`: {problem}. Expected: {expected}"),
    )
}

/// Read a required string argument.
fn req_str(args: &Value, field: &str) -> Result<String, RpcError> {
    args.get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| bad_arg(field, "missing or not a string", "a string"))
}

/// Read an optional string argument.
fn opt_str(args: &Value, field: &str) -> Option<String> {
    args.get(field).and_then(Value::as_str).map(str::to_owned)
}

/// Read an optional integer argument.
fn opt_i64(args: &Value, field: &str) -> Option<i64> {
    args.get(field).and_then(Value::as_i64)
}

/// Read an optional boolean argument.
fn opt_bool(args: &Value, field: &str) -> bool {
    args.get(field).and_then(Value::as_bool).unwrap_or(false)
}

/// Read an optional timestamp argument.
fn opt_time(args: &Value, field: &str) -> Result<Option<chrono::DateTime<chrono::Utc>>, RpcError> {
    match args.get(field).and_then(Value::as_str) {
        None => Ok(None),
        Some(raw) => chrono::DateTime::parse_from_rfc3339(raw)
            .map(|t| Some(t.with_timezone(&chrono::Utc)))
            .map_err(|_| {
                bad_arg(
                    field,
                    &format!("`{raw}` is not a timestamp"),
                    "an RFC 3339 timestamp such as 2026-08-09T14:22:01Z",
                )
            }),
    }
}

/// Build provenance from the ambient arguments.
///
/// The default actor is `claude`: this is the MCP transport, and SPEC §6.5
/// says to fall back to the transport's identity rather than refusing a write.
/// Losing attribution is bad; refusing the write is worse.
pub fn provenance_from(args: &Value) -> Result<Provenance, RpcError> {
    let surface = match opt_str(args, "surface") {
        None => None,
        Some(s) => Some(
            Surface::parse(&s).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?,
        ),
    };
    Ok(Provenance {
        actor: Actor::Claude,
        session_id: opt_str(args, "session_id"),
        surface,
    })
}

/// Resolve a project reference — id, slug or name — to an id.
pub fn resolve_project(store: &DuckStore, reference: &str) -> Result<EntityId, RpcError> {
    if let Ok(id) = EntityId::parse_as(reference, EntityType::Project)
        && store.get(&id).ok().flatten().is_some()
    {
        return Ok(id);
    }

    let candidates = store
        .list(&EntityQuery::default().of_type(EntityType::Project))
        .map_err(|e| to_rpc_error(store, e))?;

    let needle = reference.to_lowercase();
    let matched = candidates.items.iter().find(|p| match p {
        Entity::Project(pr) => {
            pr.slug.eq_ignore_ascii_case(reference)
                || pr.name.to_lowercase() == needle
                || pr.aliases.iter().any(|a| a.to_lowercase() == needle)
        }
        _ => false,
    });

    match matched {
        Some(p) => Ok(p.id().clone()),
        None => Err(bad_arg(
            "project",
            &format!("no project matches `{reference}`"),
            &format!(
                "one of: {}. Call keel_projects to list them",
                candidates
                    .items
                    .iter()
                    .filter_map(|p| match p {
                        Entity::Project(pr) => Some(pr.slug.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

/// Serialise an entity for the wire, lifting the concurrency version to the
/// top level.
///
/// Public because *every* surface must use it. `/api/entities` once serialised
/// entities directly and `/api/entity/{id}` went through here, so the same
/// field appeared in two shapes depending on which endpoint you asked — which
/// is the kind of inconsistency a caller discovers at the worst moment.
///
/// `version` lives inside the audit block in the domain model, which is right
/// there and wrong here: `keel_update` documents a `version` argument, and an
/// agent that has just read an entity should be able to copy the field of that
/// name straight across. Making it hunt inside `audit` for it is the kind of
/// papercut that turns into a 409 and a confused retry.
///
/// The audit block is left intact — this adds a field, it does not move one.
pub fn entity_json(entity: &Entity) -> Value {
    let mut value = serde_json::to_value(entity).unwrap_or(Value::Null);
    if let Some(obj) = value.as_object_mut() {
        obj.insert("version".to_owned(), json!(entity.audit().version));
        obj.insert("archived".to_owned(), json!(entity.audit().is_archived()));
    }
    value
}

/// Wrap a structured result in the `tools/call` content shape.
///
/// Both halves are populated on purpose: `structuredContent` for a client that
/// can use it, and a text rendering for one that cannot — and for the model,
/// which reads the text.
fn tool_result(summary: String, structured: Value) -> Value {
    json!({
        "content": [{ "type": "text", "text": summary }],
        "structuredContent": structured,
        "isError": false,
    })
}

/// Execute a tool call.
pub fn dispatch(store: &mut DuckStore, call: ToolCall<'_>) -> Result<Value, RpcError> {
    let args = call.arguments;
    match call.name {
        "keel_context" => keel_context(store, args),
        "keel_search" => keel_search(store, args),
        "keel_get" => keel_get(store, args),
        "keel_projects" => keel_projects(store, args),
        "keel_activity" => keel_activity(store, args),
        "keel_create" => keel_create(store, args),
        "keel_update" => keel_update(store, args),
        "keel_write_doc" => keel_write_doc(store, args),
        "keel_link" => keel_link(store, args),
        other => Err(RpcError::new(
            codes::METHOD_NOT_FOUND,
            format!(
                "no tool named `{other}`. Available: {}",
                crate::tools::all()
                    .iter()
                    .map(|t| t.name)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn keel_context(store: &DuckStore, args: &Value) -> Result<Value, RpcError> {
    // `cwd` resolves to a project by its recorded `root_path`, and — more
    // importantly — says plainly when nothing matches. TQ-17: nine of ten gate
    // sessions called this, saw a roll-up listing some *other* project, and
    // had to work out for themselves that the thing they were working on was
    // simply absent. One of them said so outright and wrote nothing.
    let cwd = opt_str(args, "cwd");
    let matched_by_cwd = cwd.as_deref().and_then(|d| project_for_directory(store, d));

    let project = match opt_str(args, "project") {
        Some(p) => Some(resolve_project(store, &p)?),
        None => matched_by_cwd.clone(),
    };
    let depth = context::Depth::parse(opt_str(args, "depth").as_deref().unwrap_or("standard"))
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e))?;
    let since = opt_time(args, "since")?;

    let digest = context::build(store, project.as_ref(), depth, since)
        .map_err(|e| to_rpc_error(store, e))?;

    let unmatched = cwd.as_deref().filter(|_| matched_by_cwd.is_none());

    let mut summary = digest.to_prose();
    if let Some(dir) = unmatched {
        // Stated before the digest, not after: an agent that reads "here is
        // project X" first has already decided what it is looking at.
        summary = format!(
            "**No project in Keel matches `{dir}`.** If you are working on something \
             that belongs here, create it — `keel_create(type: \"project\", title: …, \
             fields: {{\"root_path\": \"{dir}\"}})` — and say that you did. Creating the \
             *first* project for a directory is not the duplicate-project failure; \
             creating a second one for a project that already exists is.\n\n{summary}"
        );
    }

    let mut structured = serde_json::to_value(&digest)
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))?;

    // Echo the session back so a long conversation can self-check that it is
    // still threading correctly (SPEC §6.5).
    if let Some(obj) = structured.as_object_mut() {
        obj.insert(
            "session_id".to_owned(),
            opt_str(args, "session_id")
                .map(Value::String)
                .unwrap_or(Value::Null),
        );
        if let Some(dir) = cwd.as_deref() {
            obj.insert(
                "directory".to_owned(),
                json!({
                    "path": dir,
                    "matched_project": matched_by_cwd.as_ref().map(|p| p.to_string()),
                }),
            );
        }
    }
    Ok(tool_result(summary, structured))
}

/// Normalise a filesystem path for comparison.
///
/// Collapses repeated separators and drops a trailing one. Found by a gate
/// session, which reported `matched_project: null` for a directory that plainly
/// had a project: the caller's `cwd` contained `T//keel-gate` and the stored
/// `root_path` contained `T/keel-gate`. A naive prefix comparison called those
/// different directories, so the session started unoriented and said so — and
/// the only reason anyone noticed is that it mentioned the null in its reply.
///
/// Not canonicalisation: this must not touch the filesystem. `cwd` may name a
/// directory this process cannot see, and a lookup that silently returns
/// nothing for an unreadable path would be the same class of quiet wrong
/// answer.
fn normalise_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    let mut last_was_sep = false;
    for c in path.chars() {
        if c == '/' {
            if !last_was_sep {
                out.push(c);
            }
            last_was_sep = true;
        } else {
            out.push(c);
            last_was_sep = false;
        }
    }
    while out.len() > 1 && out.ends_with('/') {
        out.pop();
    }
    out
}

/// The project whose checkout contains `dir`, if any.
///
/// Longest `root_path` wins, so a project nested inside another checkout
/// resolves to the inner one rather than whichever happened to be listed first.
fn project_for_directory(store: &DuckStore, dir: &str) -> Option<EntityId> {
    let dir = normalise_path(dir);
    let dir = dir.as_str();
    let page = store
        .list(
            &EntityQuery::default()
                .of_type(EntityType::Project)
                .limited(500),
        )
        .ok()?;
    let mut best: Option<(usize, EntityId)> = None;
    for entity in page.items {
        let Entity::Project(p) = entity else { continue };
        let Some(root) = p.root_path.as_deref() else {
            continue;
        };
        let root_trimmed = normalise_path(root);
        let root_trimmed = root_trimmed.as_str();
        if dir == root_trimmed || dir.starts_with(&format!("{root_trimmed}/")) {
            let len = root_trimmed.len();
            if best.as_ref().is_none_or(|(best_len, _)| len > *best_len) {
                best = Some((len, p.id));
            }
        }
    }
    best.map(|(_, id)| id)
}

fn keel_search(store: &DuckStore, args: &Value) -> Result<Value, RpcError> {
    let text = req_str(args, "query")?;
    let project_id = match opt_str(args, "project") {
        Some(p) => Some(resolve_project(store, &p)?),
        None => None,
    };
    let entity_types = parse_types(args.get("types"))?;

    let query = SearchQuery {
        text,
        project_id,
        entity_types,
        since: opt_time(args, "since")?,
        until: opt_time(args, "until")?,
        limit: opt_i64(args, "limit").unwrap_or(20).clamp(1, 100) as usize,
    };

    let page = store.search(&query).map_err(|e| to_rpc_error(store, e))?;
    let summary = if page.items.is_empty() {
        format!(
            "No matches for “{}”. Try fewer words, or drop the type filter.",
            query.text
        )
    } else {
        let mut lines = vec![format!(
            "{} match(es) for “{}”:",
            page.items.len(),
            query.text
        )];
        for hit in &page.items {
            lines.push(format!(
                "  [{}] {} — {}\n      {}",
                hit.entity_type, hit.title, hit.entity_id, hit.excerpt
            ));
        }
        if page.truncated {
            lines.push(format!(
                "  … {} more not shown",
                page.total - page.items.len()
            ));
        }
        lines.join("\n")
    };

    Ok(tool_result(
        summary,
        json!({
            "hits": page.items,
            "total": page.total,
            "truncated": page.truncated,
        }),
    ))
}

fn keel_get(store: &DuckStore, args: &Value) -> Result<Value, RpcError> {
    let ids: Vec<String> = args
        .get("ids")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .ok_or_else(|| bad_arg("ids", "missing", "an array of prefixed ULIDs"))?;
    if ids.is_empty() {
        return Err(bad_arg("ids", "empty", "at least one id"));
    }

    let include_body = args
        .get("include_body")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let depth = opt_i64(args, "depth").unwrap_or(0).clamp(0, 16) as u8;
    let direction = Direction::parse(opt_str(args, "direction").as_deref().unwrap_or("both"))
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let rels = parse_rels(args.get("rels"))?;
    let version = opt_i64(args, "version").map(|v| v as i32);
    let diff_against = opt_i64(args, "diff_against").map(|v| v as i32);

    let mut found = Vec::new();
    let mut missing = Vec::new();

    for raw in &ids {
        let id = EntityId::parse(raw)
            .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
        let Some(entity) = store.get(&id).map_err(|e| to_rpc_error(store, e))? else {
            missing.push(raw.clone());
            continue;
        };

        let mut item = json!({ "entity": entity_json(&entity) });

        if include_body && id.entity_type().has_document() {
            let doc = store
                .revision(&id, version)
                .map_err(|e| to_rpc_error(store, e))?;
            item["document"] = serde_json::to_value(&doc).unwrap_or(Value::Null);

            if let Some(other) = diff_against {
                let from = version.unwrap_or_else(|| doc.as_ref().map(|d| d.version).unwrap_or(1));
                let diff = store
                    .diff(&id, other.min(from), other.max(from))
                    .map_err(|e| to_rpc_error(store, e))?;
                item["diff"] = serde_json::to_value(&diff).unwrap_or(Value::Null);
            }
        }

        if depth > 0 {
            let neighbours = store
                .neighbours(&id, direction, &rels, depth)
                .map_err(|e| to_rpc_error(store, e))?;
            item["neighbours"] = serde_json::to_value(&neighbours).unwrap_or(Value::Null);
        }

        found.push(item);
    }

    let mut summary = format!("{} artifact(s):", found.len());
    for item in &found {
        if let Some(e) = item.get("entity") {
            summary.push_str(&format!(
                "\n  [{}] {} — {}",
                e.get("type").and_then(Value::as_str).unwrap_or("?"),
                label_of(e),
                e.get("id").and_then(Value::as_str).unwrap_or("?"),
            ));
        }
    }
    if !missing.is_empty() {
        // Not silent: an agent given fewer artifacts than it asked for, with
        // no indication, will assume the missing ones do not exist.
        summary.push_str(&format!("\n  not found: {}", missing.join(", ")));
    }

    Ok(tool_result(
        summary,
        json!({ "artifacts": found, "not_found": missing }),
    ))
}

fn keel_projects(store: &DuckStore, args: &Value) -> Result<Value, RpcError> {
    let include_archived = opt_bool(args, "include_archived");
    let page = store
        .list(&EntityQuery {
            include_archived,
            ..EntityQuery::default().of_type(EntityType::Project)
        })
        .map_err(|e| to_rpc_error(store, e))?;

    let projects: Vec<&Project> = page
        .items
        .iter()
        .filter_map(|e| match e {
            Entity::Project(p) => Some(p),
            _ => None,
        })
        .collect();

    let Some(query) = opt_str(args, "query") else {
        let summary = if projects.is_empty() {
            "No projects yet.".to_owned()
        } else {
            let mut lines = vec![format!("{} project(s):", projects.len())];
            for p in &projects {
                lines.push(format!("  {} ({}) — {}", p.name, p.slug, p.status));
            }
            lines.join("\n")
        };
        return Ok(tool_result(summary, json!({ "projects": projects })));
    };

    // Fuzzy match on name, slug, aliases and repo URL — §6.4's disambiguation
    // surface. Scored rather than boolean so near-misses still surface: the
    // failure being prevented is a *second* project for something that already
    // exists, and a near-miss is exactly when that happens.
    let needle = query.to_lowercase();
    let mut scored: Vec<(u32, &&Project)> = projects
        .iter()
        .filter_map(|p| {
            let score = match_score(&needle, p);
            (score > 0).then_some((score, p))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));

    let candidates: Vec<&Project> = scored.iter().map(|(_, p)| **p).collect();
    let exact = scored.first().is_some_and(|(s, _)| *s >= 100);
    let requires_confirmation = !candidates.is_empty() && !exact;

    let summary = if candidates.is_empty() {
        format!(
            "No project matches “{query}”. If you are about to create one, confirm the name \
             with the human first."
        )
    } else if exact {
        format!("“{query}” resolves to {}.", candidates[0].slug)
    } else {
        let mut lines = vec![format!(
            "“{query}” did not match exactly, but these are close. Ask the human before \
             creating a new project:"
        )];
        for p in &candidates {
            lines.push(format!("  {} ({})", p.name, p.slug));
        }
        lines.join("\n")
    };

    Ok(tool_result(
        summary,
        json!({
            "projects": candidates,
            "requires_confirmation": requires_confirmation,
        }),
    ))
}

/// Score a project against a query. 100+ means an exact identifier match.
fn match_score(needle: &str, p: &Project) -> u32 {
    if p.slug.to_lowercase() == needle || p.name.to_lowercase() == needle {
        return 120;
    }
    if p.aliases.iter().any(|a| a.to_lowercase() == needle) {
        return 110;
    }
    if p.repo_urls
        .iter()
        .any(|u| u.to_lowercase().contains(needle))
    {
        return 90;
    }
    if p.slug.to_lowercase().contains(needle) || p.name.to_lowercase().contains(needle) {
        return 60;
    }
    if needle.len() >= 4
        && (needle.contains(&p.slug.to_lowercase()) || needle.contains(&p.name.to_lowercase()))
    {
        return 50;
    }
    0
}

fn keel_activity(store: &DuckStore, args: &Value) -> Result<Value, RpcError> {
    let project = match opt_str(args, "project") {
        Some(p) => Some(resolve_project(store, &p)?),
        None => None,
    };
    let cursor = match opt_str(args, "cursor") {
        Some(raw) => Cursor::After(
            EventId::parse(&raw)
                .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?,
        ),
        None => match opt_time(args, "since")? {
            Some(t) => Cursor::Since(t),
            None => Cursor::Beginning,
        },
    };
    let limit = opt_i64(args, "limit").unwrap_or(50).clamp(1, 500) as usize;

    let page = store
        .events(&cursor, project.as_ref(), limit)
        .map_err(|e| to_rpc_error(store, e))?;

    let next_cursor = page.items.last().map(|e| e.id.as_str().to_owned());
    let summary = if page.items.is_empty() {
        "Nothing has changed.".to_owned()
    } else {
        let mut lines = vec![format!("{} change(s):", page.items.len())];
        for e in &page.items {
            lines.push(format!(
                "  {} {} {} — {}",
                e.created_at.format("%Y-%m-%d %H:%M"),
                e.actor,
                e.action,
                e.summary
            ));
        }
        if page.truncated {
            lines.push(format!(
                "  … {} more; pass cursor to continue",
                page.total - page.items.len()
            ));
        }
        lines.join("\n")
    };

    Ok(tool_result(
        summary,
        json!({
            "events": page.items,
            "total": page.total,
            "truncated": page.truncated,
            "cursor": next_cursor,
        }),
    ))
}

fn keel_create(store: &mut DuckStore, args: &Value) -> Result<Value, RpcError> {
    let type_name = req_str(args, "type")?;
    let entity_type = EntityType::parse(&type_name)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let provenance = provenance_from(args)?;

    let title = opt_str(args, "title")
        .or_else(|| opt_str(args, "name"))
        .or_else(|| opt_str(args, "term"));
    let body = opt_str(args, "body");

    let project_id = match opt_str(args, "project") {
        Some(p) => Some(resolve_project(store, &p)?),
        None => None,
    };

    if entity_type != EntityType::Project && entity_type != EntityType::Term && project_id.is_none()
    {
        return Err(bad_arg(
            "project",
            &format!("{entity_type} must belong to a project"),
            "a project id or slug; call keel_projects to list them",
        ));
    }

    let mut entity = build_entity(
        entity_type,
        project_id.clone(),
        title.clone(),
        body.clone(),
        args,
    )?;

    // Extra columns, applied through the same validated path an update uses,
    // so `fields: {status: "dun"}` produces the same helpful error either way.
    if let Some(Value::Object(fields)) = args.get("fields") {
        let mut cleaned = Map::new();
        for (k, v) in fields {
            cleaned.insert(k.clone(), v.clone());
        }
        keel_core::store::apply_changes(&mut entity, &cleaned)
            .map_err(|e| to_rpc_error(store, e))?;
    }

    if let Some(key) = opt_str(args, "idempotency_key") {
        entity.set_idempotency_key(key);
    }

    let created = store
        .create(entity, &provenance)
        .map_err(|e| to_rpc_error(store, e))?;

    // A prose body becomes the first document revision rather than a column.
    let mut document = Value::Null;
    if let Some(text) = body.filter(|_| entity_type.has_document()) {
        let doc = keel_core::Document::first(
            entity_type,
            created.entity.id().clone(),
            created.entity.project_id().cloned(),
            title.clone().unwrap_or_default(),
            text,
            provenance.actor,
            chrono::Utc::now(),
        )
        .map_err(|e| to_rpc_error(store, e))?
        .attributed(provenance.session_id.clone(), provenance.surface);
        let written = store
            .write_revision(doc)
            .map_err(|e| to_rpc_error(store, e))?;
        document = serde_json::to_value(&written).unwrap_or(Value::Null);
    }

    // Re-read so `current_doc_version` reflects the revision just written.
    let entity = store
        .get(created.entity.id())
        .map_err(|e| to_rpc_error(store, e))?
        .unwrap_or(created.entity);

    let summary = if created.created {
        format!(
            "Created {} “{}” — {}",
            entity_type,
            entity.label(),
            entity.id()
        )
    } else {
        format!(
            "{} “{}” already exists — {} (nothing was created)",
            entity_type,
            entity.label(),
            entity.id()
        )
    };

    Ok(tool_result(
        summary,
        json!({
            "entity": entity_json(&entity),
            "created": created.created,
            "document": document
        }),
    ))
}

/// Construct the right struct for a type, from the common arguments.
fn build_entity(
    entity_type: EntityType,
    project_id: Option<EntityId>,
    title: Option<String>,
    body: Option<String>,
    args: &Value,
) -> Result<Entity, RpcError> {
    let need_title = || -> Result<String, RpcError> {
        title.clone().ok_or_else(|| {
            bad_arg(
                "title",
                &format!("{entity_type} needs a name"),
                "a short one-line title",
            )
        })
    };
    let need_project = || -> Result<EntityId, RpcError> {
        project_id
            .clone()
            .ok_or_else(|| bad_arg("project", "required", "a project id or slug"))
    };

    Ok(match entity_type {
        EntityType::Project => {
            let name = need_title()?;
            let slug = opt_str(args, "slug").unwrap_or_else(|| slugify(&name));
            let mut p = Project::new(slug, name);
            p.description = body;
            p.into()
        }
        EntityType::Milestone => Milestone::new(need_project()?, need_title()?).into(),
        EntityType::Task => {
            let mut t = Task::new(need_project()?, need_title()?);
            t.body = body;
            t.into()
        }
        EntityType::Spec => Spec::new(need_project()?, need_title()?).into(),
        EntityType::Decision => Decision::new(need_project()?, need_title()?).into(),
        EntityType::Question => Question::new(need_project()?, need_title()?).into(),
        EntityType::Term => {
            let definition = body
                .clone()
                .or_else(|| opt_str(args, "definition"))
                .ok_or_else(|| {
                    bad_arg(
                        "body",
                        "a term needs a definition",
                        "what the word means in this project",
                    )
                })?;
            Term::new(project_id.clone(), need_title()?, definition).into()
        }
        EntityType::Feedback => Feedback::new(need_project()?, need_title()?).into(),
        EntityType::Design => Design::new(need_project()?, need_title()?).into(),
        EntityType::Environment => Environment::new(need_project()?, need_title()?).into(),
        EntityType::Metric => Metric::new(need_project()?, need_title()?).into(),
        EntityType::MetricObservation => {
            let metric_raw = req_str(args, "metric_id")?;
            let metric_id = EntityId::parse_as(&metric_raw, EntityType::Metric)
                .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
            let value = args
                .get("value")
                .and_then(Value::as_f64)
                .ok_or_else(|| bad_arg("value", "missing", "the measured number"))?;
            let observed_at = opt_time(args, "observed_at")?.unwrap_or_else(chrono::Utc::now);
            MetricObservation::new(metric_id, need_project()?, value, observed_at).into()
        }
        EntityType::Artifact => Artifact::new(need_project()?, need_title()?).into(),
    })
}

/// A URL-safe slug from a display name.
fn slugify(name: &str) -> String {
    let s: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    s.split('-')
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn keel_update(store: &mut DuckStore, args: &Value) -> Result<Value, RpcError> {
    let raw_id = req_str(args, "id")?;
    let id = EntityId::parse(&raw_id)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let version = opt_i64(args, "version").ok_or_else(|| {
        bad_arg(
            "version",
            "missing",
            "the `version` you read, so a concurrent edit can be detected",
        )
    })? as i32;
    let provenance = provenance_from(args)?;

    if opt_bool(args, "archive") {
        let archived = store
            .archive(&id, version, &provenance)
            .map_err(|e| to_rpc_error(store, e))?;
        return Ok(tool_result(
            format!("Archived {} “{}”", archived.entity_type(), archived.label()),
            json!({ "entity": entity_json(&archived), "archived": true }),
        ));
    }

    let changes = match args.get("changes") {
        Some(Value::Object(m)) => m.clone(),
        _ => {
            return Err(bad_arg(
                "changes",
                "missing or not an object",
                "an object of field names to new values",
            ));
        }
    };

    let updated = store
        .update(&id, version, &changes, &provenance)
        .map_err(|e| to_rpc_error(store, e))?;

    let changed: Vec<&String> = changes.keys().collect();
    let summary = if changed.is_empty() {
        format!(
            "{} was already in that state; nothing changed.",
            updated.label()
        )
    } else {
        format!(
            "Updated {} “{}” ({}) — now version {}",
            updated.entity_type(),
            updated.label(),
            changed
                .iter()
                .map(|k| k.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            updated.audit().version
        )
    };

    Ok(tool_result(
        summary,
        json!({ "entity": entity_json(&updated) }),
    ))
}

fn keel_write_doc(store: &mut DuckStore, args: &Value) -> Result<Value, RpcError> {
    let raw_id = req_str(args, "id")?;
    let id = EntityId::parse(&raw_id)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let body = req_str(args, "body")?;
    let provenance = provenance_from(args)?;

    if !id.entity_type().has_document() {
        return Err(bad_arg(
            "id",
            &format!("{} has no prose body", id.entity_type()),
            "a spec, decision, question, feedback or design id",
        ));
    }

    let entity = store
        .get(&id)
        .map_err(|e| to_rpc_error(store, e))?
        .ok_or_else(|| {
            bad_arg(
                "id",
                &format!("no artifact with id {id}"),
                "an id returned by keel_create or keel_search",
            )
        })?;

    let title = opt_str(args, "title").unwrap_or_else(|| entity.label().to_owned());
    let previous = store
        .revision(&id, None)
        .map_err(|e| to_rpc_error(store, e))?
        .map(|d| d.version);

    let doc = keel_core::Document::first(
        id.entity_type(),
        id.clone(),
        entity.project_id().cloned(),
        title,
        body,
        provenance.actor,
        chrono::Utc::now(),
    )
    .map_err(|e| to_rpc_error(store, e))?
    .attributed(provenance.session_id.clone(), provenance.surface);

    let written = store
        .write_revision(doc)
        .map_err(|e| to_rpc_error(store, e))?;

    let unchanged = previous == Some(written.version);
    let summary = if unchanged {
        format!(
            "Body of “{}” is unchanged; still at revision {}.",
            written.title, written.version
        )
    } else {
        format!(
            "Wrote revision {} of “{}” ({} bytes).",
            written.version,
            written.title,
            written.body.len()
        )
    };

    Ok(tool_result(
        summary,
        json!({ "document": written, "created_revision": !unchanged }),
    ))
}

fn keel_link(store: &mut DuckStore, args: &Value) -> Result<Value, RpcError> {
    let from = EntityId::parse(&req_str(args, "from")?)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let to = EntityId::parse(&req_str(args, "to")?)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let rel = Relation::parse(&req_str(args, "rel")?)
        .map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))?;
    let anchor = opt_str(args, "anchor").unwrap_or_default();
    let provenance = provenance_from(args)?;

    if opt_bool(args, "remove") {
        let removed = store
            .unlink(&from, rel, &to, &anchor, &provenance)
            .map_err(|e| to_rpc_error(store, e))?;
        return Ok(tool_result(
            format!("Removed {from} {rel} {to}"),
            json!({ "link": removed, "removed": true }),
        ));
    }

    let mut new_link = NewLink::new(from.clone(), rel, to.clone());
    if !anchor.is_empty() {
        new_link = new_link.anchored(anchor.clone());
    }
    if let Some(note) = opt_str(args, "note") {
        new_link = new_link.noted(note);
    }

    let link = store
        .link(new_link, &provenance)
        .map_err(|e| to_rpc_error(store, e))?;

    // Say what was actually stored when the caller asked for `depends_on`.
    // Leaving it implicit is how the next reader concludes the endpoints went
    // in backwards.
    let summary = if rel == Relation::DependsOn {
        format!(
            "Linked {from} depends_on {to}, stored as {} blocks {} — `depends_on` and `blocks` \
             are the same fact, and Keel keeps one direction.",
            link.from_id, link.to_id
        )
    } else {
        format!(
            "Linked {} {} {}{}",
            link.from_id,
            link.rel,
            link.to_id,
            if anchor.is_empty() {
                String::new()
            } else {
                format!(" at {anchor}")
            }
        )
    };

    Ok(tool_result(summary, json!({ "link": link })))
}

/// Parse a `types` argument.
fn parse_types(value: Option<&Value>) -> Result<Vec<EntityType>, RpcError> {
    let Some(Value::Array(items)) = value else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .map(|s| {
            EntityType::parse(s).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))
        })
        .collect()
}

/// Parse a `rels` argument.
fn parse_rels(value: Option<&Value>) -> Result<Vec<Relation>, RpcError> {
    let Some(Value::Array(items)) = value else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .filter_map(Value::as_str)
        .map(|s| {
            Relation::parse(s).map_err(|e| RpcError::new(codes::INVALID_PARAMS, e.to_string()))
        })
        .collect()
}

/// The display label of a serialised entity.
fn label_of(entity: &Value) -> &str {
    for key in ["title", "name", "term", "summary"] {
        if let Some(v) = entity.get(key).and_then(Value::as_str) {
            return v;
        }
    }
    "(unnamed)"
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_url_safe() {
        assert_eq!(slugify("Keel"), "keel");
        assert_eq!(slugify("Harbour Billing"), "harbour-billing");
        assert_eq!(slugify("  Weird — Name!  "), "weird-name");
        assert_eq!(slugify("v1.0 Release"), "v1-0-release");
    }

    #[test]
    fn an_exact_slug_beats_a_substring() {
        let mut exact = Project::new("keel", "Keel");
        exact.aliases = vec!["spine".to_owned()];
        let partial = Project::new("keel-desktop", "Keel Desktop");

        assert!(match_score("keel", &exact) > match_score("keel", &partial));
        assert_eq!(match_score("spine", &exact), 110);
        assert_eq!(match_score("nothing-like-it", &exact), 0);
    }

    #[test]
    fn a_repo_url_matches() {
        let mut p = Project::new("harbour", "Harbour");
        p.repo_urls = vec!["https://github.com/kb/harbour".to_owned()];
        assert!(match_score("github.com/kb/harbour", &p) > 0);
    }

    #[test]
    fn missing_arguments_say_what_was_expected() {
        let err = req_str(&json!({}), "query").unwrap_err();
        assert!(err.message.contains("query"), "{}", err.message);
        assert!(err.message.contains("Expected"), "{}", err.message);
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn a_malformed_timestamp_shows_the_shape_wanted() {
        let err = opt_time(&json!({"since": "yesterday"}), "since").unwrap_err();
        assert!(err.message.contains("RFC 3339"), "{}", err.message);
    }

    #[test]
    fn provenance_defaults_to_claude_but_keeps_a_supplied_session() {
        let p = provenance_from(&json!({"session_id": "ses_x", "surface": "chat"})).unwrap();
        assert_eq!(p.actor, Actor::Claude);
        assert_eq!(p.session_id.as_deref(), Some("ses_x"));
        assert_eq!(p.surface, Some(Surface::Chat));

        // No session is legal — refusing the write would be worse (D-10).
        let bare = provenance_from(&json!({})).unwrap();
        assert_eq!(bare.session_id, None);
        assert_eq!(bare.actor, Actor::Claude);
    }

    #[test]
    fn an_unknown_surface_is_rejected_with_the_valid_ones() {
        let err = provenance_from(&json!({"surface": "fax"})).unwrap_err();
        assert!(err.message.contains("chat"), "{}", err.message);
    }

    #[test]
    fn unknown_types_and_relations_are_rejected() {
        assert!(parse_types(Some(&json!(["task", "epic"]))).is_err());
        assert!(parse_types(Some(&json!(["task", "spec"]))).is_ok());
        assert!(parse_rels(Some(&json!(["relates_to"]))).is_err());
        assert!(parse_rels(Some(&json!(["implements"]))).is_ok());
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod path_tests {
    use super::normalise_path;

    #[test]
    fn redundant_separators_do_not_make_two_paths_different() {
        // The live failure: a session reported `matched_project: null` for a
        // directory that had a project, because `cwd` carried a doubled
        // separator and the stored root_path did not. It started unoriented,
        // and only mentioned it in passing.
        assert_eq!(
            normalise_path("/tmp//keel-gate"),
            normalise_path("/tmp/keel-gate")
        );
        assert_eq!(normalise_path("/a///b//c"), "/a/b/c");
        assert_eq!(normalise_path("/a/b/"), "/a/b");
        assert_eq!(normalise_path("/a/b///"), "/a/b");
        // Root itself survives, rather than normalising to the empty string.
        assert_eq!(normalise_path("/"), "/");
    }

    #[test]
    fn a_sibling_with_a_shared_prefix_is_not_inside_the_project() {
        // `/a/bee` must not resolve to a project rooted at `/a/b`. The match
        // appends a separator for exactly this, and normalisation must not
        // undo it.
        let root = normalise_path("/a/b");
        let sibling = normalise_path("/a/bee");
        assert_ne!(sibling, root);
        assert!(!sibling.starts_with(&format!("{root}/")));

        let child = normalise_path("/a/b//c");
        assert!(child.starts_with(&format!("{root}/")));
    }
}
