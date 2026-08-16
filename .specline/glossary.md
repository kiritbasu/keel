# Glossary

<!-- specline:generated glossary prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from Specline — edits here are not saved.

**Anchor** — A reference to a block inside a document, such as REQ-4, so a task can link to one requirement rather than a whole spec.

**Artifact** — Any stored entity. Used generically, not as the specific `artifact` type.

**Claude Code vs Claude Desktop** — Two different products, and "desktop" names both — which cost a session an incorrect answer about how Keel installs.

**Claude Code** is what Keel is built for. It runs as a terminal CLI *and* as a Mac desktop app; both are Claude Code and both support plugins, hooks and slash commands. Everything Keel depends on — the marketplace entry, `/keel:setup`, the SessionStart digest, the Stop nudge — works in either. Writes from it carry surface `code`. This is 1,182 of the events in the store.

**Claude Desktop** is the separate general-purpose chat client. It can reach the daemon's MCP endpoint, because the transport's `Origin` check is aimed at browsers and a native client sends none — but it has no Claude Code plugins and, decisively, no session hooks. Writes from it would carry surface `chat`, of which there are four.

So "I use Claude Desktop" most likely means the Claude Code desktop app, and the way to tell is the surface on the writes rather than the words.


**Digest** — The compact project summary returned by keel_context. Budgeted to roughly 3–4k tokens.

**Era** — Which MCP revision a request belongs to. Modern is 2026-07-28; Legacy is 2025-11-25 and earlier.

**Hybrid search** — Keyword and semantic retrieval fused by reciprocal rank, because BM25 scores and vector distances are not on comparable scales.

**Mirror** — Generated read-only markdown written into a project repo. Never a source of truth.

**Phase** — What this project calls a milestone. `Phase` and `milestone` are the same thing; this is the word to use when talking to a person.

**Phase gate** — The exit criterion for a build phase. Two of Keel's cannot be verified without a human.

**Revision** — One immutable version of a document body.

**Session** — One Claude conversation, used as the provenance unit. Caller-supplied; Keel never invents one.

**Surface** — Where a write came from: chat, cowork, code, ui or cli.

**Traversal direction** — Which way an edge is walked. Outbound matches from_id, inbound matches to_id. Getting it wrong returns an empty set that looks legitimate.

**Vertex view** — v_entities — the UNION over all thirteen tables that lets a query resolve an id without knowing its type.

