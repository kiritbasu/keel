<!-- specline:generated spec spc_01KZKSMDY6329PQKCHC3M0YGX4
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# Keel — Handoff to Claude Code

> Read this once, at the start of the project. After that, `product/CLAUDE.md` is the standing contract and `product/STATUS.md` is where the work lives.

---

## What you're building

Keel is a local-first store for everything that describes a software project other than the code: PRDs, specs, decisions, tasks, bugs, milestones, roadmap, design artifacts, environments, metrics, risks, and customer feedback.

It has two faces. **An MCP server** is the primary interface — Claude reads and writes to it constantly from chat, Cowork, and Claude Code. **A Tauri desktop app** is the secondary interface — a human read/search surface.

Read `product/PRD.md` for what and why. Read `product/SPEC.md` for how. Both are detailed and both have been adversarially audited; treat them as authoritative unless you find a defect, in which case see "When the spec is wrong" below.

**The user is KB.** Solo developer. The only other actor is Claude. There is no team, no permissions model, no assignees.

---

## Read order for your first session

1. `product/HANDOFF.md` (this file) — orientation
2. `product/CLAUDE.md` — the rules you'll operate under
3. `product/STATUS.md` — current state and the task list
4. `product/PRD.md` §6 (artifact model), §8 (requirements), §10 (phasing)
5. `product/SPEC.md` §1–§4 (architecture, storage, data model, graph)

Skim the rest of `product/SPEC.md`. Read §5–§9 properly when you reach the phase that needs them.

---

## What is already decided — do not relitigate

`product/SPEC.md` §13 lists twelve decisions (D-1 … D-11, including D-2b). They were argued through at length with KB. Treat them as settled:

| | Decision |
|---|---|
| D-1 | DuckDB + Lance. Not SQLite. |
| D-2 | One unified Lance `documents` dataset for all prose, not one per type. |
| D-2b | Revisions in user columns, not Lance dataset versions. |
| D-3 | Database canonical; the git markdown mirror is a generated export. |
| D-4 | Recursive CTEs for graph. Not DuckPGQ, not FalkorDB. |
| D-5 | The daemon owns the single write path. |
| D-6 | Rust + Tauri. |
| D-7 | Local embeddings via fastembed. |
| D-8 | Propose task closure on PR merge; never auto-close. |
| D-9 | Soft delete only, links included. |
| D-10 | `session_id` is caller-supplied, never daemon-invented. |
| D-11 | `blocks`/`depends_on` normalised to one stored direction. |

If you believe one is wrong, record a question in Keel and ask KB. Do not quietly implement something else.

---

## Where to start

**Phase 0 is already broken into 16 tasks in `product/STATUS.md`.** Start at P0-1.

But **do P0-2 before anything that depends on it.** Several claims in `product/SPEC.md` come from fast-moving sources and were true when written in August 2026. Re-verify them against primary documentation before you build on them:

- The Lance extension for DuckDB — availability, version compatibility, and whether `lance_hybrid_search()` / `ATTACH … (TYPE lance)` work as described in `product/SPEC.md` §5.
- DuckDB's current version and whether the Lance extension and DuckPGQ can now coexist (if they can, record it as a question — it doesn't change D-4 for v1, but it changes the contingency cost).
- Quack's status. `product/SPEC.md` §7.1 deliberately does *not* depend on it; confirm that's still the right call.
- The current MCP spec version, its transport model, and the header names in `product/SPEC.md` §6. If the spec has moved past 2026-07-28, build against the current one and record the delta.
- `fastembed-rs` current version and model availability.

Write what you find into `product/DECISIONS.md`. If any of it invalidates part of `product/SPEC.md`, stop and ask KB before proceeding — a wrong storage layer is expensive to unwind.

---

## Phase gates

Do not start a phase before the previous one meets its exit criteria (`product/SPEC.md` §12).

| Phase | Exit criteria |
|---|---|
| **0 — Spine** | All 13 entity types round-trip; event log correct; 200-entity fixture loads; graph-direction tests pass in both directions for every relation; backup round-trips (back up → wipe → restore → diff clean) including the Lance→Parquet dump |
| **1 — Daemon** | A live Claude session completes PRD UC-1→UC-4; two concurrent sessions produce zero duplicates and zero lost updates |
| **2 — Plugin** | Across 10 unprompted sessions, Claude writes to Keel in ≥9, threads `session_id` on every write, creates 0 duplicate projects |
| **3 — Desktop** | UC-6 (Sunday review) completes in under 30s |
| **4 — Integrations** | PR merge proposes closure; design proposed-vs-built renders |
| **5 — Remote** | Project status readable from phone |

**Phase 2 is the real test of the premise.** If Keel isn't useful after Phase 2 with no UI at all, the idea is wrong and a UI won't rescue it. Do not write a line of the desktop app before Phase 2 passes. If you find yourself wanting to — that's a signal the daemon isn't good enough yet.

