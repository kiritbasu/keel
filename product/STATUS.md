# Keel — Status

> **Maintained by Claude Code. Updated at the end of every session, without exception.**
> This file is the tracker until Keel can track itself (Phase 1 exit). Its shape mirrors Keel's own data model on purpose — importing it will be the first real end-to-end test.

---

## At a glance

| | |
|---|---|
| **Current phase** | Phase 0 — Spine |
| **Phase progress** | Phase 0: **16/16** · Phase 1: **12/12** |
| **Status** | Phases 0 and 1 built; Phase 2 in progress |
| **Blocked on** | Nothing |
| **Next up** | Phase 2 — the plugin: skill, session threading, mirror hooks |
| **Last session** | 2026-08-09 |
| **Last updated** | 2026-08-09 |

**Scope for this stretch:** KB confirmed Phases 0–3 before going away. Git is local-only, no remote. `session_id` is a skill-minted per-conversation ULID (Q-8 answered). Human-gated phase exits get an automated proxy plus an honest note here — see "Phase gates I cannot verify" below.

---

## Phase 0 — Spine

**Goal:** `keel-core` can create, read, update and archive every entity type, with correct versioning, provenance, links and events. No network, no UI.

**Exit criteria — all met, 2026-08-09:**

| Criterion | Evidence |
|---|---|
| All 13 entity types round-trip | `tests/roundtrip.rs` — create, read, update, archive, against real storage |
| Event log correct | `tests/concurrency.rs` — cursor paging visits every event exactly once, no gaps or repeats |
| 200-entity fixture loads | **212 entities, 29 links, 52 revisions**, three projects |
| Graph direction, both ways, every relation | `tests/graph_direction.rs` — 21 tests. Each asserts the correct traversal finds it *and* both inversions find nothing |
| Backup restores and diffs clean | `keel backup` → wipe → `keel restore` → row counts verified per table on both engines, then `fsck` clean and search still working |

| ID | Task | Status | Notes |
|---|---|---|---|
| P0-1 | Cargo workspace scaffold, CI, lint/fmt/deny gates | `done` | Five crates scaffolded per SPEC §1.1. CI runs fmt, clippy `-D warnings`, tests and `cargo deny`. unwrap/expect/panic are workspace clippy lints, so the definition of done is a build failure rather than review discipline |
| P0-2 | **Verify fast-moving dependencies** | `done` | **Verified against running code, not docs.** DuckDB 1.5.5 + Lance extension work end to end. DuckPGQ confirmed absent for 1.5.x (HTTP 404). MCP 2026-07-28 current. Two SPEC §5 syntax errors found and fixed in place. Nine MCP deltas recorded for Phase 1. Full table in DECISIONS.md |
| P0-3 | Domain types, ULID prefixes, the `<audit>` block | `done` | Thirteen types, prefixed ULIDs, audit block, relations, events, document revisions. 54 unit tests. Found: ULIDs need a monotonic generator or the §3.4 event cursor silently skips rows (B-9) |
| P0-4 | DuckDB schema + forward-only migrations | `done` | Forward-only migrations in `_keel_migrations`. `v_entities` built now rather than deferred (resolves TQ-4). PROVISIONAL: `idempotency_key` on all thirteen tables, not just tasks — see TQ-9 |
| P0-5 | Lance `documents` + `blobs` datasets, ATTACH wiring | `done` | Lance `documents` + `blobs` created through the DuckDB extension; no `lance` Rust crate needed (B-2). Found: Lance `CREATE TABLE` rejects all column constraints, so nullability is application-level — consistent with §2.1 |
| P0-6 | Entity storage layer — CRUD for all 13 types | `done` | CRUD for all 13 types behind `EntityStore`. Field patching goes through serde round-trip, so enum errors already name the valid values. 8 round-trip integration tests against real storage |
| P0-7 | Document revisions — append, fetch by version, diff | `done` | Append, fetch by version, unified diff via `similar`. Identical content does not grow the history — the §8.1 mirror hook regenerates and re-reads constantly |
| P0-8 | Links, `GraphStore` trait, CTE implementation | `done` | **21 tests, all 9 relations, both directions, plus both inversions.** `depends_on` normalises to `blocks` with swapped endpoints and never reaches the table. Cycle guard, depth clamp and shortest-path dedup all covered |
| P0-9 | Event log — append, query since cursor | `done` | Append-only, cursor and timestamp queries. Cursor paging test asserts every event is visited exactly once — it only holds because ULIDs are monotonic (B-9) |
| P0-10 | Embeddings via fastembed | `done` | `fastembed` behind an `Embedder` trait, passed in rather than constructed, so tests use a deterministic hash embedder instead of downloading 130 MB. A failed embed warns and keeps the write |
| P0-11 | Hybrid search — Lance + DuckDB FTS, RRF fusion | `done` | Lance hybrid + DuckDB BM25, fused by reciprocal rank. Found: the DuckDB FTS index is a snapshot and silently misses rows created after it was built — now rebuilt off the event-log watermark |
| P0-12 | Backup: DuckDB **and Lance** → Parquet, restore | `done` | **Both engines.** DuckDB `EXPORT DATABASE` plus an explicit Lance→Parquet dump. Restore refuses a backup missing its Lance half, and refuses to overwrite an existing store. Verified by row count per table, not by eye |
| P0-13 | `keel-cli fsck` — cross-engine referential integrity | `done` | 27 checks. Dangling links, stored `depends_on`, doc pointers to nowhere, duplicate idempotency keys, provenance gaps. Every finding says what it breaks and what to do |
| P0-14 | 200-entity fixture across all types and relations | `done` | **212 entities, 29 links, 52 revisions** across three projects. Loaded through the ordinary write path, so it exercises validation, idempotency and events — not just the schema |
| P0-15 | Test suite: concurrency, idempotency, OCC, round-trip | `done` | Round-trip, idempotency, OCC, events, graph direction, backup round-trip. Concurrency test written and `#[ignore]`d for Phase 1 with the reason on the line |
| P0-16 | Implement idempotency keys and optimistic concurrency | `done` | Derived keys normalise whitespace and case (R-6). OCC enforced by `WHERE version = ?`, not a read-then-write. Stale updates return `latest_version` |

