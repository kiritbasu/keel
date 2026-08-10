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

## Step 1 — the validity audit, and the number was wrong

Outside panel review (`product/WAY-FORWARD.md`) said the measurement was invalid. Step 1 audited it against the archived Claude Code transcripts, which survived teardown — 41 gate sessions recoverable in full. Zero build. Output: `product/VALIDITY-AUDIT.md`.

**Run 4 was 5 of 10, not 3.** `keel gate` counts distinct `session_id` values; five sessions called `keel_create` and two pairs collided on date-based ids. Runs 1–3 were reported accurately — the collision can only bite when more than one session writes.

| Run | Condition | Wrote | Never touched Keel |
|---|---|---|---|
| 1 | live store, cold | 1 | 7 |
| 2 | Tideline archived | 0 | 6 |
| 3 | empty scratch store | 0 | 6 |
| 4 | SessionStart hook | **5** | **3** |

**The trend is 1 → 0 → 0 → 5**, and orientation moved as much as writing did: sessions never touching Keel fell 7, 6, 6 → 3.

Five checks, all closed:

- **The permission-allowlist confound is dead.** No `keel_*` call was ever denied. That was the check that could kill a confound without spending a run, and it did. Two writes failed on *validation* instead — `priority: "high"` and `"medium"` against an enum of `p0`–`p3` — and both retried successfully. That is the only organic evidence on the enrich-vs-collapse schema question, and it says at least one field is wrong about how a model thinks.
- **No `--permission-mode`, no `.claude/` in either scratch project.** Nothing silently permitting or denying.
- **Single-turn confirmed.** `claude -p … </dev/null`, no continuation flags. The panel's central claim holds: *"I'll hold off until you say go"* addressed a turn that could not exist.
- **All four post-run-4 fixes landed 18–84 minutes after it.** One correction to the panel: a one-sentence anti-asking instruction *was* live in run 4, so the weak form has a number and it is 5, not 3. The expanded form is unmeasured.
- **Transcripts survived teardown** and are the archive of record, not the `tee`'d logs.

**What it changes.** The gap to the bar is 5→9, not 3→9. "The premise may be dead" rested on 3/10, which was wrong, from an instrument that cannot resolve 55% from 100%. And the `--sessions` denominator fix does not finish the job — the numerator is still model-minted ids, so **the launcher must inject the session id** before any further run.

---

## What the seven silent sessions actually did

Read all ten transcripts. **The silent seven were not unaware of Keel. Five of them worked out exactly what to record, drafted it, and stopped to ask.**

> *"This looks like a real open risk for Tideline and it isn't tracked yet — want me to log it as an open question in Keel? I'll hold off until you say so."*

> *"Want me to log the open design question so it's not lost? I'll hold off until you say go."*

Eleven separate offers to write across the ten transcripts. The session that asked about the chart-datum risk had done the hard part — identified a genuine safety issue and drafted the question. Only the write was missing.

**Why it survives every instruction:** the offer looks like good manners. But the human is mid-conversation about code; they do not want a second decision about bookkeeping, they want the thing not lost. Asking turns a free write into an interruption, and an ignored interruption into a lost record. Now addressed in both the skill and the hook's preamble, with the measurement attached — the test is *"did something become true?"*, not *"have I been authorised?"*

**Two real bugs the transcripts exposed, which is why reading them was worth more than rerunning:**

- **A redundant slash defeated project matching.** A session reported `matched_project: null` for a directory that had a project — `cwd` had `T//keel-gate`, `root_path` had `T/keel-gate`, and a naive prefix comparison called them different. So some sessions started *unoriented* despite a project existing, and 3 of 10 understates the hook. Fixed with normalisation and two tests. Caught only because one session mentioned the null in passing.
- **Session ids collide.** Sessions minted `tideline-2026-08-09` — date-based, not conversation-based — so two sessions in a day merge into one row and the gate undercounts.

**An evidence gap I created:** one session said "Logged as an open question on Pellet" and I had already torn down the scratch store, so I cannot tell whether that write landed or was only claimed. A session reporting a write it did not make is worse than a silent one, and I destroyed the record that would say which. Next run keeps the store until the transcripts are read.

---

## The SessionStart hook: 3 of 10, up from 0–1

Built, installed, and measured against a fresh gate run — ten sessions, scratch store, both projects pre-created so the cold-start question was held constant.

