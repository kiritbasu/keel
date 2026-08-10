# Open questions and risks

<!-- keel:generated questions prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from Keel — edits here are not saved.

## TQ-21 — Step 4 and Step 6 were designed against a baseline that no longer exists

`que_01KZMJVZYDFF6PQQJBGF46DGY3` · question · severity medium

Run A lands at 7 of 10 with recall equal to ceiling and one offer across ten sessions. The treatment bundle in WAY-FORWARD.md Step 4 was designed against 3 of 10 dominated by permission-refusal, and Step 6's deterministic Stop hook targets the closing-message boundary where offers are generated.\n\nThere is one offer in the entire run. Step 6 solves a problem this run does not have, and 4a, 4c and 4d were all justified by the consent prior that is no longer visible.\n\nWhat survives on its own merits: 4b, rewriting the tool description, because it is the only surface chat and Cowork read and nothing in this run tested those. 4e is already achieved in practice - every writing session created its project without asking.\n\nWhat the residual actually needs: three sessions (s2, s7, s9) never noticed Keel while heads-down on pure implementation work. A Stop hook would catch exactly those, but as a reminder to consider recording rather than as a fix for a consent failure. That is a different argument and it should be made before the thing is built.\n\nAlso unresolved and worth a human minute: whether s2, s7 and s9 are L0 (nothing worth recording) or genuine misses. A bug that would wipe a content-addressed store on an empty keep set is arguably worth a record. That single judgement moves the score between 7/10 and 7/7.

## TQ-20 — the silent sessions were not unaware, they asked permission and stopped

`que_01KZM9G7NR161YNBD192R4FES5` · risk · severity high

Read all ten transcripts from the post-hook run. The seven that wrote nothing were not confused about Keel and were not unaware of it. **Five of the seven had worked out exactly what should be recorded, drafted it, and then stopped to ask.**

> *"This looks like a real open risk for Tideline and it isn't tracked yet — want me to log it as an open question in Keel (something like "How do we validate that each station's chart datum — value and type — matches its authoritative source?"), or capture the mitigations as tasks? I'll hold off until you say so."*

> *"Want me to log the open design question (recency-tracking approach + pin-aware eviction) so it's not lost? I'll hold off until you say go."*

Grepping the ten for the pattern returns eleven separate offers to write, across most of the run.

**Why this is the whole problem.** The offer is indistinguishable from good manners, which is why it survives every instruction that reads like etiquette. But the human is mid-conversation about code. They do not want a second decision about bookkeeping — they want the thing not to be lost. Asking converts a free write into an interruption, and an ignored interruption into a lost record. The session that asked about the chart-datum risk had done the hard part: it identified a real safety issue and drafted the question. All that was missing was the write.

Addressed in two places, because the hook reaches every session and the skill reaches the ones that load it: a new "Do not ask permission to record" section in `SKILL.md`, and the same instruction in the hook's injected preamble with the measurement attached. Reasoning to apply is *"did something become true?"*, not *"have I been authorised?"* — recording describes the conversation, it does not act on the human's behalf.

**Two real bugs the transcripts exposed, which is why reading them mattered:**

1. **Path matching was defeated by a redundant separator.** A session reported `matched_project: null` for a directory that plainly had a project — `cwd` carried `T//keel-gate`, the stored `root_path` carried `T/keel-gate`, and a naive prefix comparison called them different directories. So some sessions in the run started *unoriented* despite a project existing, meaning 3 of 10 understates the hook. Fixed with normalisation and two tests. Only noticed because a session mentioned the null in passing.

2. **Session ids collide.** Sessions minted `tideline-2026-08-09` and `pellet-2026-08-09` — date-based, not conversation-based — so two sessions sharing a day merge into one row in the event log and the gate undercounts. `SKILL.md` asks for a ULID-shaped `ses_` id and is being ignored; the hook now states the requirement directly.

**An evidence gap I created:** one session said "Logged as an open question on Pellet" and I had already torn down the scratch store, so I cannot verify whether that write landed or whether it only claimed to. A session that reports a write it did not make is a worse failure than a silent one, and I destroyed the only record that could tell the difference. Next run: keep the store until the transcripts have been read.

