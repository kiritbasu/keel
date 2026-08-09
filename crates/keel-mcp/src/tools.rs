//! The nine tool definitions.
//!
//! Nine, not forty. Models choose correctly among nine and badly among forty,
//! and `product/CLAUDE.md` names expanding this surface as an anti-pattern
//! explicitly: more tools means worse selection, not more capability.
//!
//! # These descriptions are the product
//!
//! The MCP surface is what an agent actually experiences, and the tool
//! description is the only documentation it gets. So each one says *when to
//! reach for this tool*, not merely what it does — a description that reads
//! like a function signature produces an agent that calls the wrong tool
//! confidently.

use keel_core::{EntityType, Relation, TaskPriority, TaskStatus};
use serde_json::{Value, json};

/// A tool as advertised over `tools/list`.
#[derive(Debug, Clone)]
pub struct Tool {
    /// The name an agent calls.
    pub name: &'static str,
    /// Short label for a UI.
    pub title: &'static str,
    /// When to use it and what it returns.
    pub description: String,
    /// JSON Schema for the arguments.
    pub input_schema: Value,
    /// Whether the tool mutates anything. Advertised so a host can gate
    /// writes without inferring intent from the name.
    pub read_only: bool,
}

impl Tool {
    /// The `tools/list` representation.
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "title": self.title,
            "description": self.description,
            "inputSchema": self.input_schema,
            "annotations": {
                "readOnlyHint": self.read_only,
                // Nothing in Keel is ever deleted (D-9), so no tool is
                // destructive in the sense a host cares about.
                "destructiveHint": false,
                "idempotentHint": true,
            }
        })
    }
}

/// The three ambient arguments every tool accepts.
///
/// Documented once in SPEC §6.2 rather than repeated per tool, and injected
/// once here for the same reason. `session_id` is the one that matters: it is
/// the provenance unit G3 and REQ-2 rest on, and the daemon never invents one.
fn with_ambient(mut schema: Value, write: bool) -> Value {
    let Some(props) = schema.get_mut("properties").and_then(Value::as_object_mut) else {
        return schema;
    };

    props.insert(
        "session_id".to_owned(),
        json!({
            "type": "string",
            "description": "A stable identifier for this conversation, minted once at first \
                            use and passed on every call. Keel never invents one: a stateless \
                            transport has no session to borrow, so provenance is cooperative. \
                            Omitting it still works, but the write is attributed only to \
                            'some Claude session'."
        }),
    );
    props.insert(
        "surface".to_owned(),
        json!({
            "type": "string",
            "enum": ["chat", "cowork", "code", "ui", "cli"],
            "description": "Where this call came from."
        }),
    );
    if write {
        props.insert(
            "idempotency_key".to_owned(),
            json!({
                "type": "string",
                "description": "Makes a retry a no-op instead of a duplicate. Derived from the \
                                project, type and normalised title when omitted, which is \
                                usually what you want — supply one only when two genuinely \
                                different things share a title."
            }),
        );
    }
    schema
}

/// Enumerate a closed set for a schema.
fn enum_of<T: AsRef<str>>(values: impl IntoIterator<Item = T>) -> Value {
    Value::Array(
        values
            .into_iter()
            .map(|v| Value::String(v.as_ref().to_owned()))
            .collect(),
    )
}

/// Every entity type name, for `type` arguments.
fn type_enum() -> Value {
    enum_of(EntityType::wire_names())
}

/// Every relation name.
fn relation_enum() -> Value {
    enum_of(Relation::ALL.iter().map(|r| r.as_str()))
}

