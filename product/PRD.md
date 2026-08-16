<!-- specline:generated spec spc_01KZKSMDV8C1AHKZQ69MA06EVX
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# Keel — Product Requirements Document

> **Status:** Draft v1
> **Author:** KB (with Claude)
> **Date:** 2026-08-09
> **Working name:** *Keel* — the structural spine a ship is built around. Placeholder; rename freely.

---

## 1. Summary

Keel is a single, local-first store for everything that describes a software project *other than the code itself*: PRDs, specs, decisions, tasks, bugs, milestones, roadmap, design artifacts, environments, metrics, risks, and customer feedback.

It has two faces:

- **An MCP server** — the primary interface. Claude reads and writes to it constantly, from Claude chat, Cowork, and Claude Code.
- **A desktop app** — the secondary interface. A human read/search/browse surface for looking at state on a big screen.

Every artifact is versioned, attributed to the session that wrote it, and connected to other artifacts through a typed relationship graph. Multiple projects live in one store so they can be reviewed in aggregate.

---

## 2. Problem

Project context for a serious software project currently lives scattered across markdown files in repos, chat scrollback, design tools, issue trackers, and memory. This creates three distinct failures:

**For the human.** There is no single screen that answers "what is the state of this project?" or "what is the state of *all* my projects?" Reading a folder of markdown files is not a status view.

**For the agent.** Every new Claude session starts cold. It cannot know what was decided three sessions ago, what open questions exist, which spec is current, or what it already tried. Context is re-established by hand, expensively and incompletely, at the start of every session.

**For continuity between the two.** Work that happens in Claude chat (early product thinking, PRDs, feedback triage) is disconnected from work that happens in Claude Code (implementation), which is disconnected from design. There is no artifact that spans them.

Existing tools each solve a slice: Linear does tasks, Notion does docs, GitHub does code. None of them are agent-native — they are human-first tools with an API bolted on — and none of them make a coherent, queryable graph across the whole project.

---

## 3. Users and interfaces

| User | Interface | Access pattern |
|---|---|---|
| Claude (chat, Cowork, Code) | MCP | Read + write, constant, high frequency |
| KB | Desktop app | Read + search, browse, occasional manual entry |
| KB | Claude, conversationally | The main way writes happen |
| KB | Mobile (later) | Read-only status check |

Keel is **single-tenant, single-human**. There is no multi-user permission model, no assignees, no team collaboration. Concurrency matters not because of many humans but because of many simultaneous agent sessions.

---

## 4. Goals

- **G1.** One store, one source of truth, for all non-code project artifacts across all projects.
- **G2.** An agent can orient itself on a project in a single MCP call.
- **G3.** Everything is versioned with full provenance — what changed, when, and which session did it.
- **G4.** Hybrid semantic + keyword search across every artifact in every project.
- **G5.** A typed relationship graph connecting artifacts, so traceability (requirement → task → PR) is queryable.
- **G6.** A comfortable read/search UI that makes "look through md files" unnecessary.
- **G7.** New projects are trivially added, and existing projects roll up into an aggregate view.
- **G8.** Runs entirely locally with no cloud dependency; deployable later without redesign.

## 5. Non-goals

- **N1.** Not a replacement for GitHub Issues as a *public* issue tracker. Keel is private.
- **N2.** No team collaboration, permissions, assignment, or multi-user auth in v1.
- **N3.** No sprints, story points, velocity, burndown, or time tracking. Ever, ideally.
- **N4.** Not a code host, CI system, or design tool. It references those; it doesn't replace them.
- **N5.** Not a general-purpose wiki. Every artifact has a type and a place in the graph.
- **N6.** No two-way sync with Linear/Jira/Notion. Keel replaces them.

---

## 6. Artifact model

Thirteen artifact types, plus two connective structures (Link and Event) that are infrastructure rather than artifacts. Each has a stable ULID, timestamps, and provenance. Most belong to exactly one project; `term` may be global, and `metric observation` hangs off a metric (carrying a denormalised project reference for filtering).

### Structural

**Project** — the root container. Name, slug, status, description, repo URLs, links to environments. Everything else belongs to exactly one project (or is global, for terms).