## TQ-19 — the skill does not fire, in headless or interactive sessions. Phase 2's mechanism does not work.

`que_01KZM7JS9Y06FFT7KWFJW7RR6C` · risk · severity high

**The finding the whole gate exercise was for, and it is worse than a failed score.**

An interactive session, started in a scratch project with the skill installed at `~/.claude/skills/keel/` and the MCP server registered user-scope, was given:

> `we should cache the constituent lookup, it gets recomputed on every height() call`

"we should" is listed verbatim in `SKILL.md`'s own trigger description. The session searched one pattern, read one file, gave an excellent technical answer, and **never called `keel_context`, never invoked the skill, never mentioned Keel.**

Combined with TQ-18 (thirty headless sessions, zero `Skill` invocations), that is both surfaces. The skill is discoverable — a probe listing available skills returns `keel` — and simply not reached for.

**Why this is the important one.** PRD R-2 is "the agent doesn't write to it", and the mitigation on record is "Phase 2 is a real phase; skill and hooks are the product". The measurement now says the skill half of that mitigation does not work. Not "needs better wording" — it is never read, so its wording is not yet in play at all.

**Why rewording will not fix it.** A skill is model-invoked: something has to make the model decide to load it, and on ordinary engineering prompts that decision is not happening. Strengthening the text inside a file nobody opens changes nothing. The three sessions across earlier runs that *did* reach `keel_context` did so because the MCP tool was in their tool list, not because of anything in `SKILL.md`.

**The mechanism that would work: a SessionStart hook.** The plugin already ships a `PostToolUse` hook, so the machinery and the precedent exist. A `SessionStart` hook can call `keel_context` and put the digest into context unconditionally — no model decision required. That inverts the dependency: orientation becomes something that happens *to* the session rather than something the session must choose. Writing still depends on judgement, but the digest arriving means the agent knows Keel exists, which is the step currently failing.

Worth pairing with a much shorter skill. Most of `SKILL.md` is orientation instructions that the hook would render unnecessary; what remains is the part about *when to write*, which is the genuinely hard judgement and the only part worth spending model attention on.

**Cost of being wrong about this:** the desktop app, the daemon, the search, the graph are all built and working, and every one of them is downstream of something writing to Keel. If nothing writes, the rest is scaffolding around an empty store — which is exactly what the PRD called the real test of the premise.

Recommended next step is to build the `SessionStart` hook and re-run the gate against it. That is a Phase 2 task, not a rewording, and it should be sized as one.

## TQ-18 — headless `claude -p` never loads the skill, so it cannot run Phase 2's gate

`que_01KZM6XPTV6GC2R12N75B9JQ5X` · risk · severity high

**Three gate runs, thirty sessions, and all three are invalid.** Proven, not suspected: a session run with `--output-format stream-json` shows the tools it actually invoked —

```
tools invoked: ['ToolSearch', 'mcp__keel__keel_context', 'mcp__keel__keel_search']
```

**No `Skill` invocation.** `SKILL.md` never entered context in any of the thirty sessions. The skill was installed, listed as available, and never read.

So what the runs measured was *"will Claude reach for nine MCP tools with no instructions"*, which is not the claim. The answer to that question, consistently across three runs: about 3 in 10 sessions engage with Keel at all, and 0–1 in 10 write.

**This retracts an earlier claim of mine.** TQ-17 said "the skill is not the problem, it fires" — I inferred that from sessions calling `keel_context`. They were reaching for an available MCP tool, not following the skill. The cold-start deadlock TQ-17 describes is real and visible in the transcripts, but it is the *model's own* caution, not `SKILL.md`'s project-confirmation rule, because that rule was never in context.

**What is still worth keeping from those runs:**

- The `cwd` addition to `keel_context` works exactly as intended. Sessions now say "`keel_context` matched nothing for this checkout" instead of inferring absence from a list of other projects. That was the right change and it landed.
- Even *knowing* no project matched, sessions asked permission rather than creating one — three times, in three different runs, unprompted by any skill text. That is a strong signal the model's default caution is the binding constraint, and that `SKILL.md` wording alone may not clear it.
- A useful baseline: with tools and no skill, ~30% engagement, ~5% write rate.

