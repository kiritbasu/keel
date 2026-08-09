<!-- keel:generated spec spc_01KZMAAKFG5QKXKRNAW2BQC13J
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — fundamental problems, for outside review

*Written 2026-08-09 for readers with no prior context. Everything here is measured unless it says otherwise.*

---

## What Keel is

A local-first store for everything that describes a software project except the code: specs, decisions, tasks, roadmap, questions, risks, customer feedback, glossary, environments. One user, a few thousand rows.

The premise: **an AI coding agent is the primary way things get in and out.** There is a desktop app, but it is a read surface. If the agent does not write, nothing does.

Architecture: DuckDB for entities plus Lance for prose and embeddings, a local daemon owning the single write path, an MCP server exposing nine tools, a Claude Code plugin (skill + hooks), a Tauri desktop app.

**What works.** Storage, versioned documents with diffs, a typed graph, hybrid search, backup/restore, the MCP surface, the desktop app, generating the repo's markdown from the store. 305 tests, four CI gates green. The problems below are not distributed across the system — they are concentrated in one layer.

---

## Problem 1 — The agent will not write to it

**This is the project's central risk and it is currently failing.**

The exit criterion for the plugin phase: across ten unprompted sessions of ordinary work, the agent writes to Keel in at least nine.

### Measurements

Four runs, ten sessions each, in throwaway projects that mention Keel nowhere. Prompts were ordinary developer talk — a bug, a refactor, a decision, a customer complaint.

| Run | Condition | Wrote |
|---|---|---|
| 1–3 | Skill installed | 0–1 of 10 *(invalid — see below)* |
| 4 | SessionStart hook injecting the digest | **3 of 10** |

Runs 1–3 were invalid: the skill was never loaded. Proven by inspecting the tool calls a session actually makes — thirty sessions, zero `Skill` invocations. A skill is model-invoked, and on ordinary engineering prompts the model never chose to load it. Its contents were advice nobody read.

Run 4 replaced that with a `SessionStart` hook that injects the project digest unconditionally. Orientation was fixed; writing was not.

### What the transcripts show

The seven silent sessions in run 4 were **not** unaware of Keel. Five of them worked out exactly what should be recorded, drafted it, and then stopped to ask:

> *"This looks like a real open risk and it isn't tracked yet — want me to log it as an open question (something like "How do we validate that each station's chart datum matches its authoritative source?")? I'll hold off until you say so."*

> *"Want me to log the open design question so it's not lost? I'll hold off until you say go."*

Eleven separate offers to write across ten transcripts. The first had identified a genuine safety issue and drafted the question. Only the write was missing.

### The observation that reframes it

The project owner pointed out: *ask Claude to maintain a markdown tracker file and it just does, reliably, every session, no guessing.* That is true, and it happened in this very repository — the agent maintained a hand-written `STATUS.md` diligently for a full day while touching **zero** task rows in Keel. Same agent, same session, same instructions.

Four differences look like they explain it:

1. **Visibility.** A tracker file is in the working directory. The store is invisible.
2. **Modality.** Editing a tracker is the same tool call already being used on the code. Writing to Keel is a different surface with a schema.
3. **Instruction placement.** "Update the tracker" lives in project instructions loaded every session. Keel's equivalent lived in a skill that was never loaded, then in one injected block competing with a wall of digest.
4. **Presumption.** Editing a repo file feels like part of the work. Writing to a shared external system feels like an act requiring permission — which is exactly what the transcripts show.

### Where outside input would change the answer

The current plan is to make Keel *look like the thing that already works*: generate a markdown tracker into the repo, ship a project-instruction stanza saying to keep it current, and use an existing `PostToolUse` hook that converts an edit to a generated file into a properly attributed database revision. File ergonomics, database truth.

**Is that right, or is it accommodating a limitation that should be attacked directly?** The alternative considered and set aside was a `Stop` hook that reads the session transcript, extracts what became true, and writes it — removing the agent's choice entirely, at the cost of an extra model call per session and a risk of recording noise. A third option nobody has argued for: accept a lower write rate and design around partial capture.

---

## Problem 2 — The criterion resists measurement

"Unprompted" is the entire claim, and a test that calls the tool has prompted it. That was documented from the start and is still true.

