<!-- keel:generated spec spc_01KZKSMDZCHZXY4HMBCMYEVT3H
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — Status

> **This file is generated.** Keel is the source of truth; `keel generate keel` writes it. Edit the prose in Keel, not here — see `product/CLAUDE.md`, "Keel is the source of truth".
>
> It is still *stored* prose rather than rendered from the task rows, because the rows carry no per-task notes and those notes are most of what makes this worth reading. Finishing that is TQ-14.

---

## At a glance

| | |
|---|---|
| **Current phase** | Phase 3 — Desktop (complete). Dogfooding on the real store |
| **Phase progress** | Phase 0: **16/16** · Phase 1: **14/14** · Phase 2: **7/8** · Phase 3: **13/13** |
| **Status** | Phases 0–3 built. Keel is the source of truth; every `product/*.md` is generated from it |
| **Blocked on** | Nothing. TQ-14 (render the tracker from task rows) is the last half-step of the dogfooding switch |
| **Warning** | **R-2 observed live** — session 3 shipped four features and moved zero task rows. See "The session that did not write to Keel" below |
| **Next up** | KB: run the Phase 2 ten-session gate. Then Phase 4 (GitHub) — needs your account |
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
| P1-13 | Route reads through the daemon's API | `done` | **Found by using it:** nothing could read the store while the daemon held the write lock. Tried the obvious fix first — a read-only connection — and it fails identically, because DuckDB blocks readers too. So D-5's "connect read-only" is not a thing that exists, and generation moved into the daemon as `POST /api/generate` (B-21). The CLI is a client, falling back to a direct open only when no daemon answers. Spec correction flagged as TQ-15 |
| P1-14 | `POST /api/generate` | `done` | The daemon owns the store, so it owns generation. Resolves the read half of TQ-12; writes from the CLI still need the daemon down, which is now only `keel import` and `keel bootstrap` — both one-time migrations |

---

## Phase 2 — Plugin

**Goal:** Claude writes to Keel without being told to.

**Exit criterion:** across 10 unprompted sessions, Claude writes to Keel in ≥9, threads `session_id` on every write, and creates 0 duplicate projects.

> **This gate cannot be automated and has not been run.** "Unprompted" is the whole claim, and a test that calls the tool has prompted it. `plugin/README.md` has the protocol for running it by hand, including what each failure mode looks like and which part of `SKILL.md` to change. Everything else in this phase is built and verified.

| ID | Task | Status | Notes |
|---|---|---|---|
| P2-1 | Markdown mirror generator | `done` | Prose only (TQ-5). Skips unchanged files by comparing content with the timestamped header stripped, so regenerating does not dirty the tree. A test asserts this module contains **no** way to read a mirror as truth — the absence is the enforcement of D-3 |
| P2-2 | The skill | `done` | `plugin/skills/keel/SKILL.md`. Leads with orientation, then session threading, then a when-this-happens-write-that table. Names the two most-skipped artifacts (questions, decisions) and the shredding failure explicitly |
| P2-3 | Session-ID threading | `done` | Third section of the skill, with the self-check: `keel_context` echoes the id back, and `null` means it is not threading |
| P2-4 | Project-confirmation behaviour | `done` | `keel_projects` returns `requires_confirmation` on a near miss; the skill makes this the one place the agent must stop and ask rather than get on with it |
| P2-5 | `PostToolUse` hook for mirror edits | `done` | **Verified end to end against a running daemon:** an edit to a generated spec became revision 2, attributed to the session. Refuses aggregate files, unheadered files and non-mirror paths; reports a down daemon without failing the edit |
| P2-6 | MCP config and install script | `done` | `plugin/.mcp.json`, `plugin/install.sh`. The installer prints the configuration rather than editing anyone's settings file |
| P2-7 | The ten-session protocol | `done` | `plugin/README.md`. What to measure, how to measure it, and what each failure mode means |

---

## Phase 3 — Desktop

**Goal:** the Sunday review works.