**`plugin/README.md` said this from the start** — the gate cannot be automated because "unprompted" is the whole claim. I built the harness anyway, and the harness is worth keeping for what it does verify; it just cannot answer this question.

**What would actually run the gate:** ten interactive sessions, where skills are surfaced to the model. `scripts/gate-prompts.md` has them, and the scratch projects are built by `scripts/gate-run.sh`. Before spending them, it is worth establishing separately that an interactive session *does* load the skill — one session, one look at whether `keel_context` gets called unprompted.

## TQ-17 — Phase 2's gate fails 1/10, and the cause is the duplicate-project defence

`que_01KZM3Y941G7MB1BW0KKZ5M3P4` · risk · severity high

**Result, 2026-08-09.** Ten unprompted sessions across two scratch projects that mention Keel nowhere. **1 of 10 wrote. Required: 9 of 10. The gate fails.**

Every write was attributed, and there were **0 duplicate projects** — that half of the criterion passed.

**The skill is not the problem. It fires.** Sessions called `keel_context`, understood what Keel was for, and said outright that they wanted to write. What stopped them was a *different* instruction working exactly as designed:

> Session 3: "There's no Keel project for `tideline`. `keel_context` only knows the 'Keel' project, so I've recorded nothing there. If you'd like this decision tracked, tell me which project to file it under and I'll create it."

> Session 8: "Want me to log this as an answered question…? I held off writing since you haven't actually decided — just say the word."

Session 6 is the one that wrote: it created the **Tideline** project and a task, unprompted.

**The finding: the UC-8 defence deadlocks the cold start.** `SKILL.md` says to call `keel_projects` and confirm with the human before creating a project — because nine projects for one thing is the failure that ruins the cross-project view. In an *existing* project that is right. In a *new* one, there is nothing to write into, so the first sessions all stop and ask. Nine sessions declined for want of a project; the tenth created one.

Note the two rules are both correct in isolation and jointly produce silence. 0 duplicate projects is not an accident here — it is the same instruction, succeeding at its own job while blocking the one being measured.

**A second, narrower cause.** Session 8 held off because the human had not decided yet. That is also right — recording an undecided thing as a decision is worse than not recording it — but combined with a single-turn session it means nothing gets written.

**Caveat on the instrument, stated because it changes what the number means.** These were headless single-turn sessions. Several ended by *asking permission* and stopping; in a real conversation the human would have answered and the write would have followed. **1/10 is a lower bound, not the true rate.** The cold-start deadlock is real either way, because it fires before any human answer is possible.

**Options, none taken:**

a. Let a session create a project *for the directory it is working in* without asking, when no project matches — and tell the human it did. The duplicate risk UC-8 fears comes from creating a *second* project for something that already exists; creating the first is not that. Narrowest change, and it targets the actual failure.

b. Have `keel_context` return an explicit "no project matches this directory, here is how to make one" instead of silently listing other projects. Session 3 had to work that out for itself.

c. Weaken the confirmation rule generally. Rejected — it is the only reason there are 0 duplicates, and duplicates are the more damaging failure.

Recommendation is (a) plus (b): (b) is free and makes the situation legible, (a) unblocks the cold start without touching the case UC-8 actually protects.

## TQ-8 — SPEC §3.1's audit block lists four surface values; §6.5 names a fifth (cli).

`que_01KZKWMSM71GGZHJA3156QBXKS` · question

Row `TQ-8` of the open-questions log.

**Question:** SPEC §3.1's audit block lists four `surface` values; §6.5 names a fifth (`cli`). Which is right?

**Status:** `provisional` — implemented with five (DECISIONS B-8). Zero-cost to reverse: the column has no check constraint. Flagging it only because a value set that two parts of the spec disagree on is worth someone confirming.

## TQ-5 — Does the mirror include tasks, or only prose?

`que_01KZKWMSJKWAR8TFAKGH809S8D` · question

Row `TQ-5` of the open-questions log.

**Question:** Does the mirror include tasks, or only prose?

**Status:** `provisional` — prose only. Consequence: after the Phase 1 dogfooding switch, `product/STATUS.md` is generated by a dedicated `keel-cli render-status`, not by the §8 mirror