---

## Phase 1 — Daemon

**Goal:** a live Claude session can orient itself, search, read, write and link, over MCP.

**Exit criteria:** a Claude session completes PRD UC-1 → UC-4; two concurrent sessions writing produce zero duplicates and zero lost updates.

> **On the MCP surface:** §6 was written against the 2026-07-28 announcement rather than the finished specification. Nine deltas are recorded in `product/DECISIONS.md` ("MCP deltas"). None changes the nine-tool surface; all of them change the daemon's wire handling. `server/discover` is now a *required* RPC, results carry `resultType`, and `tools/list` must return `ttlMs`/`cacheScope`.

| ID | Task | Status | Notes |
|---|---|---|---|
| P1-1 | JSON-RPC + stateless Streamable HTTP transport | `done` | Header/body validation with the renumbered codes (`-32020/21/22`, not the draft `-3200{1,3,4}`). GET/DELETE → 405 so an older client can tell "wrong protocol" from "no endpoint". **Found and fixed: the `Origin` check used `starts_with`, so `https://localhost.evil.example` passed it** |
| P1-2 | `server/discover` and `tools/list` | `done` | `server/discover` is required in this revision and is implemented. `tools/list` carries `ttlMs` and `cacheScope`, and the order is deterministic for prompt-cache hits |
| P1-3 | The nine tool schemas | `done` | Nine tools. The descriptions say *when to reach for this*, not just what it does — a test enforces that, because a description that reads like a signature produces an agent that calls the wrong tool confidently |
| P1-4 | `keel_context` — the digest | `done` | Budgeted to 3–4k tokens; trims in order of what an agent can most cheaply re-fetch. Questions and terms are **never** trimmed — verified with 60 of each, which returns them in full and sets `budget_exceeded` |
| P1-5 | Read tools: `keel_search`, `keel_get`, `keel_activity`, `keel_projects` | `done` | Including `version` + `diff_against` on `keel_get` (REQ-2 at the API layer) and fuzzy project matching with `requires_confirmation` (REQ-8) |
| P1-6 | Write tools: `keel_create`, `keel_update`, `keel_write_doc`, `keel_link` | `done` | 409 carries `latest_version`, current state and `events_since`. **Found and fixed: `version` was nested inside `audit` on read but asked for at the top level on write** — an agent had to hunt for it (B-13) |
| P1-7 | Shared single write path | `done` | One store, one mutex, whole process. Held across synchronous work, never across an await |
| P1-8 | Local REST + SSE for the desktop app | `done` | REST + SSE under `/api`, dispatching through the same tool layer as MCP so the two cannot drift |
| P1-9 | Un-ignore the concurrency test | `done` | **Phase 1 exit criterion met (mechanical half).** 16 concurrent sessions: exactly one create wins, all 16 updates land under retry, the event log is gapless and strictly ordered, concurrent identical links produce one edge |
| P1-10 | Snapshot tests for every tool response | `done` | 8 `insta` snapshots covering the tool list, discovery, the digest, creates, search, projects and every error shape. Ids and timestamps redacted so the snapshots do not churn |
| P1-11 | `keel-cli render-status` | `done` | Renders milestones, tasks by status, open questions, decisions, and a changelog derived from the event log. One-directional, like the mirror |
| P1-12 | Scripted UC-1 → UC-4 harness | `done` | 21 tests driving real HTTP against a real daemon. UC-1→UC-4 all pass mechanically. **The human half of the gate is still unverified** — see below |