Four attempts to measure it produced: three invalid runs (skill never loaded), one valid run, and a string of instrument failures that each looked like product failures — a stale CLI login, an environment routing the agent through a third-party API endpoint, unrelated MCP servers adding a minute of startup per session, an interactive prompt hidden by output capture, and a scorer whose denominator came from the event log, so seven sessions that wrote nothing were invisible and it reported "3 of 3".

Each run costs 15–20 minutes because it is ten real sessions in sequence. Every wrong turn costs another run.

**The open question:** is there a valid cheap proxy for "would an agent do this unprompted", or is this inherently expensive to measure? If inherently expensive, what is the right sampling strategy — and how do you avoid tuning to a ten-sample signal?

---

## Problem 3 — Prose and structure are two representations of the same thing

Decisions and open questions exist twice: as prose documents that read well, and as typed rows that the app, search and graph can use.

This was found the bad way. Of 28 questions in the log, 16 existed only as rows inside one markdown blob — invisible to the board, unrankable by search, unlinkable. Same for 10 of 22 decisions. Everything written in a day's work had gone into a table inside a document and nowhere else, because a document is stored whole.

Decomposing them fixed the instance. Nothing prevents recurrence: add a numbered row tomorrow and it exists only as prose again. There is a check that compares generated *files* to the store; there is no check that the *rows inside a file* exist as artifacts.

**The open question:** is decomposition the right model at all? Prose is how a human writes a decision log; structure is what makes it queryable. Options are to keep both in sync (currently manual and demonstrably fails), to make prose primary and extract structure, or to make structure primary and render prose. The third is what the design intends and it is blocked on Problem 4.

---

## Problem 4 — The tracker cannot be generated yet

The design says the tracker becomes generated output once the store holds the project. The renderer exists and works. It is not switched on because the task rows carry no per-task notes, and those notes are most of what makes the tracker worth reading — rendering today would trade rich prose for a bare task list.

So the file is authoritative in the store but hand-shaped, and the task rows and the tracker can disagree. The work is migrating ~50 notes into task bodies; nothing structural. It is unfinished because it is tedious, which is its own kind of signal about what this tool asks of people.

---

## Problem 5 — The daemon is fragile in ways that surface as total failure

- **Archiving a project wedges the running daemon.** Every entity read then fails while the data is provably fine — with the daemon stopped, the CLI reads normally and the integrity check passes 27 checks. Seen twice; one restart did not clear it. The failure is total, not partial: an agent mid-session gets storage errors on everything with no indication of the cause.
- **DuckDB allows one writer, and blocks readers while it holds the lock.** A design note claimed non-daemon processes could "connect read-only"; that turns out not to exist. Every read-shaped CLI command had to move behind the daemon's API, and some still have not.
- **Errors lose their cause.** The failures above surface as "count matching rows" — the operation, not the reason. Diagnosing them meant reproducing in isolation.

**The open question:** how much robustness does a single-user local daemon actually warrant? The instinct is to harden it; the project's own scale discipline says do not build for problems you do not have. These are real failures, not hypothetical ones — but they were all found by one person in one evening of unusually hard use.

---

## Problem 6 — Nothing notices when it goes wrong

Every problem above was found by a human noticing something, not by the system reporting it.

- Task rows frozen for a day while commits accumulated: noticed by a human looking at the app.
- 16 questions existing only as prose: noticed by a human asking why one was missing from a board.
- Project matching broken by a redundant path separator: noticed because a session mentioned it in passing.
- The scorer's broken denominator: noticed by reading output that happened to be wrong in an obvious way.

There is a check that fails when a generated file drifts from the store. There is no equivalent for "a session produced commits and no task mutations", or "an artifact exists that no generated file mentions", or "a project's tracker has not changed while its repository has".

**The open question:** for a system whose whole job is to prevent a project's state from being lost, what should it be watching about itself — and what is the smallest set of self-checks that would have caught most of the above?

---

## What would help most

In rough order:

1. **Problem 1.** Is "make it look like a file the agent already maintains" the right resolution, or an accommodation? This decides the product.
2. **Problem 2.** Can "would it do this unprompted" be measured cheaply and validly, or is expense inherent?
3. **Problem 3.** Prose-primary, structure-primary, or genuinely both — and if both, what keeps them honest?
4. **Problem 6.** What self-checks are worth their weight at this scale?

Problems 4 and 5 are known work rather than open questions, and are listed for completeness.