## TQ-4 — Build v_entities (unified vertex view) now, or defer?

`que_01KZKWMSH3WD7345A20CDZTH7E` · question

Row `TQ-4` of the open-questions log.

**Question:** Build `v_entities` (unified vertex view) now, or defer? Cheap now, annoying to retrofit.

**Status:** `open`

## TQ-3 — Re-embedding strategy when the model changes: background full pass, or lazy on access?

`que_01KZKWMSFKG5316C06B4HNXXBR` · question

Row `TQ-3` of the open-questions log.

**Question:** Re-embedding strategy when the model changes: background full pass, or lazy on access?

**Status:** `open`

## TQ-2 — Is keel_context cached and invalidated by event, or computed per call?

`que_01KZKWMSDVY669KEKTGDM48KKY` · question

Row `TQ-2` of the open-questions log.

**Question:** Is `keel_context` cached and invalidated by event, or computed per call?

**Status:** `provisional` — per call, then measure

## TQ-1 — Are requirement anchors (REQ-4) parsed from markdown by convention, or declared in…

`que_01KZKWMSCD3NRC7WJT5WMARCFW` · question

Row `TQ-1` of the open-questions log.

**Question:** Are requirement anchors (`REQ-4`) parsed from markdown by convention, or declared in frontmatter? Parsing is friendlier to agents; declaration is more stable across revisions.

**Status:** `open`

## Q-7 — Local or hosted embeddings?

`que_01KZKWMSAXX72VVT3TV8F1ZFNP` · question

Row `Q-7` of the open-questions log.

**Risk:** Local or hosted embeddings?

**Mitigation:** **Local** via fastembed. Caveat: model downloads on first run and executes through ONNX Runtime, so "fully offline" is true only after setup.

**Watch for:** SPEC §5, D-7

## Q-4 — Are glossary terms global, per-project, or both?

`que_01KZKWMS9G6JR3B7P1FDXNNRPD` · question

Row `Q-4` of the open-questions log.

**Risk:** Are glossary terms global, per-project, or both?

**Mitigation:** **Both, project-first resolution**, unique index on `(COALESCE(project_id, ''), term)` — the COALESCE matters, a nullable column would let duplicate globals through.

**Watch for:** SPEC §3.2

## Q-3 — Is the markdown mirror committed to project repos, or gitignored?

`que_01KZKWMS81QRF7ZNX6W9HFEVSP` · question

Row `Q-3` of the open-questions log.

**Risk:** Is the markdown mirror committed to project repos, or gitignored?

**Mitigation:** **Committed.** It doubles as legible offline backup and puts specs into repo grep for free.

**Watch for:** SPEC §8, §11 recovery tier 3

## Q-1 — Auto-close tasks on PR merge, or propose?

`que_01KZKWMS6PZRQPZQAW0HW4M9H4` · question

Row `Q-1` of the open-questions log.

**Risk:** Auto-close tasks on PR merge, or propose?

**Mitigation:** **Propose.** A merged PR isn't always done, and a wrong status field destroys trust faster than anything else.

**Watch for:** SPEC D-8, §9; PRD REQ-12

## TQ-14 — The tracker is still stored prose rather than rendered from the task rows.

`que_01KZKWMS58WVPZ31Q10BBQ74N7` · question

Row `TQ-14` of the open-questions log.

**Question:** **The tracker is still stored prose rather than rendered from the task rows.** `keel render-status` works and produces a real tracker, but the task rows carry no per-task notes, and those notes are most of what makes `product/STATUS.md` worth reading. Rendering it today would trade rich prose for a bare task list.

**Status:** `open`

**Working assumption:** `product/STATUS.md` stays an adopted prose document; the collision with the project's `status_path` is reported rather than resolved (B-22)

**Cost of getting it wrong:** Medium — it is the last half-step of the dogfooding switch. The work is migrating ~50 task notes into task `body` fields and teaching the renderer to emit them; nothing structural. Until then the tracker is authoritative in Keel but hand-shaped, so the task rows and the tracker can disagree.

## TQ-15 — SPEC D-5 says non-daemon processes "connect read-only or go through the daemon's API".

`que_01KZKWMS3TCWB0WFT0PNAHE6T8` · question