**Exit criterion:** UC-6 completes in under 30 seconds.

| ID | Task | Status | Notes |
|---|---|---|---|
| P3-1 | Tauri v2 shell, daemon as a sidecar | `done` | Starts the daemon only if one is not already running, and only kills what it started — terminating someone else's daemon on window close would take out whatever they had pointed at it |
| P3-2 | Typed API client + live refresh | `done` | Relative paths in dev (Vite proxies), absolute baked in at build time for the webview. One bundle, different base URL, as SPEC §10 wants. SSE `lagged` is surfaced rather than swallowed |
| P3-3 | Screen 1 — Home | `done` | At-risk projects sort first; every number visible without a click. **Found and fixed: the cross-project roll-up never populated `recent`**, so the Sunday review said "no activity yet" against a store with 500 events |
| P3-4 | Screen 2 — Project dashboard | `done` | The same data `keel_context` gives an agent. If a human and a model see different summaries of one project, one is wrong and nobody knows which |
| P3-5 | Screen 3 — Roadmap | `done` | Built from milestones. Dated first in date order, undated after — a milestone with no target is unplanned, not far-future |
| P3-6 | Screen 4 — Board | `done` | **Found and fixed: a six-column grid with per-column min-widths overflows its tracks rather than scrolling**, so each column's cards landed on top of the next column's heading |
| P3-7 | Screen 5 — Documents | `done` | Reader, revision picker, side-by-side diff, and the link graph. The diff is why this screen exists rather than being a markdown viewer |
| P3-8 | Screen 6 — Search | `done` | Faceted by type, scoped by project. Hits say which index found them, so "is the semantic half earning its keep" stays answerable (R-3) |
| P3-9 | Screen 9 — Activity | `done` | Filterable by actor. Writes with no `session_id` are marked `unattributed` — that count is what Phase 2's gate is about |
| P3-10 | Keyboard navigation | `done` | Digits switch screens, `/` jumps to search. **Found and fixed: the navigation keypress leaked into the newly-focused search input** |
| P3-11 | `keel import` — whole markdown files into Keel | `done` | KB asked whether specs could live in the store and be read in the app. They can: SPEC.md imports at 51,695 bytes and round-trips byte-identical, searchable and diffable. Re-importable and content-addressed, so an unchanged file appends no revision (B-18). All seven `product/*.md` are now in `~/.keel` |
| P3-13 | `keel generate` — the repo files become outputs | `done` | KB's call: Keel is the source of truth. A prose artifact records the file it *is* (`mirror_path`) and generation writes its body there verbatim under an HTML-comment banner (B-20). All seven `product/*.md` round-trip byte-identical — proved by generating over them and diffing: the only change was the banner. `--check` exits non-zero on drift, for a pre-commit hook. **Found and handled: a path claimed by both a document and the tracker** — neither is written and the conflict is reported, because letting the last writer win is how a file silently loses half its content (B-22) |
| P3-12 | Rendered markdown in the reader | `done` | The reader was showing bodies as preformatted text, which made a real spec unreadable — the point of storing it. `react-markdown` + `remark-gfm`, no raw HTML (B-19). Verified against the imported 51 KB SPEC: headings, fenced code, blockquotes and the §3.3 direction table all render |

---

## Later phases

Not broken down yet. Decompose at the start of each phase, not before — earlier phases will change what the later ones need.

| Phase | Scope | Gate |
|---|---|---|
| 1 — Daemon | axum, 9 MCP tools, `keel_context`, concurrency safety, `keel-cli render-status` for the dogfooding switch | Live Claude session completes UC-1→UC-4; 2 concurrent sessions, 0 duplicates, 0 lost updates |
| 2 — Plugin | Skill, session-ID threading, project confirmation, mirror hooks | ≥9 of 10 unprompted sessions write to Keel; 0 duplicate projects |
| 4 — Integrations | GitHub App, design artifacts, metrics, screens 7–8 | PR merge proposes closure |
| 5 — Remote | Deployable daemon, auth, mobile client | Status readable from phone |