/// The nine tools, in a stable order.
///
/// Order is deliberate and must not change casually: the specification asks
/// for a deterministic `tools/list` so clients can cache it and so the list
/// lands identically in every prompt, which is worth real money in cache hits.
/// The order is also pedagogical — `keel_context` first because it is the
/// entry point, writes after reads.
pub fn all() -> Vec<Tool> {
    vec![
        Tool {
            name: "keel_context",
            title: "Orient on a project",
            description:
                "START HERE in any new conversation about a project. Returns a compact digest: \
                 what the project is, the active milestone, urgent and blocked work, recent \
                 decisions, every unresolved question, the glossary, what is deployed, and a \
                 suggested next action.\n\n\
                 Call this before reading files or asking the human what is going on — it is \
                 one call and roughly 3–4k tokens. With no `project`, returns a one-line \
                 roll-up of every project plus anything at risk.\n\n\
                 Open questions and glossary terms are never truncated. A missing open question \
                 makes you re-litigate something already settled; a missing glossary term makes \
                 you use the wrong word for a domain concept. Everything else degrades and the \
                 response reports what it dropped."
                    .to_owned(),
            read_only: true,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "properties": {
                        "project": {
                            "type": "string",
                            "description": "Project id, slug or name. Omit for a cross-project roll-up."
                        },
                        "depth": {
                            "type": "string",
                            "enum": ["brief", "standard", "full"],
                            "default": "standard",
                            "description": "How much to include. `brief` is a few hundred tokens; \
                                            `full` drops most limits."
                        },
                        "since": {
                            "type": "string",
                            "format": "date-time",
                            "description": "Only summarise activity after this instant. Useful when \
                                            resuming a conversation you already have context for."
                        }
                    }
                }),
                false,
            ),
        },
        Tool {
            name: "keel_search",
            title: "Search everything",
            description:
                "Hybrid keyword and semantic search across every artifact that carries text, in \
                 every project. Use it to answer 'what do we know about X', 'has anyone raised \
                 this before', or 'what did customers say about onboarding'.\n\n\
                 Searches specs, decisions, questions, feedback and design captions by meaning \
                 and by keyword together, and tasks, milestones, terms, environments, artifacts \
                 and projects by keyword. Metrics are deliberately excluded — they are numbers, \
                 and reaching them is a filter rather than a search.\n\n\
                 Prefer a natural question over keywords; the semantic half is what makes \
                 'why is billing slow' find a decision titled 'Aggregate hourly, not per-minute'."
                    .to_owned(),
            read_only: true,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["query"],
                    "properties": {
                        "query": { "type": "string", "description": "What you are looking for." },
                        "project": { "type": "string", "description": "Restrict to one project." },
                        "types": {
                            "type": "array",
                            "items": { "type": "string", "enum": type_enum() },
                            "description": "Restrict to these artifact types."
                        },
                        "since": { "type": "string", "format": "date-time" },
                        "until": { "type": "string", "format": "date-time" },
                        "limit": { "type": "integer", "default": 20, "minimum": 1, "maximum": 100 }
                    }
                }),
                false,
            ),
        },
        Tool {
            name: "keel_get",
            title: "Fetch by id",
            description:
                "Fetch one or more artifacts by id, optionally with their prose body, their \
                 linked neighbours, or a diff between two revisions.\n\n\
                 Use `depth` to pull in the graph around something — `keel_get(id: spec_id, \
                 depth: 2)` answers 'what implements this spec, and what do those things \
                 depend on' in one call. Use `version` to read an older revision and \
                 `diff_against` to see what changed between two."
                    .to_owned(),
            read_only: true,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["ids"],
                    "properties": {
                        "ids": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Prefixed ULIDs, e.g. tsk_01H8… The prefix says what \
                                            the artifact is, so you never need to say."
                        },
                        "include_body": {
                            "type": "boolean",
                            "default": true,
                            "description": "Include the prose body for artifacts that have one."
                        },
                        "version": {
                            "type": "integer",
                            "description": "Read this document revision instead of the current one."
                        },
                        "diff_against": {
                            "type": "integer",
                            "description": "Also return a unified diff between `version` (or the \
                                            current revision) and this one."
                        },
                        "depth": {
                            "type": "integer",
                            "default": 0,
                            "minimum": 0,
                            "maximum": 16,
                            "description": "Also return linked neighbours to this depth. 0 means \
                                            no traversal; 6 is the usual maximum worth asking for."
                        },
                        "direction": {
                            "type": "string",
                            "enum": ["outbound", "inbound", "both"],
                            "default": "both",
                            "description": "Which way to walk. Outbound follows edges away from \
                                            the artifact ('what does this implement'); inbound \
                                            follows edges into it ('what implements this')."
                        },
                        "rels": {
                            "type": "array",
                            "items": { "type": "string", "enum": relation_enum() },
                            "description": "Restrict the traversal to these relations."
                        }
                    }
                }),
                false,
            ),
        },
        Tool {
            name: "keel_projects",
            title: "List and resolve projects",
            description:
                "List projects, or resolve a name to one. **Call this before creating a project, \
                 every time.** It fuzzy-matches on name, slug, aliases and repository URL, and \
                 when anything plausible matches it returns `requires_confirmation: true` with \
                 the candidates.\n\n\
                 When that happens, ask the human before creating anything. Nine near-duplicate \
                 projects is the failure mode that quietly ruins the cross-project view, and it \
                 is much cheaper to ask than to merge later."
                    .to_owned(),
            read_only: true,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "A name, slug, alias or repo URL to match against. \
                                            Omit to list everything."
                        },
                        "include_archived": { "type": "boolean", "default": false }
                    }
                }),
                false,
            ),
        },
        Tool {
            name: "keel_activity",
            title: "What changed",
            description:
                "Every mutation since a timestamp or an event cursor, oldest first. Use it to \
                 catch up: 'what happened since I last looked', or to see what another session \
                 did while you were working.\n\n\
                 Pass the `cursor` from a previous response to continue exactly where you left \
                 off, with no gaps and no repeats."
                    .to_owned(),
            read_only: true,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "properties": {
                        "project": { "type": "string" },
                        "since": { "type": "string", "format": "date-time" },
                        "cursor": {
                            "type": "string",
                            "description": "An event id from a previous response. Takes precedence \
                                            over `since`."
                        },
                        "limit": { "type": "integer", "default": 50, "minimum": 1, "maximum": 500 }
                    }
                }),
                false,
            ),
        },
        Tool {
            name: "keel_create",
            title: "Create an artifact",
            description:
                "Create any of the thirteen artifact types. Returns the created artifact, so you \
                 never need to read it back.\n\n\
                 Creates are idempotent: calling twice with the same project, type and title \
                 returns the existing artifact with `created: false` rather than making a \
                 duplicate. Whitespace and capitalisation are normalised, so 'Add login page' \
                 and 'add  Login  Page' are one task.\n\n\
                 Before creating a **project**, call `keel_projects` first and confirm with the \
                 human (see that tool). Prefer consolidating into fewer, larger artifacts: a \
                 project with forty trivial tasks that should be eight is worse than useless."
                    .to_owned(),
            read_only: false,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["type"],
                    "properties": {
                        "type": {
                            "type": "string",
                            "enum": type_enum(),
                            "description": "Which artifact type to create."
                        },
                        "project": {
                            "type": "string",
                            "description": "Project id or slug. Required for everything except \
                                            `project` itself, and optional for `term` — omitting \
                                            it there defines the term globally."
                        },
                        "title": {
                            "type": "string",
                            "description": "The name. Called `name` on some types and `term` on \
                                            glossary entries; `title` is accepted for all of them."
                        },
                        "body": {
                            "type": "string",
                            "description": "For prose-bearing types (spec, decision, question, \
                                            feedback, design) this is written as the first \
                                            document revision. For a task it is the short detail \
                                            field — anything long-form belongs in a spec."
                        },
                        "fields": {
                            "type": "object",
                            "description": "Any other column on the type: status, kind, priority, \
                                            labels, target_date, severity, sentiment, url, and so \
                                            on. Invalid values are rejected with the list of \
                                            valid ones.",
                            "additionalProperties": true
                        }
                    }
                }),
                true,
            ),
        },
        Tool {
            name: "keel_update",
            title: "Update an artifact",
            description:
                "Change fields on an existing artifact, including status transitions. Returns the \
                 updated artifact.\n\n\
                 Pass the `version` you read. If someone else changed it since, the call is \
                 rejected with the current state and the events that happened in between, so you \
                 can usually merge and retry without asking anyone.\n\n\
                 An accepted decision's content is immutable — supersede it with a new decision \
                 linked by `supersedes` rather than editing it. Use `archive: true` to soft-delete; \
                 nothing in Keel is ever really deleted."
                    .to_owned(),
            read_only: false,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["id", "version"],
                    "properties": {
                        "id": { "type": "string" },
                        "version": {
                            "type": "integer",
                            "description": "The `version` from when you read it. Not the document \
                                            revision — that is `current_doc_version`."
                        },
                        "changes": {
                            "type": "object",
                            "description": "Fields to set. Unknown fields are rejected with the \
                                            list of real ones.",
                            "additionalProperties": true
                        },
                        "archive": {
                            "type": "boolean",
                            "default": false,
                            "description": "Soft-delete instead of updating."
                        }
                    }
                }),
                true,
            ),
        },
        Tool {
            name: "keel_write_doc",
            title: "Write a document revision",
            description:
                "Append a new revision of an artifact's prose body — for specs, decisions, \
                 questions, feedback and design captions.\n\n\
                 Use this whenever the *content* of a document changes. Use `keel_update` \
                 instead for the fields around it: title, status, kind. The two are separate \
                 because a body is versioned and a status is not, and conflating them would \
                 either version every status flip or lose the history of every edit.\n\n\
                 Always send the **full** body, not a patch — the revision is a snapshot. The \
                 previous one is kept and stays readable by version, and `keel_get` will diff \
                 any two. Writing content identical to the current revision is a no-op rather \
                 than a new version, so regenerating a document you have not changed is safe."
                    .to_owned(),
            read_only: false,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["id", "body"],
                    "properties": {
                        "id": { "type": "string" },
                        "title": {
                            "type": "string",
                            "description": "Update the title alongside the body. Omit to keep it."
                        },
                        "body": { "type": "string", "description": "The full markdown body." }
                    }
                }),
                true,
            ),
        },
        Tool {
            name: "keel_link",
            title: "Link two artifacts",
            description:
                "Create or remove a typed edge. Direction matters and reads left to right: \
                 `from` does the verb to `to`.\n\n\
                 A task **implements** a spec. A blocker **blocks** the thing waiting on it. A \
                 newer decision **supersedes** an older one. A decision **resolves** a question. \
                 Feedback **informs** a spec. If you find yourself wanting to say 'A depends on \
                 B', use `depends_on` and Keel will store it the right way round.\n\n\
                 Use `anchor` to link to one requirement inside a spec (`REQ-4`) rather than the \
                 whole document — that is what makes traceability answerable per requirement."
                    .to_owned(),
            read_only: false,
            input_schema: with_ambient(
                json!({
                    "type": "object",
                    "required": ["from", "rel", "to"],
                    "properties": {
                        "from": { "type": "string", "description": "The artifact doing the verb." },
                        "rel": { "type": "string", "enum": relation_enum() },
                        "to": { "type": "string", "description": "The artifact it is done to." },
                        "anchor": {
                            "type": "string",
                            "description": "A block inside the target, e.g. `REQ-4`. Omit for a \
                                            whole-artifact link."
                        },
                        "note": { "type": "string", "description": "Why this link exists." },
                        "remove": {
                            "type": "boolean",
                            "default": false,
                            "description": "Archive the edge instead of creating it."
                        }
                    }
                }),
                true,
            ),
        },
    ]
}