**Milestone** — a planning or shipping unit. `kind: milestone | release`. Has a target date, status, and an optional version string when it's a release. Milestones are what the roadmap view is built from. Replaces "epic."

### Work

**Task** — `kind: task | bug | chore | spike`. Status, priority, description. Links to the milestone it serves, the requirement it implements, and the PR that closes it.

### Knowledge

**Spec** — a prose document. `kind: prd | spec | rfc | design-doc | note`. The header lives relationally; the body is a versioned document. Requirements inside a spec are addressable blocks (`REQ-4`), so tasks can link to them individually.

**Decision** — an ADR. Context, options considered, the decision, consequences. Immutable once accepted; superseded rather than edited.

**Question** — `kind: question | risk | assumption`. The register of unknowns. Has a status (open / answered / accepted / mitigated / moot) and resolves to a decision or spec revision. This is what stops open questions from evaporating between sessions.

**Term** — glossary. A domain word and what it means *in this project*. Deliberately cheap to add. Exists primarily to keep agents from drifting on vocabulary across sessions.

### Inputs

**Feedback** — `kind: interview | support | sales | idea | competitor | observation`. Raw, unstructured input from the world. Explicitly designed to be entered casually by the human mid-conversation with a customer, then mined later by Claude for patterns and turned into specs or tasks. Source, date, verbatim body, optional contact.

**Metric** — a named measure with a target. **Metric observation** — a timestamped value. Together they close the loop on PRD success criteria, which are otherwise fiction.

### Surfaces

**Design artifact** (`design_artifacts`, `entity_type: design`) — a mockup, wireframe, screenshot, or Figma node. Stores the actual image, not just a link, plus a caption and a state (`proposed | approved | built`). Lets the app show intended design next to a screenshot of what shipped.

**Environment** — a deployment target. Name, URL, deployed version, deployed commit, status. Small; answers "what is actually live."

**Artifact** — generic escape hatch for files and links that don't fit above.

### Connective tissue

**Link** — a typed directed edge between any two artifacts. Relations: `implements`, `blocks`, `depends_on`, `supersedes`, `derived_from`, `resolves`, `references`, `duplicates`, `informs`. Direction is normative and defined in the spec — see SPEC §3.3.

**Event** — append-only log of every mutation. Actor, session, entity, before/after, timestamp. Powers the activity feed, undo, and the agent's "what changed since I last looked."

---

## 7. Key use cases

**UC-1 — Agent orientation.** A fresh Claude session in any surface calls one tool and receives a compact digest: project summary, active milestone, open P0 tasks, recent decisions, unresolved questions, relevant glossary terms. It is now oriented without reading a single file.

**UC-2 — Conversational capture.** KB is talking through an idea in Claude chat. Claude writes a PRD into Keel as a versioned document, creates the milestone, decomposes it into tasks, and records the three open questions it couldn't resolve — all without KB touching a UI.

**UC-3 — Implementation handoff.** A Claude Code session in the repo asks Keel for the current spec and the tasks under the active milestone, implements one, and marks it done with a link to the PR. The task's timeline now shows the commit.

**UC-4 — Customer feedback triage.** KB is on a call and types three raw notes into Keel. A week later he asks Claude "what have customers said about onboarding?" — Claude runs a hybrid search across all feedback in all projects, clusters it, and proposes two tasks and a spec amendment.

**UC-5 — Design–code loop.** A design artifact is attached to a spec. After implementation, a screenshot is attached as the `built` state. The UI shows them side by side, and the diff is obvious.

**UC-6 — Aggregate review.** KB opens the desktop app on Sunday and sees all projects: what shipped this week, what's at risk, what's blocked, which questions have been open longest, which metrics moved.

**UC-7 — Traceability audit.** "Is spec X actually built?" resolves to a graph query: every requirement block in the current revision, the tasks that implement each, their status, and the PRs that closed them.

**UC-8 — Project disambiguation.** Claude is about to create a project. The plugin requires it to confirm with the human first — "I don't see an existing project for this; create *Foo*?" — preventing nine near-duplicate projects.

---