| | Before the hook | With the hook |
|---|---|---|
| Sessions that wrote | 0–1 of 10 | **3 of 10** |
| Every write attributed | — | yes |
| Duplicate projects | 0 | 0 |
| Verdict | invalid (skill never loaded) | **FAIL** — needs 9 of 10 |

**Orientation is solved. Writing is not.** Sessions now arrive knowing what the project is; seven of ten read the digest and recorded nothing anyway. The failure has moved from *"never heard of Keel"* to *"oriented and chose not to record"* — a judgement problem in the remaining `SKILL.md` rather than a plumbing one, and a much narrower target.

The three that did write minted readable session ids of their own (`tideline-2026-08-09`) and wrote one to three artifacts each. When they engage, they engage properly.

**Also fixed, found by watching this run:** `keel gate` counted its denominator from sessions present in the event log. A session that writes nothing leaves no event, so it read "3 of 3" after seven silent ones — the scorer structurally could not express the failure it exists to detect. It now takes `--sessions` and reports the silent count.

**The runner is sequential**, which is why each run costs 15–20 minutes: ten ordinary Claude conversations in a queue. They could run concurrently and finish in about two. Not done.

---

## The skill does not fire. Phase 2's mechanism does not work.

An interactive session, scratch project, skill installed at `~/.claude/skills/keel/`, MCP server registered user-scope. Prompt:

> `we should cache the constituent lookup, it gets recomputed on every height() call`

"we should" is listed verbatim in `SKILL.md`'s own trigger description. The session searched one pattern, read one file, gave a good technical answer, and **never called `keel_context`, never invoked the skill, never mentioned Keel.**

With TQ-18 — thirty headless sessions, zero `Skill` invocations — that is both surfaces. The skill is discoverable and simply not reached for.

**This is the finding the whole evening was for.** PRD R-2 is "the agent doesn't write to it" and the mitigation on record is "skill and hooks are the product". The skill half does not work, and not in a way rewording fixes: a skill is model-invoked, so the text inside a file nobody opens is not yet in play.

**The mechanism that would work is a `SessionStart` hook** that calls `keel_context` and injects the digest unconditionally — orientation as something that happens *to* the session rather than something it must choose. The plugin already ships a `PostToolUse` hook, so the machinery exists. Pair it with a much shorter skill: the orientation half becomes unnecessary, and what remains is *when to write*, which is the real judgement.

Recorded as **TQ-19** with a p0 task. Until that exists, Phase 2's criterion cannot pass — and everything built on top of Keel is scaffolding around an empty store.

---

## Phase 2's gate: three runs, all invalid

**Retracting what the section below says.** Thirty sessions across three runs, and none of them loaded the skill. Proven with `--output-format stream-json`, which lists the tools a session actually invoked:

```
tools invoked: ['ToolSearch', 'mcp__keel__keel_context', 'mcp__keel__keel_search']
```

No `Skill` invocation. `SKILL.md` was installed, listed as available, and never read. So the runs measured *"will Claude reach for nine MCP tools with no instructions"* — not the claim Phase 2 makes. `plugin/README.md` said from the start that this gate cannot be automated. It was right and I built the harness anyway.

| Run | Store | Result | Valid? |
|---|---|---|---|
| 1 | live, cold | 1 of 10 wrote | no — skill never loaded |
| 2 | live, Tideline archived | 0 of 10 | no — plus an archived near-match I created |
| 3 | empty scratch store | 0 of 10 | no — skill never loaded |

**What the runs do establish**, and it is not nothing:

- A baseline: with the tools present and no skill, ~3 in 10 sessions engage with Keel and ~0–1 in 10 write.
- The `cwd` addition to `keel_context` works. Sessions now say "`keel_context` matched nothing for this checkout" rather than inferring absence from a list of other projects. That change was right and it landed.
- **Even knowing no project matched, sessions asked permission instead of creating one** — three times, in three runs, with no skill text telling them to. So the binding constraint is the model's own caution, and wording in `SKILL.md` may not be enough to clear it. That is a more interesting finding than the gate score.

Recorded as **TQ-18** (`que_01KZM6XPTV6GC2R12N75B9JQ5X`). Next step is one interactive session to establish whether skills load there at all, before spending ten.

---

## Phase 2's gate: run, and failed 1 of 10

Run 2026-08-09, headless, ten sessions across two scratch projects that mention Keel nowhere, against real Claude with the skill loaded.

| Criterion | Required | Result |
|---|---|---|
| Sessions that wrote | 9 of 10 | **1 of 10 — fails** |
| Every write attributed | yes | yes |
| Duplicate projects | 0 | 0 |

