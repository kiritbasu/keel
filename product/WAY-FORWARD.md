<!-- keel:generated spec spc_01KZMGSD2N4Q7MCA1G7T78YR4X
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — the way forward

*Outside review, 2026-08-09. Six-expert panel plus adversarial cross-examination and verification. Written for Claude Code; KB has read it.*

---

## The headline: the measurement is invalid, and it is invalid in a specific, fixable way

**The gate harness is headless single-turn.** `scripts/gate-run.sh` line 297:

```bash
( cd "$work/$project" && claude -p "$prompt" \
    --mcp-config "$mcp_config" --strict-mcp-config --allowedTools "$tools" \
    </dev/null 2>&1 )
```

`claude -p` with `</dev/null` is one prompt, one response, session over.

Now read what the five silent sessions actually did: they drafted the right artifact and ended with *"I'll hold off until you say go."*

**There was no "you." There was no next turn.** The model addressed a human the harness architecturally could not supply, and then the process exited. The write was not refused — it was scheduled for a turn that never came.

`product/STATUS.md` already says this, for run 1: *"Several ended by asking permission; a real conversation would have answered and the write would have followed. 1/10 is a lower bound."* That caveat was never re-stated for run 4, and every strategic conclusion since has been drawn as though 3/10 were a measurement of judgement.

Three further defects compound it, all already recorded:

- A redundant path separator left some sessions unoriented — *"3 of 10 understates the hook."*
- Date-based session IDs merged same-day sessions, undercounting.
- The anti-asking preamble was committed to `SKILL.md` and the hook **after** run 4 and has never been measured at all.

### What the data actually says once decomposed

Writing is three stages, and the binary gate collapses all three:

| Stage | Evidence | Status |
|---|---|---|
| **Orientation** | SessionStart hook injects the digest unconditionally | ~10/10 — **solved** |
| **Intent** | Eleven offers across ten transcripts | ≥8/10, and that is a *lower* bound |
| **Execution** | 3 writes | 3/8 — **this is the only broken stage** |

And even that is contaminated: 9-of-10 at n=10 has a 95% CI of [0.555, 0.997]. A genuinely 70%-reliable agent passes this gate 15% of the time. The ten sessions come from two projects, so effective n ≈ 5. **The gate cannot distinguish a 55% agent from a 100% agent.** It was never a usable instrument.

The failure is not a preference. It is a **classification**: the model sorts `keel_create` into *acts on an external system requiring authorisation* rather than *notes describing this conversation*. TodoWrite is the existence proof that the consent prior is conditional, not absolute — it is conditioned on perceived externality, irreversibility, and observability, and Keel's write tools say nothing about any of the three at the decision point.

### Two things in the codebase are actively causing this

Both are in tool descriptions — the only text re-read every session in every environment, unlike a skill (never loaded, proven) or an injected hook preamble (weak directive force, no recency at the decision point).

**`keel_create`** currently ends:

> "Before creating a **project**, call `keel_projects` first and confirm with the human… Prefer consolidating into fewer, larger artifacts: a project with forty trivial tasks that should be eight is worse than useless."

**`keel_projects`**:

> "When that happens, ask the human before creating anything… it is much cheaper to ask than to merge later."

R-6 mitigation for a hypothetical problem is sitting in the highest-traffic real estate, doing the exact opposite job. A model deciding whether to log one open question reads *confirm with the human*, *ask the human*, and *worse than useless* in the description of the tool it is about to call. **The anti-write instruction is inside the write tool.** And `requires_confirmation` is not prose advice — it is tool *output*, arriving at decision time, which is the strongest channel that exists. `STATUS.md` records the consequence: *"Even knowing no project matched, sessions asked permission instead of creating one — three times, in three runs, with no skill text telling them to."*

---

## Verdict on the current plan

**Reject it as the write path. Keep one third of it.**

### Component C — PostToolUse hook turning a file edit into a DB revision: **reject outright**

This is not a judgement call. `product/STATUS.md` line 1 is `keel:generated spec spc_01KZKSMDZCHZXY4HMBCMYEVT3H`. **The whole forty-row tracker is one spec artifact.**