## 8. Requirements

### Must have (v1)

- **REQ-1** — All thirteen artifact types are creatable, readable, and updatable via MCP.
- **REQ-2** — Every prose document is versioned; every version records author and timestamp, and records session ID **when the caller supplies one**. Session attribution is cooperative rather than enforced — see SPEC §6.5 and D-10 — because a stateless transport has no session to bind to. Any two versions can be fetched and diffed via MCP (`keel_get` with a `version` argument), not only in the UI.
- **REQ-3** — A single `keel_context` call returns a project digest sized to fit comfortably in an agent's context window.
- **REQ-4** — Hybrid (semantic + keyword) search spans every artifact type that carries text — prose-bearing types via the documents index, and the remainder (task, milestone, term, environment, artifact, project) via a title/body index — across all projects, with filters by project, type, and date range. Metrics and metric observations are excluded by design: they're numeric, and reaching them is a filter, not a search.
- **REQ-5** — Typed links can be created between any two artifacts, and the graph is traversable in both directions to a configurable depth (default 6, hard cap 16).
- **REQ-6** — Every mutation writes an event; events are queryable as "everything that changed since timestamp T."
- **REQ-7** — Concurrent agent writes are safe: creates are idempotent by key, updates use optimistic concurrency and reject stale writes.
- **REQ-8** — Project creation via MCP is permitted, but the accompanying skill/plugin must confirm project identity with the human before creating.
- **REQ-9** — A read-only markdown mirror of prose documents is generated into each project's repo. The mirror is never a source of truth and is never reconciled against the database; the single permitted read is the event-triggered hook path in SPEC §8.1, where an observed edit is converted into an attributed revision and the file is immediately regenerated.
- **REQ-10** — The desktop app provides: project list, project dashboard, roadmap/timeline, task board, document reader with version history, search, and an activity feed.
- **REQ-11** — The whole store is backed up as a single restorable unit.

### Should have (v1.1)

- **REQ-12** — GitHub integration: link tasks to PRs, **propose** a status transition on merge for confirmation (never silently auto-close — see SPEC D-8), surface commits on the task timeline.
- **REQ-13** — Design artifacts store image bytes and render inline, with proposed/approved/built states side by side.
- **REQ-14** — Metric observations are chartable over time against target.
- **REQ-15** — Feedback clustering — group semantically similar feedback and suggest themes.

### Could have (later)

- **REQ-16** — Remote-deployable daemon for phone access.
- **REQ-17** — Mobile client (thin, read-only).
- **REQ-18** — Automatic staleness detection — flag specs whose linked code has changed since the spec's last revision.
- **REQ-19** — Cross-project dependency tracking.

---

## 9. Success metrics

| Metric | Target | Why |
|---|---|---|
| Agent orientation cost | 1 tool call, < 4k tokens | If orienting is expensive, agents skip it |
| Sessions where Claude writes to Keel unprompted | > 80% | Measures whether the skill actually fires |
| Projects tracked | ≥ 5 within a month | Proves the multi-project premise |
| Manual markdown files consulted per week | → 0 | The original complaint |
| Duplicate/junk projects created | 0 | Tests REQ-8 |
| Open questions with resolution | > 70% | Tests whether the register is used or just accumulates |
| Time to answer "what's the state of everything" | < 30 seconds | The Sunday-review use case |

---

## 10. Phasing

**Phase 0 — Spine.** Core crate, DuckDB + Lance storage, schema, event log, ULIDs. No interface. Exit: entities can be created and read from a test harness.

**Phase 1 — MCP daemon.** Axum server, full tool surface, hybrid search, context digest, concurrency safety. Exit: a Claude session can run UC-1 through UC-4 end to end.

**Phase 2 — Claude plugin.** Skill teaching when to write, project-confirmation behaviour, hooks for the markdown mirror. Exit: Claude writes to Keel without being told to.

**Phase 3 — Desktop app.** Tauri shell, daemon as sidecar, all v1 screens. Exit: the Sunday-review use case works.

**Phase 4 — Integrations.** GitHub App, design artifacts, metrics charts.

**Phase 5 — Remote.** Deployable daemon, auth, mobile client.

