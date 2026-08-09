# Keel — Status

> **Maintained by Claude Code. Updated at the end of every session, without exception.**
> This file is the tracker until Keel can track itself (Phase 1 exit). Its shape mirrors Keel's own data model on purpose — importing it will be the first real end-to-end test.

---

## At a glance

| | |
|---|---|
| **Current phase** | Phase 0 — Spine |
| **Phase progress** | 6 / 16 tasks done |
| **Status** | In progress |
| **Blocked on** | Nothing |
| **Next up** | P0-7 (document revisions), then P0-9/P0-16 |
| **Last session** | 2026-08-09 |
| **Last updated** | 2026-08-09 |

**Scope for this stretch:** KB confirmed Phases 0–3 before going away. Git is local-only, no remote. `session_id` is a skill-minted per-conversation ULID (Q-8 answered). Human-gated phase exits get an automated proxy plus an honest note here — see "Phase gates I cannot verify" below.

---

## Phase 0 — Spine

**Goal:** `keel-core` can create, read, update and archive every entity type, with correct versioning, provenance, links and events. No network, no UI.

**Exit criteria:** all 13 entity types round-trip; event log correct; 200-entity fixture loads; graph-direction tests pass in both directions for every relation; backup restores and diffs clean.

| ID | Task | Status | Notes |
|---|---|---|---|
| P0-1 | Cargo workspace scaffold, CI, lint/fmt/deny gates | `done` | Five crates scaffolded per SPEC §1.1. CI runs fmt, clippy `-D warnings`, tests and `cargo deny`. unwrap/expect/panic are workspace clippy lints, so the definition of done is a build failure rather than review discipline |
| P0-2 | **Verify fast-moving dependencies** | `done` | **Verified against running code, not docs.** DuckDB 1.5.5 + Lance extension work end to end. DuckPGQ confirmed absent for 1.5.x (HTTP 404). MCP 2026-07-28 current. Two SPEC §5 syntax errors found and fixed in place. Nine MCP deltas recorded for Phase 1. Full table in DECISIONS.md |
| P0-3 | Domain types, ULID prefixes, the `<audit>` block | `done` | Thirteen types, prefixed ULIDs, audit block, relations, events, document revisions. 54 unit tests. Found: ULIDs need a monotonic generator or the §3.4 event cursor silently skips rows (B-9) |
| P0-4 | DuckDB schema + forward-only migrations | `done` | Forward-only migrations in `_keel_migrations`. `v_entities` built now rather than deferred (resolves TQ-4). PROVISIONAL: `idempotency_key` on all thirteen tables, not just tasks — see TQ-9 |
| P0-5 | Lance `documents` + `blobs` datasets, ATTACH wiring | `done` | Lance `documents` + `blobs` created through the DuckDB extension; no `lance` Rust crate needed (B-2). Found: Lance `CREATE TABLE` rejects all column constraints, so nullability is application-level — consistent with §2.1 |
| P0-6 | Entity storage layer — CRUD for all 13 types | `done` | CRUD for all 13 types behind `EntityStore`. Field patching goes through serde round-trip, so enum errors already name the valid values. 8 round-trip integration tests against real storage |
| P0-7 | Document revisions — append, fetch by version, diff | `todo` | `similar` crate for diffs. SPEC §2.1 |
| P0-8 | Links, `GraphStore` trait, CTE implementation | `done` | **21 tests, all 9 relations, both directions, plus both inversions.** `depends_on` normalises to `blocks` with swapped endpoints and never reaches the table. Cycle guard, depth clamp and shortest-path dedup all covered |
| P0-9 | Event log — append, query since cursor | `todo` | Append-only. SPEC §3.4 |
| P0-10 | Embeddings via fastembed | `todo` | Store `embedding_model` + `embedding_version`. Note first-run download |
| P0-11 | Hybrid search — Lance + DuckDB FTS, RRF fusion | `todo` | SPEC §5. `k_inner = k_outer * 4` |
| P0-12 | Backup: DuckDB **and Lance** → Parquet, restore | `todo` | Lance→Parquet is easy to skip and is the whole escape hatch. PRD R-5 |
| P0-13 | `keel-cli fsck` — cross-engine referential integrity | `todo` | SPEC §3.1. FKs can't be enforced, so this is the safety net |
| P0-14 | 200-entity fixture across all types and relations | `todo` | Realistic, not `foo`/`bar`. It's the search-quality corpus too |
| P0-15 | Test suite: concurrency, idempotency, OCC, round-trip | `todo` | Concurrency test written now as `#[ignore = "unblocks in Phase 1"]` so CI stays green |
| P0-16 | Implement idempotency keys and optimistic concurrency | `todo` | SPEC §7.2, §7.3. P0-15 tests these; nothing else builds them. 409 payload uses `latest_version` |

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