/// Find a tool by name.
pub fn find(name: &str) -> Option<Tool> {
    all().into_iter().find(|t| t.name == name)
}

/// The `tools/list` result, with the cache hints this revision requires.
pub fn list_result() -> Value {
    json!({
        "tools": all().iter().map(Tool::to_json).collect::<Vec<_>>(),
        // Keel's tool list is static — it changes when the binary changes and
        // never at runtime — so a long TTL is honest and stops clients
        // polling. `public` because there is nothing caller-specific in it.
        "ttlMs": 86_400_000u64,
        "cacheScope": "public",
    })
}

/// The `server/discover` result.
///
/// Required in this revision: a client may call it before anything else to
/// pick a protocol version, and on stdio it doubles as the backward-
/// compatibility probe.
pub fn discover_result() -> Value {
    json!({
        "protocolVersions": [crate::protocol::PROTOCOL_VERSION],
        "serverInfo": crate::protocol::server_info(),
        "capabilities": {
            "tools": { "listChanged": false },
            // Resources, prompts, sampling, roots and logging are all
            // deliberately absent. Keel's surface is nine tools; advertising
            // capabilities it does not implement would invite calls it would
            // then have to refuse.
        },
        "instructions":
            "Keel stores everything about a software project except the code. Call \
             `keel_context` first to orient. Pass a stable `session_id` on every call so writes \
             are attributed to this conversation. Before creating a project, call \
             `keel_projects` and confirm with the human."
    })
}