Phases 0–2 are the ones that matter. If Keel is useful after Phase 2 with no UI at all, the premise is validated. If it isn't, the UI won't save it.

---

## 11. Risks

**R-1 — Schema creep.** The most likely cause of death. Thirteen types is already at the edge. Mitigation: no new types without deleting one, for at least six months.

**R-2 — The agent doesn't write to it.** If Claude has to be reminded every session, this fails. Mitigation: Phase 2 is a real phase, not an afterthought; the skill and hooks are the product as much as the daemon is.

**R-3 — Retrieval quality.** If search returns mediocre results, agents will stop trusting it. Mitigation: hybrid (not pure vector) search from day one; evaluate on real queries before building the UI.

**R-4 — Single point of failure.** The daemon owns everything and GitHub is no longer an implicit backup. Mitigation: REQ-11, plus the markdown mirror as a legible secondary.

**R-5 — Young dependencies.** The Lance DuckDB extension is new and is load-bearing. Quack (multi-process DuckDB writes) is beta, but is deliberately *not* depended on — see SPEC D-5. DuckPGQ is a research project and is explicitly rejected for v1 (SPEC D-4). Mitigation: storage sits behind three traits — `GraphStore` (SPEC §4), `DocumentStore` and `EntityStore` — so implementations can be swapped. Lance is the one genuinely unhedged dependency — and note that the Parquet export in SPEC §11 covers DuckDB only, so it is *not* an escape hatch from Lance. Closing that gap means adding a Lance→Parquet document export to `keel-cli backup`, which should be treated as part of Phase 0 rather than deferred.

**R-6 — Write amplification.** An over-eager agent creating dozens of trivial tasks would make the store worse than useless. Mitigation: the skill emphasises consolidation; the UI makes junk visible; events make bulk-undo possible.

---

## 12. Open questions

- **Q-1** — Should tasks auto-close on PR merge, or only *propose* closure for confirmation? Auto is convenient, but a merged PR doesn't always mean done. *Provisionally resolved: propose* (SPEC D-8); REQ-12 and SPEC §9 assume this.
- **Q-2** — Where does the store live on disk, and does it get its own git repo for backup?
- **Q-3** — Does the markdown mirror get committed to the project repo, or gitignored and local-only? *Provisionally resolved: commit it.* SPEC §11's recovery tier 3 assumes this; if the answer flips, that tier disappears and backup rests entirely on `~/.keel`.
- **Q-4** — Should terms be global across projects, per-project, or both with per-project override? *Provisionally resolved: both, project-first resolution* (SPEC §3.2), enforced by a unique index on `(project_id, term)`.
- **Q-5** — What is the retention policy on the event log? It grows forever. Options: keep everything (probably fine for a decade at this write volume), or roll up events older than a year into daily summaries. This is the single canonical home for this question; the spec defers to it.
- **Q-6** — Should Keel ingest anything automatically (commits, deploys), or only ever what an agent or human explicitly writes?
- **Q-7** — Embedding model: local (fast, private, weaker) or hosted (better, requires network)? Affects the "runs entirely locally" goal. *Provisionally resolved local* (SPEC D-7), with the caveat that the local model is downloaded once on first run and executes through ONNX Runtime — so "no network" is true only after setup.
- **Q-8** — Under a stateless MCP transport there is no protocol-level session. Where does `session_id` come from — client-supplied, daemon-derived from a stable client identity, or synthesised per conversation by the skill? G3 and REQ-2 depend on the answer. See SPEC §6.5.

---

## 13. Glossary

| Term | Meaning in Keel |
|---|---|
| **Artifact** | Any stored entity. Used generically, not as the specific `artifact` type. |
| **Document** | The versioned prose body of a spec, decision, or feedback item. |
| **Revision** | One immutable version of a document. |
| **Digest** | The compact project summary returned by `keel_context`. |
| **Mirror** | Generated read-only markdown written into a project repo. |
| **Link** | A typed directed edge between two artifacts. |
| **Event** | One record in the append-only mutation log. |
| **Session** | One Claude conversation, used as the provenance unit. |