**Phase 1 exit is also the dogfooding switch.** At that point, import this file, `product/DECISIONS.md` and `product/QUESTIONS.md` into Keel as its first project, and Keel becomes the tracker.

---

## Blocked

Nothing.

*(Format: task ID — what's blocking — who or what unblocks it — since when.)*

---

## The prose blob problem

KB, later the same session: *"How come I'm not seeing TQ-15 or any of the other upcoming tasks on the boards, what's missing?"*

TQ-15 did not exist. It was a row in a markdown table inside a document body — and a document body is one artifact, however many things are written in it. Counted properly:

| | In the prose | As artifacts | Missing |
|---|---|---|---|
| Decisions | 22 | 12 | 10 |
| Questions and risks | 28 | 12 | 16 |

The twelve of each were what `keel bootstrap` seeded that morning. **Everything written since — five decisions, four questions, one risk — went into a markdown table and nowhere else.** Invisible to the board, unrankable by search, unlinkable, absent from `keel_context`. The same failure as the task one, one layer up, and with the same cause: `keel import` stores a document *whole*, which is right for a spec and wrong for a log of numbered rows.

Fixed by decomposing both tables into artifacts through MCP. Now 28 questions and 23 decisions, each question titled with its canonical id (`TQ-15 — …`) so the two representations can be matched by eye.

Three things the migration turned up, all of them the system working:

- **Accepted decisions could not be retitled.** The store refused with D-6: content is immutable, supersede instead. So decisions keep their titles and carry the id in the body. The constraint was right and the plan was wrong.
- **Fuzzy title matching proposed re-creating B-3, B-9, B-13 and B-17**, which already existed under paraphrased titles. Caught by a dry run. Replaced with a hand-checked map of twelve — the exact duplicate-artifact failure the exercise was meant to remove.
- **One decision exists as an artifact with no row in the log at all**: "Fixture links are addressed by name, never by position". The mirror image of the gap. Left alone rather than given an invented id.

**What is still not fixed:** nothing prevents this recurring. A new numbered row added to either table tomorrow will again exist only as prose. `keel generate --check` compares files to the store; there is no check that the rows *inside* a file exist as artifacts.

---

## The session that did not write to Keel

KB asked, part-way through session 3: *"I am not seeing the actual project task items getting updated from this chat, is it wired correctly?"*

It was wired correctly. That was the problem.

Of the 134 events in the store at the moment he asked, 119 came from `keel bootstrap`, 13 from `keel import`, and 2 from one-off `curl` calls. **Not one task row changed status during a session that shipped four features.** The tracker prose was accurate, because it was hand-written and imported; the task rows were frozen at what bootstrap wrote at 16:11 that morning. The app showed the truth and the markdown did not.

This is R-2 — "the agent doesn't write to it" — happening to the agent building the thing, with the MCP surface connected and the project's own contract instructing otherwise. Recorded as a `risk` in Keel (`que_01KZKW33RGTJY86XYDTHSPMF0C`) because it is evidence about the product rather than about one session. Three things it says:

- Wiring was never the issue. The tools worked on the first call, before and after.
- The pull toward editing a markdown file beat a tool surface designed specifically to replace it.
- **Nothing complained.** Commits accumulated, `--check` stayed green, and the drift was invisible until a human opened the app.

The second point is the one the PRD is betting against. The third is cheap to fix and is not fixed: `keel generate --check` fails on stale *prose*, and there is no equivalent that fails when a session produces commits and no task mutations.

The proper answer is the Phase 2 skill and hooks, which is exactly what the ten-session gate measures — and this session is a data point that the gate matters and would currently fail.

---

## Phase gates I cannot verify

KB's instruction was to substitute an automated proxy for the human-in-the-loop gates, proceed, and record honestly what is unverified. This is that record.

| Gate | Mechanical proxy — **passing** | What remains unverified |
|---|---|---|
| **Phase 1** — "a live Claude session completes UC-1 → UC-4" | `keel-daemon/tests/use_cases.rs`: 21 tests driving real HTTP, real headers, real JSON-RPC against a real daemon and a real store. All four use cases complete. | That the *tool descriptions* lead a model to pick the right tool unprompted. A scripted client is told which tool to call; an agent is not. This is the half that matters and only KB can run it. |
| **Phase 1** — "two concurrent sessions, zero duplicates, zero lost updates" | `keel-daemon/tests/concurrency.rs`: 16 concurrent sessions. **Fully verified** — this gate needs no human. | Nothing. |
| **Phase 2** — "≥9 of 10 unprompted sessions write to Keel; 0 duplicate projects" | Not substitutable, and not attempted. "Unprompted" is the entire claim, and a test that calls the tool has prompted it. The *mechanism* is verified: the mirror hook round-trips an edit into an attributed revision against a live daemon. | Whether the skill actually fires. This is the phase the PRD calls the real test of the premise, and it is the one thing here that only KB can run. `plugin/README.md` has the protocol. |
| **Phase 3** — "UC-6 completes in under 30s" | The Sunday-review data all comes from one `keel_context` roll-up call, which returns in milliseconds against the fixture. | Whether *a human* can absorb it in 30 seconds. That is a question about the UI, not the query. |

---

## Changelog

One entry per session. Append; never edit history.

| Date | Session | Landed | Next |
|---|---|---|---|
| 2026-08-09 | — | Tracker seeded from PRD/SPEC. No code yet. | P0-1 |
| 2026-08-09 | 1 | **Phases 0–3.** Storage spine, MCP daemon with nine tools, Claude Code plugin, Tauri desktop app. 264 Rust tests, nothing ignored. All four CI gates green. Twelve build-time decisions and five spec corrections recorded. Two gates left unrun because they need a human — see "Phase gates I cannot verify". | KB: run the Phase 2 ten-session gate. It is the one that tests the premise. |
| 2026-08-09 | 3 | **Keel is the source of truth.** KB's call, in one line, after seeing the specs render in the app. Prose artifacts now record the repository file they are, and `keel generate keel` writes all seven `product/*.md` from Keel — verified byte-identical over the existing files, banner aside. Generation moved into the daemon, because the read-only escape hatch D-5 assumes turns out not to exist in DuckDB at all. `--check` makes drift a build failure. 291 tests. **One half-step left (TQ-14):** the tracker is authoritative in Keel but still stored prose, not rendered from the task rows — the rows have no per-task notes yet. **Then KB caught the thing that matters more:** this session wrote nothing to the task rows all day. Fixed — the work is in Keel now, through MCP — and recorded as a high-severity risk, because it is R-2 observed live. | KB: the Phase 2 ten-session gate. This session is evidence it is not a formality. |
| 2026-08-09 | 2 | **Made it real on KB's own machine.** `bundled` DuckDB became a feature (B-3, at KB's push); the daemon learned the legacy 2025-11-25 handshake so Claude Code actually connects (B-17); `keel bootstrap` seeded Keel's own project and archived the sample ones; roadmap ordering fixed. Then `keel import` and a real markdown reader (B-18, B-19), so all seven `product/*.md` live in the store and read whole in the app. 278 tests. **Two things want KB: TQ-12** (the CLI cannot write while the daemon is up — contradicts D-5) **and TQ-13** (do the repo files become generated outputs, or stay authoritative?). | KB: the Phase 2 ten-session gate, still. Then TQ-13, which gets more expensive the longer the two copies drift. |

---

## Conventions for maintaining this file

- Task statuses: `todo` · `in_progress` · `blocked` · `done` · `dropped`
- Move to `in_progress` **before** starting, not after.
- Task IDs are permanent and never reused, even for dropped tasks.
- Update "At a glance" every session — that table is what KB reads first.
- If a task splits, keep the original ID and add suffixed children (`P0-8a`, `P0-8b`).
- A session that achieved nothing still gets a changelog entry saying so.