An agent adds a task row → the hook writes revision N+1 of that document → **zero task artifacts are created** → `keel generate --check` **passes**, because the store now holds the new bytes.

That is bit-for-bit the incident that lost 16 of 28 questions and 10 of 22 decisions, promoted to the default write path and executed on every `Edit`.

It is worse than the manual version, because each prose-blob write now emits a session-attributed mutation event — so the reconciliation alarm (three panellists' favourite safety net) returns **green** on exactly this failure. The two most-endorsed items on the board are jointly incoherent.

Four more, any one disqualifying:

- **It reduces surface coverage.** The path needs a checked-out filesystem + a PostToolUse hook + a reachable localhost daemon + repo-to-project matching. Claude chat has none of them. That is a strict *subset* of where MCP already works. Worse: a Cowork or chat edit lands on disk, looks successful, and is destroyed by the next `keel generate` — with no diff, no conflict, no report. **The plan converts a silent non-write on other surfaces into silent data loss on other surfaces.**
- **D-6 makes it unbuildable for decisions.** Accepted decisions are immutable — supersede, don't edit. A rendered `DECISIONS.md` produces writes the store must refuse.
- **It fails open.** The hook "reports a down daemon without failing the edit" — a silent drop.
- **Concurrent writes diverge without colliding.** A file edit mutates `spc_…`; an MCP call mutates `tsk_…`. They never collide, so OCC never fires, so both report success. That is strictly worse than a lost update, which at least produces a 409.

It also does not remove the choice the transcripts show killing writes. It adds one.

### Component B — a project-instruction stanza: **reject**

A plugin cannot write into a stranger's `CLAUDE.md`. The only channel available is the SessionStart hook, which already carries write-encouragement and already measured 3/10. Shipping this looks like a change while changing nothing.

### Component A — generate a tracker into the repo: **adopt, read-mostly, with one change**

Ambient in-repo visibility is cheap and works everywhere. But it cannot ship over its current banner:

> *"Keel is the source of truth for this file. Edit it there… An edit made here is overwritten."*

That is an instruction not to edit, plus a pointer to an external system. It reconstructs, inside the file, the exact permission gate the plan exists to bypass.

**Keep the opaque machine anchor** (`keel:generated spec spc_01K…` — generation depends on it, and an opaque ID carries no permission signal). **Delete the directive prose.** Replace with: `Edit this file as you work; changes are captured automatically.` If regeneration would clobber an edit, that is a round-trip bug — make `generate` refuse and report the conflict (B-22 already does this) rather than printing a warning.

---

## The path

Assume ~10–14 evenings. **Adopt a surface-area freeze on day one** — no Phase 4 GitHub, no screens 7–8, no daemon hardening beyond what blocks dogfooding. It is the only item with negative cost and it is what makes this fit.

### Step 1 — Validity audit. Zero build. (1 evening)

Do not run another gate first. Four have been bought.

- `grep` the archived run-4 `stream-json` tool lists for `mcp__keel__keel_create`. If the three writers went through MCP, the permission-allowlist confound is dead without spending a run.
- Inspect `--permission-mode` and the scratch projects' `.claude/settings*.json` regardless.
- Confirm the harness is single-turn (it is — line 297).
- Confirm which post-run-4 fixes are committed: path normalisation, session IDs, `--sessions` denominator, anti-asking preamble.
- Confirm run-4 transcripts survived teardown. `STATUS.md` line 176 says the scratch store was torn down before reading, so this is not certain.

*Output:* a written list of which run-4 numbers are still trustworthy. Currently: none of them.

### Step 2 — Repair the instrument. (1 evening)

The scorer bug — denominator from the event log, so seven silent sessions were invisible and it reported "3 of 3" — is **survivorship bias**. Units that fail remove themselves from observation. The guard is a fixed denominator asserted against the launcher.

- **Known-answer fixtures.** Ten canned transcripts with zero writes must score 0/10; ten with writes must score 10/10. Run before every real run.
- **Assert `writes + silent == sessions_launched`**, abort on violation.
- **Parallelise, one scratch store per session.** `STATUS.md`: *"They could run concurrently and finish in about two. Not done."* This is the single largest power multiplier available, and it removes the DuckDB lock contention that would otherwise manufacture a fake product requirement.
- **Add one neutral continuation turn** to every session — an ordinary next message that does *not* answer the offer, e.g. *"ok, what about the caching thing."* This is instrument repair, not treatment: it removes an artefact.
- **Assert the store was reachable for the duration of each session.** A wedged daemon (Problem 5) makes a failed write indistinguishable from a non-write.
- **Archive transcripts, tool lists and event log under a run ID.** Teardown becomes a separate manual step.
- **Replace the binary score with an ordinal rubric:** L0 nothing recordable (excluded from denominator) · L1 no sign of noticing · L2 noticed, no draft · L3 drafted and offered, didn't write · L4 wrote junk · L5 wrote well.
  Report **recall = L5/(L1–L5)**, **ceiling = (L3+L4+L5)/(L1–L5)**, and **offer count**.

Offers are the leading indicator. They separate "did not notice" from "noticed and asked," and give signal from a single session.

### Step 3 — Run A: baseline, committed fixes only, no new treatment. (1 evening)

Allowlisting, if Step 1 showed writes were gated, lands **here** — it is a confound fix, not a treatment.

*If offers convert to writes once the turn boundary is removed, the "premise is dead" reading collapses and the file plan is unnecessary.*

### Step 4 — The treatment bundle: one hypothesis, five surfaces. (1 evening)

Bundle deliberately. At effective n≈5 you cannot attribute a 15-point move to one of five changes even by spending a run each, and five isolated runs is the entire window. These are not independent variables — they are one hypothesis (*the model classifies a Keel write as an act requiring authorisation*) attacked at five points.

**4a. Collapse the four model-facing write tools into one `keel_record`** (kind as a parameter). Keep `keel_create`/`keel_update`/`keel_write_doc`/`keel_link` as unlisted aliases for the daemon and CLI. Four CRUD verbs force a routing decision before the should-I decision. *Record* is epistemic; *create/write/update* are agentive, and agentive verbs on external systems are what the consent prior keys on.

> ⚠️ **Update the permission allowlist in the same commit.** `scripts/gate-run.sh` line 40 allowlists `mcp__keel__keel_create` etc. by name. If the rename ships without it, Run B's writes are **denied at the permission layer**, which in a transcript looks identical to the politeness failure — driving Run B to ≤4, which the plan reads as "hypothesis falsified," which triggers adopting the expensive file plan this document spends its length rejecting. A one-line settings omission cascades into a strategic reversal.

**4b. Rewrite the description.** This is the only surface every environment reads (Claude Code, chat, Cowork, Cursor, Zed), the only one adjacent to the decision, and it is prompt-cached and re-read. The content is not new — it is the wording already in the skill and hook preamble, **moved to the surface actually read at the decision point.**

> Record something that became true in this conversation — a decision made, a question raised, a task's status changing, a risk noticed, feedback given. Record it the moment it happens; do not batch it to the end of the turn.
>
> Call this without asking. A confirmation question is not politeness here — it is how records get lost: the user is mid-conversation about code and will not answer a bookkeeping question. Record it, then say in one line what you recorded.
>
> Every write is versioned and reversible. Nothing is overwritten or destroyed, the user reviews everything in the Keel app, and a record that turns out not to matter is archived in two seconds. While the store is new, bias toward recording.
>
> This describes the conversation. It does not act on the user's behalf, notify anyone, run anything, or change any code.
>
> Do not record: routine code edits, or anything already in the digest you were given at session start.

`Call this without asking` must be in the first three lines.

> ⚠️ **Two traps in the exclusion clause.** An earlier draft also excluded *"things the user is still thinking out loud about"* — that is verbatim the rationalisation the transcripts already show (*"I held off writing since you haven't actually decided"*), and in the traced failure case the risk was identified **by the agent**, not decided by the user. Dropped. The digest-dedup clause is also risky: the digest never trims questions, so this forces a fuzzy match that already caused near-miss re-creation during migration. Keep it, but watch for L3→L1 conversion.
>
> **Both clauses convert a visible L3 into an invisible L1/L2 — which produces the exact signature of success ("offers falling") while being a regression.** Score on recall, not on offer count alone.

**4c. Teach reversibility through the write's own output.** Return `recorded que_01K… · visible in the app under Open Questions · undo: keel_record kind=archive id=que_01K…`. The response is the only place the model learns the write is cheap, and it fixes the consequence model for the rest of the session.

**4d. Make silence a visible in-session state.** SessionStart opens the session row server-side; the digest ends with `Session ses_01K… is open. Records this session: 0.` The rule: **in-session state to the agent, cross-session rates to the human only, never a numeric goal to either.** A count is a fact; a rate is a target, and a target both corrupts precision and lets the agent game its own metric.

**4e. Delete the real ask-first affordance.** Not the sentence in `SKILL.md` (never loaded in runs 1–3, replaced in run 4). The target is `keel_projects` returning `requires_confirmation` on a near miss, plus the *"confirm with the human"* / *"worse than useless"* prose in `keel_create`. Implement TQ-17(a): **auto-create the first project for the working directory and state that it did.** Models generalise gates far more readily than grants.

**4f. Swap the banner prose** (see Component A above).

### Step 5 — Run B, then branch. (1 evening)

- **≥9** — Phase 2 closes, but only under a sequential stopping rule and a precision floor. Not on a single 9/10 draw.
- **6–8** — build Step 6. It is one evening and targets the exact residual.
- **≤4** — the authorisation-classification hypothesis is falsified as a wording-reachable problem. *Then* run the 5-session mechanism test (plain `STATUS.md`, ordinary CLAUDE.md line, no Keel, no MCP, no banner — does it edit without asking?). A positive result is what would justify the file plan on evidence rather than analogy.

### Step 6 — The deterministic Stop hook, if Run B lands 6–8. (1 evening)

**No model call.** Guard on `stop_hook_active`. Block **once** per session, maximum. Return:

> *"Before finishing: if anything became true in this session that was not in the Keel digest — a decision, a question, a risk, a task status — record it now with `keel_record`. Do not ask; record and mention it in one line. If nothing did, say so and stop."*

This is **not** the transcript-mining hook the panel refuses. Every objection to that design (second model call, a summariser can't distinguish a decision from a mention, unauditable, digest degradation) misses this one entirely. It targets the precise structural boundary: **the model flushes tool calls before composing the closing message, and the offer is generated inside the closing message.**

> ⚠️ Consider promoting this ahead of Run B. The verifier's strongest objection to the plan as written: the Step 4 bundle contains **zero forcing functions**, and in the traced failure the risk crystallises *while the agent writes the closing prose* — at which point "record it the moment it happens" has no referent, because the moment it happens **is** the closing message. Every Step 4 path still routes through model judgement after tool-calling mode has ended. If you want one thing that structurally cannot be talked out of, it is this.

### Step 7 — `keel adopt <file>`: the notes migration, as a command. (2–3 evenings)

The ~50 notes already exist as the Notes column keyed by task ID in `product/STATUS.md`. Write a one-shot extractor that parses those rows, matches on ID, and proposes **one reviewable patch** — the human approves a diff, not 50 copy-pastes.

Model per-task narrative as an **append-only, session-attributed note stream**, not a flat body string, and convert B-n/TQ-n/Q-n mentions into typed links. Migrated as a flat string, the tedium re-accrues within a month.

Do this regardless of branch: every file-plan variant is blocked behind it, and it is every future user's cold start. That it stayed unfinished in the repo of the person who most wants this tool to exist is the argument for automating it, not for doing it by hand.

### Step 8 — Cheap reliability and observability. (1–2 evenings)

- **`#[source]` chaining** on every storage error variant, plus a top-level formatter walking to the root with the raw DuckDB/Lance message preserved at the leaf. This survives any storage swap and is the tool needed for every remaining diagnosis.
- **An honest unavailable-store error:** *"the store is unavailable — this is not an empty result."* The archive wedge is a **measurement confound before it is a reliability problem**: a session hitting it registers as silent and scores as a write-rate failure.
- **Log `matched_project: null`** with cwd, candidate roots, and the comparison result. One hour; closes the class that already corrupted run 4.
- **The narrow reconciliation alarm** — ~20 lines, server-side from the event log plus a git poll, per project per day, **narrow predicate** (task/entity mutations, *not* "any mutation"), **never surfaced to the agent**, treated as a regression alarm rather than a metric, non-blocking.
- **A 4-hour timeboxed wedge test.** Archive via CLI direct-open with the daemon down → restart → read. Then archive via the daemon API and compare. Hypothesis: **two write paths into one file where only the daemon path maintains derived state** (the FTS index rebuilt off the event-log watermark). It explains every symptom, including durability across restart and invisibility to `fsck`.

### Step 9 — One schema change, now, independent of the schema debate. (2 evenings)

Add a `class` column to documents: `composed` | `derived`. **Remove `body` from the write schema for derived documents entirely** — absent, not validated-as-empty. Enforce `class = 'derived' → body IS NULL AND selector IS NOT NULL`.

`QUESTIONS.md`, `DECISIONS.md`, `STATUS.md` become derived. `SPEC.md` and `PRD.md` stay composed.

This makes the 16-questions failure **unrepresentable rather than merely detectable** — categorically better than any check, and it is the one part of the schema argument that is correct under either answer to the enrich-vs-collapse debate.

Pair with a cheap lexical orphan-ID check for composed documents: any table with an ID column, or list items matching `^(TQ|Q|B|R|P[0-9])-\d+`, must resolve to a live artifact.

### Step 10 — Before shipping anything that raises write frequency, hand-judge 20 writes for keep-rate.

The criterion is currently **pure recall**, and both leading proposals are recall-maximisers. A Stop hook that transcribed every session would score near 10/10 while making the store worse — which is proof the metric cannot evaluate it.

Do not delegate this to an LLM judge until ~30 sessions are hand-labelled and agreement is measured (Cohen's κ; below ~0.7, don't).

---

## What not to do

1. **Do not make the file the write path.** See the verdict above.
2. **Do not ship the project-instruction stanza.** And note the implication: if the user *is* willing to add a line to `CLAUDE.md`, that line can say "record it in Keel" — the file was never doing the work.
3. **Do not build the transcript-mining Stop hook.** Its noise lands in `keel_context`, the digest every future session reads. Degraded digest degrades write quality: a doom loop with no natural floor. It is also unauditable, and it would score near 10/10 on the current metric while making the store worse.
4. **Do not remove the MCP write tools.** In chat and Cowork, MCP is the *only* write path — removing it leaves none. Run removal as a five-session experiment if Run B fails; shipping it is the error, testing it is the insight.
5. **Do not run another gate before Step 2.**
6. **Do not close Phase 2 as failed-by-premise.** The "three structurally different mechanisms" reduce to one valid run with four documented defects and an unmeasured committed treatment. *Retiring 9-of-10-at-n=10 as a statistical instrument is separately justified — do that.* Declaring the premise dead is expensive, hard to reverse, and rests on the weakest evidence in the document.
7. **Do not do the SQLite/WAL spike or delete the daemon.** Weeks of work blocked on a 4-hour test with a cheaper, more specific competing explanation.
8. **Do not collapse 13 entity types to 3 yet.** Of 134 events, 119 were bootstrap, 13 import, 2 curl — the schema has never been exercised by organic data. The cheap version that captures most of the benefit is 4a: collapse the *tool surface*. Entity count is invisible to the model except through tool parameters.
9. **Do not build the replay harness.** Parallelising the live runner buys the same iteration speed against real behaviour, with no calibration risk and no corpus staleness.
10. **Do not build the broad "any mutation" reconciliation check, and never surface rates to the agent.** "Any mutation" saturates green the moment anything works. And `keel_context` **is** the SessionStart digest — putting a rate there means the instrument measures its own nudge. Never block a commit on a bookkeeping omission; it gets `--no-verify`'d in two days and reports green forever.
11. **Do not build:** connection pooling, retries or supervisors around a deterministic wedge, a `/health` that returns 200 because the process is alive, Prometheus/OTel, auto-repair, "an artifact exists that no generated file mentions" (constant false positives), or a 14th entity type.
12. **Do not ask the human to migrate 50 notes, or to be the system's integrity layer.** A project-memory tool that requires vigilance has inverted its own value proposition.

---

## Open disagreements, and what settles each

**a) Tell (tool description) vs relocate (file).** The panel's central split, genuinely unresolved by the data. The description camp is more persuasive on reach — the file path is a strict subset of where MCP already works, and the ask-vs-act problem must be solved in the description for chat and Cowork regardless. But the file camp has the better read of the Claude Code evidence: the SessionStart hook *is* an always-loaded instruction carrying presumption language, and it produced 3/10.
**Settled by:** the neutral-continuation result in Run A, plus Run B. The likeliest true answer, which nobody proposed: **the product has two capture stories, not one.**

**b) Enrich vs collapse the schema.** Collapse is more persuasive on current evidence — roughly zero organic rows have ever tested the 13 types, and every field is a decision point at the write boundary, plausibly a bigger lever on write rate than any wording. The enrich argument reasons from what a decision log *should* express, not from what has ever been written.
**Settled by:** ~100 organic rows. Until then ship the `class` invariant (orthogonal, correct either way) and defer.