/// Task statuses that count as open, for digest wording.
pub fn open_task_statuses() -> Vec<&'static str> {
    TaskStatus::ALL
        .iter()
        .filter(|s| s.is_open())
        .map(|s| s.as_str())
        .collect()
}

/// Priorities that count as urgent.
pub fn urgent_priorities() -> Vec<&'static str> {
    TaskPriority::ALL
        .iter()
        .filter(|p| p.is_urgent())
        .map(|p| p.as_str())
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_nine_tools() {
        // The ceiling from SPEC §6.1. Adding a tenth needs KB's agreement,
        // not a passing test suite.
        assert_eq!(
            all().len(),
            9,
            "nine tools is the ceiling — more tools means worse model selection"
        );
    }

    #[test]
    fn tool_names_match_the_spec() {
        let names: Vec<&str> = all().iter().map(|t| t.name).collect();
        assert_eq!(
            names,
            vec![
                "keel_context",
                "keel_search",
                "keel_get",
                "keel_projects",
                "keel_activity",
                "keel_create",
                "keel_update",
                "keel_write_doc",
                "keel_link",
            ]
        );
    }

    #[test]
    fn the_order_is_deterministic() {
        // Required for client-side caching and prompt-cache hits.
        let first: Vec<&str> = all().iter().map(|t| t.name).collect();
        let second: Vec<&str> = all().iter().map(|t| t.name).collect();
        assert_eq!(first, second);
    }

    #[test]
    fn every_tool_accepts_the_ambient_arguments() {
        for tool in all() {
            let props = tool.input_schema["properties"].as_object().unwrap();
            assert!(
                props.contains_key("session_id"),
                "{} must accept session_id — it is the provenance unit",
                tool.name
            );
            assert!(props.contains_key("surface"), "{}", tool.name);
            if !tool.read_only {
                assert!(
                    props.contains_key("idempotency_key"),
                    "{} is a write and must accept an idempotency key",
                    tool.name
                );
            }
        }
    }

    #[test]
    fn read_tools_are_marked_read_only() {
        let reads = [
            "keel_context",
            "keel_search",
            "keel_get",
            "keel_projects",
            "keel_activity",
        ];
        for tool in all() {
            assert_eq!(
                tool.read_only,
                reads.contains(&tool.name),
                "{} has the wrong read_only flag",
                tool.name
            );
        }
    }

    #[test]
    fn every_description_explains_when_to_use_it() {
        // A description that reads like a function signature produces an agent
        // that calls the wrong tool confidently.
        for tool in all() {
            assert!(
                tool.description.len() > 200,
                "{} has a thin description ({} chars)",
                tool.name,
                tool.description.len()
            );
            let d = tool.description.to_lowercase();
            assert!(
                d.contains("use it")
                    || d.contains("use this")
                    || d.contains("start here")
                    || d.contains("call this")
                    || d.contains("use `")
                    || d.contains("prefer"),
                "{} does not say when to reach for it",
                tool.name
            );
        }
    }

    #[test]
    fn the_link_tool_teaches_direction_by_example() {
        // Direction is the most dangerous thing to get wrong, and the tool
        // description is the only documentation an agent gets.
        let link = find("keel_link").unwrap();
        assert!(
            link.description.contains("implements"),
            "{}",
            link.description
        );
        assert!(link.description.contains("supersedes"));
        assert!(link.description.contains("depends_on"));
        assert!(link.description.contains("anchor") || link.description.contains("REQ-4"));
    }

    #[test]
    fn the_context_tool_says_it_is_the_entry_point() {
        let ctx = find("keel_context").unwrap();
        assert!(ctx.description.starts_with("START HERE"));
    }

    #[test]
    fn the_projects_tool_carries_the_disambiguation_instruction() {
        // REQ-8: safety lives in the skill and in this description, not in the
        // API, because creating a project is a legitimate thing to do.
        let p = find("keel_projects").unwrap();
        assert!(p.description.contains("before creating a project"));
        assert!(p.description.contains("ask the human"));
    }

    #[test]
    fn tools_list_carries_the_required_cache_hints() {
        let list = list_result();
        assert!(list["ttlMs"].as_u64().unwrap() > 0);
        assert_eq!(list["cacheScope"], "public");
        assert_eq!(list["tools"].as_array().unwrap().len(), 9);
    }

    #[test]
    fn discover_advertises_only_what_is_implemented() {
        let d = discover_result();
        assert_eq!(d["protocolVersions"][0], crate::protocol::PROTOCOL_VERSION);
        let caps = d["capabilities"].as_object().unwrap();
        assert!(caps.contains_key("tools"));
        for absent in ["resources", "prompts", "sampling", "roots", "logging"] {
            assert!(
                !caps.contains_key(absent),
                "advertising `{absent}` invites calls Keel would have to refuse"
            );
        }
    }

    #[test]
    fn unknown_tools_are_not_found() {
        assert!(find("keel_delete").is_none());
        assert!(find("keel_context").is_some());
    }

    #[test]
    fn schemas_are_valid_json_objects_with_a_type() {
        for tool in all() {
            assert_eq!(tool.input_schema["type"], "object", "{}", tool.name);
            assert!(tool.input_schema["properties"].is_object(), "{}", tool.name);
        }
    }
}