> **Superseded by TQ-18 — the claim in this paragraph is wrong.** The skill never loaded; sessions were reaching for an available MCP tool, not following it.

**The skill is not the problem — it fires.** Sessions called `keel_context`, understood what Keel was for, and said so:

> Session 3: *"There's no Keel project for `tideline`. `keel_context` only knows the 'Keel' project, so I've recorded nothing there. Tell me which project to file it under and I'll create it."*

> Session 8: *"Want me to log this…? I held off writing since you haven't actually decided — just say the word."*

Session 6 created the Tideline project and a task, unprompted. The write path works.

**The cause is the duplicate-project defence deadlocking a cold start.** `SKILL.md` says confirm with the human before creating a project, because nine projects for one thing ruins the cross-project view (UC-8). In an existing project that is right; in a new one there is nothing to write into, so the sessions stop and ask. The two rules are each correct and jointly produce silence — and the 0 duplicates is the *same instruction* succeeding at its own job while blocking the one being measured.

**Instrument caveat:** headless single-turn sessions. Several ended by asking permission; a real conversation would have answered and the write would have followed. **1/10 is a lower bound.** The cold-start deadlock is real regardless, because it fires before any answer is possible.

Full write-up and three options in **TQ-17** (`que_01KZM3Y941G7MB1BW0KKZ5M3P4`). Recommendation: let a session create the *first* project for the directory it is working in without asking, and have `keel_context` say plainly that no project matches. Neither touches the case UC-8 protects.

Getting the gate to run at all took four attempts — a stale CLI login, then advice of mine that sent an OpenRouter token to api.anthropic.com, then unrelated MCP servers taking a minute to boot per session, then a hidden prompt swallowed by command substitution. Root cause of the middle two: `~/.zshrc` routes Claude Code through OpenRouter, and the desktop app overrides that for the shell the agent gets — so identical commands behaved differently on each side and the agent's probes proved nothing. `scripts/gate-run.sh` now refuses to start in that shell.

---

## The roadmap does not say what is next

KB, after the logs were fixed: *"I don't understand what's next to build in the project, it just doesn't make sense looking at the roadmap or board. Is that a problem just because we are building this as we go along, or is it something we need to fix?"*

Mostly the product. Recorded as **TQ-16** (`que_01KZKX1PEJ3M0N4MYHRNB1JKKC`) with three options and a recommendation. In short:

| Finding | Product, or us? |
|---|---|
| `keel_context.next` returns counts and advice, never a named task | **Product.** The one question a spine exists to answer |
| `blocked` had no referent — 3 blocked tasks, 0 `blocks` edges, and the digest advised a query returning nothing | **Product.** Data fixed; nothing prevents the state recurring |
| No ordering anywhere; "ready" and "waiting on a human" share a board column | **Product** |
| Six of ten open tasks parked in whichever milestone was `active` | **Mine** — but nothing discourages it, and a busy human will do the same |
| Every phase target date is today | **Us.** A real project has real dates |

Fixed in the data this session: the ten-session gate now `blocks` all three Phase 4/5 tasks, TQ-6 blocks the design work, and two tasks moved to the milestone they actually belong to. "What is blocking this" and "what does this unblock" both answer now — the ten-session gate releases three.

**KB chose option (a), and it is built.** `keel_context` now opens with a `## Next` section — at character 126 of the digest rather than 7,360, which is the part that changes an agent's behaviour — naming ranked tasks with the reason attached:

```
## Next
- **Run the ten unprompted sessions** `tsk_…` — unblocks 3 other tasks · p0
- **Render the tracker from the task rows** `tsk_…` — nothing is blocking it · p1
- **Route the remaining read commands through the daemon API** `tsk_…` — nothing is blocking it · p1
```

Three buckets, because they need different responses: **ready**, **waiting on the human** (the three `Decide TQ-x` tasks, which nobody else can start), and **blocked**, each with its blocker named. Ready is ordered by what it unblocks first, then priority — a p1 releasing three tasks beats a p0 releasing none, and the count comes from edges a human drew.

The ranking lives in `keel-core::next` and the desktop board reads the same call, so the app and the agent cannot disagree. The board grew a Next banner and rank badges, and columns sort by rank — a column displaying "3" above "1" was showing a ranking and contradicting it in the same breath.

Nine tests cover the ordering, both blocked cases, the finished-blocker release, and the decision/work split. 300 tests total. Decisions B-23, B-24, B-25.

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