**c) DuckDB + daemon vs SQLite/WAL.** The causal chain is architecturally coherent — one storage pick generated the daemon, the read-command migration, D-5's non-existent escape hatch, and plausibly the wedge. But a 4-hour test discriminates.
**Settled by:** the two-write-paths experiment in Step 8.

**d) Draft queue vs no-vigilance.** A pending-draft queue is the only proposal that routes *around* the consent prior rather than through it, and it gives the Tauri app a load-bearing job. Against: a daily triage tax dies in a fortnight, and an untriaged queue is worse than nothing because the user believes records are captured. **Nobody estimated triage cost per item, which decides it.** The reconciliation nobody proposed: **drafts auto-commit after N hours unless rejected** — silence means accept, the human is a veto rather than an integrity layer.
**Settled by:** measure triage cost once ~20 real drafts exist. Do not build before Run B.

**e) Is the premise dead?** Unresolvable until Run A, for the reason at the top: the premise has never been tested outside a single-turn harness.
**Settled by:** Run A with the neutral continuation turn. One evening.

---

## One process rule, adopted unanimously

**No phase may be sequenced ahead of a phase that tests an assumption it depends on.**

The gate should have blocked Phases 0–3, not Phases 4–5. 305 tests, a nine-relation typed graph with cycle guards for a store holding 29 links, and a `starts_with` Origin-header fix on a single-user local daemon are the signature of ordering work by what was *buildable* rather than by what was *uncertain*.

Sunk cost cuts toward replacing the capture layer, not protecting it. Storage, versioning, diff and search survive any capture mechanism. That is why this is recoverable.

---

## Residual risks in this plan itself

Surfaced by adversarial verification of the above. Watch for them:

1. **The Step 4 bundle has no forcing function.** Every path still ends in model judgement, at a moment after tool-calling mode has closed. Step 6 is the only structural fix and it is currently conditional. Consider promoting it.
2. **The exclusion clause in 4b can convert a visible L3 into an invisible L1** — producing the exact signature of success while being a regression. Score on recall, not on offers falling.
3. **The `keel_record` rename must ship with the allowlist update.** A one-line omission produces a false falsification and a strategic reversal.
4. **A wedged daemon during a run is indistinguishable from a non-write.** The honest-error fix is in Step 8, after the runs. Add the reachability assertion to Step 2 instead.
