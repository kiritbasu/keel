# Keel — Status

> **Maintained by Claude Code. Updated at the end of every session, without exception.**
> This file is the tracker until Keel can track itself (Phase 1 exit). Its shape mirrors Keel's own data model on purpose — importing it will be the first real end-to-end test.

---

## At a glance

| | |
|---|---|
| **Current phase** | Phase 0 — Spine |
| **Phase progress** | **16 / 16 — Phase 0 complete** |
| **Status** | Phase 0 exited; Phase 1 in progress |
| **Blocked on** | Nothing |
| **Next up** | Phase 1 — the daemon and the nine MCP tools |
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

Nothing yet.

*(Format: task ID — what's blocking — who or what unblocks it — since when.)*

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