---

## Later phases

Not broken down yet. Decompose at the start of each phase, not before — earlier phases will change what the later ones need.

| Phase | Scope | Gate |
|---|---|---|
| 1 — Daemon | axum, 9 MCP tools, `keel_context`, concurrency safety, `keel-cli render-status` for the dogfooding switch | Live Claude session completes UC-1→UC-4; 2 concurrent sessions, 0 duplicates, 0 lost updates |
| 2 — Plugin | Skill, session-ID threading, project confirmation, mirror hooks | ≥9 of 10 unprompted sessions write to Keel; 0 duplicate projects |
| 3 — Desktop | Tauri shell, sidecar, screens 1–6 and 9 | UC-6 in under 30s |
| 4 — Integrations | GitHub App, design artifacts, metrics, screens 7–8 | PR merge proposes closure |
| 5 — Remote | Deployable daemon, auth, mobile client | Status readable from phone |

**Phase 1 exit is also the dogfooding switch.** At that point, import this file, `product/DECISIONS.md` and `product/QUESTIONS.md` into Keel as its first project, and Keel becomes the tracker.

---

## Blocked

Nothing.

*(Format: task ID — what's blocking — who or what unblocks it — since when.)*

---

## Phase gates I cannot verify

KB's instruction was to substitute an automated proxy for the human-in-the-loop gates, proceed, and record honestly what is unverified. This is that record.

| Gate | Mechanical proxy — **passing** | What remains unverified |
|---|---|---|
| **Phase 1** — "a live Claude session completes UC-1 → UC-4" | `keel-daemon/tests/use_cases.rs`: 21 tests driving real HTTP, real headers, real JSON-RPC against a real daemon and a real store. All four use cases complete. | That the *tool descriptions* lead a model to pick the right tool unprompted. A scripted client is told which tool to call; an agent is not. This is the half that matters and only KB can run it. |
| **Phase 1** — "two concurrent sessions, zero duplicates, zero lost updates" | `keel-daemon/tests/concurrency.rs`: 16 concurrent sessions. **Fully verified** — this gate needs no human. | Nothing. |
| **Phase 2** — "≥9 of 10 unprompted sessions write to Keel; 0 duplicate projects" | Not substitutable. "Unprompted" is the entire claim, and a test that calls the tool has prompted it. | All of it. See `plugin/README.md` for how to run the ten sessions. |
| **Phase 3** — "UC-6 completes in under 30s" | The Sunday-review data all comes from one `keel_context` roll-up call, which returns in milliseconds against the fixture. | Whether *a human* can absorb it in 30 seconds. That is a question about the UI, not the query. |

---

## Changelog

One entry per session. Append; never edit history.

| Date | Session | Landed | Next |
|---|---|---|---|
| 2026-08-09 | — | Tracker seeded from PRD/SPEC. No code yet. | P0-1 |

---

## Conventions for maintaining this file

- Task statuses: `todo` · `in_progress` · `blocked` · `done` · `dropped`
- Move to `in_progress` **before** starting, not after.
- Task IDs are permanent and never reused, even for dropped tasks.
- Update "At a glance" every session — that table is what KB reads first.
- If a task splits, keep the original ID and add suffixed children (`P0-8a`, `P0-8b`).
- A session that achieved nothing still gets a changelog entry saying so.