---

## The bootstrap problem, and how it resolves

KB wants to see project status. Keel is the tool for that. Keel doesn't exist yet.

So: **`product/STATUS.md` is the tracker until Keel can track itself.** It is deliberately shaped like Keel's own data model — tasks, phases, decisions, questions, a changelog — so that when Phase 1 lands, importing it is a genuine end-to-end test rather than a throwaway migration.

**The moment Phase 1 exits, Keel tracks its own development.** Import `product/STATUS.md`, `product/DECISIONS.md`, and the open questions into Keel as the first real project, with Keel as the source of truth from then on. **Done 2026-08-09**; every file under `product/` is now an output.

Note the mechanism, because it isn't quite the mirror: `product/SPEC.md` §8's mirror is prose-only (TQ-5), so it covers decisions and questions but *not* tasks or status. Regenerating `product/STATUS.md` therefore needs a dedicated `keel-cli render-status` command. Add it as a Phase 1 task when you decompose that phase.

Dogfooding is not a nice-to-have here. It's the fastest way to find out whether the write protocol is actually pleasant to use, and it's the reason Phase 1's exit criteria are written the way they are.

---

## When the spec is wrong

It will be, somewhere. Both documents were audited hard, but 41 defects were found in the first draft and it would be optimistic to think the second is clean.

When you find one:

1. **Don't silently work around it.** A workaround that contradicts the spec creates a third source of truth in your head.
2. If it's an obvious editorial error (typo, wrong section reference, a column named two ways), fix the spec in place and note it in the changelog.
3. If it's substantive (a design that doesn't work, a query that can't be written, a dependency that doesn't exist), record it as a question in Keel with the specific failure, and ask KB.
4. If it blocks you and KB isn't around, implement the smallest thing that unblocks you, mark it `PROVISIONAL` in the code with a comment pointing at the question, and flag it prominently in `product/STATUS.md`.

Two areas most likely to be wrong, based on the audit: **the exact Lance/DuckDB integration syntax** in §5, and **the MCP transport details** in §6. Both were written from documentation rather than from running code.

---

## Traps specific to this project

**Graph query direction.** The first draft of the spec had both traversals inverted. An inverted graph query returns an empty result set, which is indistinguishable from a legitimate "nothing links here." This is the single nastiest bug class in the codebase. `product/SPEC.md` §3.3 has the normative direction table; every relation gets an explicit test asserting direction in both directions.

**Silent truncation.** Any list that can be cut must report that it was cut. An agent that receives 10 of 40 open questions with no indication will confidently re-litigate settled decisions.

**Schema creep.** Thirteen artifact types is at the ceiling, not a starting point. `product/PRD.md` R-1 names this as the most likely cause of death. No new types for six months without deleting one — and if you feel one is missing, it's almost always a field or a `kind` value on an existing type.

**Over-engineering for scale that doesn't exist.** This is a single user with maybe thousands of rows. There is nothing to optimise. If you catch yourself adding a cache, a queue, or a connection pool, stop. Correctness, clarity, and good ergonomics for the agent are the only things that matter here.

**The mirror is not a sync.** One direction. The single permitted read is the event-triggered hook in `product/SPEC.md` §8.1. Any code that compares mirror state to database state and merges is a bug, regardless of how reasonable it looked.

---

## What to ask KB about, rather than deciding yourself

`.keel/questions.md` is the authority on which questions are open and which are settled. Do not re-derive the classification from this file — it is generated from the question rows and this one is not.

It has two halves. **Open** has no working answer: raise it, or say explicitly what you are assuming. **Settled** is decided with the reasoning attached — proceed on it, and only reopen if implementation shows the reasoning was wrong.

The list here used to name specific questions and went stale within a day. That is why it now names the file instead.

Anything reversible, decide yourself and record it in `product/DECISIONS.md`. Anything that changes the storage format, the MCP tool surface, or the phase order — ask.

One clarification on spec edits, because two rules here can appear to collide: `product/CLAUDE.md` says to ask KB about anything touching storage format, and this file authorises fixing editorial errors in the spec directly. The line is **semantic vs editorial**. If the spec names one column two different ways, pick the one the schema in §3.2 uses, fix the other reference, note it in the changelog — that's editorial. If the spec needs a column *added*, *removed*, or *retyped*, that's storage format. Ask.

---

## Assumptions made in this handoff

Flagging these so KB can correct them cheaply:

- Repo name `keel`, single Cargo workspace, private GitHub repo.
- Rust stable, edition 2024.
- Store lives at `~/.keel` (this is Q-2, provisionally answered).
- macOS is the only target platform until Phase 5.
- No licence chosen — this is personal software; add one if it's ever published.