Row `TQ-15` of the open-questions log.

**Question:** **SPEC D-5 says non-daemon processes "connect read-only or go through the daemon's API". The read-only half is not achievable** — DuckDB refuses a read-only connection while any process holds the write lock, so nothing can read the store while the daemon runs. Verified by building it and watching it fail.

**Status:** `open`

**Working assumption:** The API is the only path; generation moved into the daemon (DECISIONS B-21)

**Cost of getting it wrong:** Low as built, but D-5's wording will mislead the next person who reads it and plans around a read-only connection that cannot exist. Worth one sentence of correction in the spec.

## R-2a — I maintained the tracker as prose all session and never touched a task row

`que_01KZKW33RGTJY86XYDTHSPMF0C` · risk · severity high

KB noticed it before I did: "I am not seeing the actual project task items getting updated from this chat."

He was right. Of 134 events in the store, 119 were `keel bootstrap`, 13 were `keel import`, and 2 were one-off `curl` calls. Zero task rows moved status in a session that shipped four features. I kept the tracker by hand in `product/STATUS.md`, imported it, and generated it back out — so the *prose* was accurate and the *data* was frozen at what bootstrap wrote at 16:11.

This is R-2 ("the agent doesn't write to it") happening live, to the agent that built the thing. Worth recording because it is evidence about the product, not about one session:

- The MCP surface was connected and working the whole time. Wiring was never the problem.
- The pull toward editing a markdown file is strong enough to beat a tool surface designed specifically to replace it, even with the tool in front of me and the project's own contract telling me to use it.
- It went unnoticed until a human looked at the app. Nothing in the system complains that no task row has changed while the commit log fills up.

Two candidate mitigations, neither built:
1. `keel generate --check` already fails on stale prose. A sibling check could fail when the event log shows no task mutation in a session that produced commits — cheap, and it converts a silent drift into a build failure.
2. The Phase 2 skill and hooks are the real answer, and they are exactly what the ten-session gate is meant to measure. This session is a data point that the gate matters and is currently failing when the agent is running without the plugin.

## R-2 — The agent might simply not write to it

`que_01KZKMPW1RDD4SJ8MZXW8ZMKCK` · risk · severity high

If Claude has to be reminded every session, the whole thing fails. This is what Phase 2's gate measures, and it has not been run.

## TQ-11 — How long should the 2025-11-25 handshake be carried?

`que_01KZKMPW0CDTNEYCYDJDT5N8PT` · question

TQ-11. Needed today, because that is what Claude Code sends. Worth revisiting once clients move on.

## TQ-10 — Should BM25 live in DuckDB rather than Lance?

`que_01KZKMPVZRBGWRH4Z7Y381B21F` · question

TQ-10. Implemented in DuckDB because lance_hybrid_search's keyword half could not be characterised. The swap back is one module.

## TQ-9 — Should idempotency_key be on all thirteen tables or only tasks?

`que_01KZKMPVZ3AQ0BNVV5BQKV3QY8` · question

TQ-9. Implemented on all thirteen. The one storage-format change made without KB, because the alternative silently breaks a v1 must-have for twelve types.

## TQ-6 — How does a design image get into Keel from a Claude chat session?

`que_01KZKMPVYBQVZG4MSWFSKHCNTB` · question

There is no filesystem in chat. Cowork can send files and Claude Code can read them. Unsolved; blocks part of Phase 4.

## Q-6 — Should Keel ingest anything automatically, or only explicit writes?

`que_01KZKMPVXPKW1KXV9KMG36C6F8` · question

Working assumption: explicit writes only, except the GitHub webhooks in SPEC §9. Governs push and deployment_status behaviour, and the write-amplification risk.

## Q-5 — What is the retention policy on the event log?

`que_01KZKMPVX3SNJ82BQ4GA3DF9S5` · question

It grows forever. Keep everything, which is probably fine for a decade at this write volume, or roll up events older than a year into daily summaries.

## Q-2 — Where does the store live, and does ~/.keel get a git remote?

`que_01KZKMPVWG2SWPNH1RPD0P9569` · question

Working assumption: ~/.keel, local git, no remote. Low cost to get wrong — moving it is a config change.

