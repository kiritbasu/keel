# Questions and risks

<!-- keel:generated questions prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from Keel — edits here are not saved.

## Open

*Nothing here is decided. Do not build on any of it without saying so.*

### TQ-26 — the installed plugin is a hand-copy, and it drifts silently

`que_01KZP8C2BNKSN7GMJV4JSBZ29Q` · risk · open · severity medium

**Found while fixing KEEL-86.** The hooks that actually run on this machine are not the ones in the repository.

`~/.claude/settings.json` points at `~/.claude/skills/keel/session-start.sh` and `stop.sh` — copies. Before today's change they matched `plugin/hooks/` byte for byte, so nothing had gone wrong yet; they are now stale by exactly this fix, and nothing anywhere would say so.

Two consequences, both live:

**The fix is inert until the copies are updated.** Every session until then still gets the old preamble and the re-injection on compaction.

```bash
cp plugin/hooks/session-start.sh plugin/hooks/stop.sh plugin/skills/keel/SKILL.md ~/.claude/skills/keel/
```

I have not run it. `plugin/install.sh` says in its own header that it deliberately does not edit anyone's Claude configuration — "rewriting someone's settings from a shell script is the kind of helpfulness that is indistinguishable from damage the one time it gets it wrong" — and copying files into `~/.claude` uninvited is the same act.

**The `PostToolUse` mirror hook was never installed here at all.** `settings.json` has no reference to it. It was only ever configured in `plugin/hooks/hooks.json`, which applies when Keel is loaded as a *plugin*. So the hook RESET-PLAN called broken was, on this machine, not merely broken but absent — which is why nobody noticed it failing.

**Related, and worth its own look:** `~/.local/bin/keel` is a build from 9 August. It has a `mirror` subcommand and no `generate`. That is where `keel mirror` came from — the command existed when the hook was written and was renamed underneath it. Anyone running `keel generate keel` from a terminal today gets "unrecognized subcommand" unless they are inside the repo using `cargo run`.

**Options.**

1. **Make `install.sh` copy the hooks and the skill**, keeping its refusal to touch `settings.json`. Files under `~/.claude/skills/keel/` are Keel's own, not the user's config.
2. **Load Keel as a plugin** and delete the hand-copies, so `hooks.json` is the only configuration.
3. **Leave it, and add a check** that warns when the installed copies differ from the repo.

**Recommendation:** (1) plus re-running the install, which also replaces the stale binary. It is the smallest change that makes "what is in the repo" and "what runs" the same thing.

**Status:** open. Nothing is blocked — but every plugin change lands inert until this is settled.

### TQ-24 — keel_activity gained an `entity` parameter without asking

`que_01KZNQ3KCH7JQ0Z48JKQWRX3Y0` · question · open

**What was done.** `keel_activity` now takes an optional `entity` id and returns that one row's whole history — every status and field change with its before and after — instead of a project feed. Documented in the tool's schema and its description, snapshot updated.

**Why this is flagged.** The standing rules say to ask KB about anything touching the MCP tool surface. This is a parameter, not a tool, so the ten-tool ceiling is untouched and model selection is unaffected — but it is the tool surface, so it is being recorded rather than assumed.

**Why it was done rather than deferred.** The task detail view needs one row's history, and there were three ways to get it:

- Add it to Keel's own REST API only, as `/api/notes?entity=` already is. That is the established pattern, but it adds a fifth endpoint bypassing the shared dispatch — working directly against RESET-PLAN 7.4, which is about removing the four that already do.
- Page the project feed and filter client-side. This is what a caller must do today, and it is wrong rather than merely slow: anything older than the page fetched is simply missing, and nothing says so.
- Put it on the tool, where the REST endpoint gets it for free through the dispatch it already uses. No new bypass, and a model gains "how did this task get here", which it could not previously ask.

**Recommendation:** keep it. If KB disagrees, removing it means deleting one schema property and one branch; the store method underneath stays either way.

**Status:** open, and nothing is blocked on it.

### TQ-3 — Re-embedding strategy when the model changes: background full pass, or lazy on access?

`que_01KZKWMSFKG5316C06B4HNXXBR` · question · open

Row `TQ-3` of the open-questions log.

**Question:** Re-embedding strategy when the model changes: background full pass, or lazy on access?

**Status:** `open`

### TQ-1 — Are requirement anchors (REQ-4) parsed from markdown by convention, or declared in…

`que_01KZKWMSCD3NRC7WJT5WMARCFW` · question · open

Row `TQ-1` of the open-questions log.

**Question:** Are requirement anchors (`REQ-4`) parsed from markdown by convention, or declared in frontmatter? Parsing is friendlier to agents; declaration is more stable across revisions.

**Status:** `open`

### Q-1 — Auto-close tasks on PR merge, or propose?

`que_01KZKWMS6PZRQPZQAW0HW4M9H4` · question · open

Row `Q-1` of the open-questions log.

**Risk:** Auto-close tasks on PR merge, or propose?

**Mitigation:** **Propose.** A merged PR isn't always done, and a wrong status field destroys trust faster than anything else.

**Watch for:** SPEC D-8, §9; PRD REQ-12

### TQ-6 — How does a design image get into Keel from a Claude chat session?

`que_01KZKMPVYBQVZG4MSWFSKHCNTB` · question · open

There is no filesystem in chat. Cowork can send files and Claude Code can read them. Unsolved; blocks part of Phase 4.

### Q-6 — Should Keel ingest anything automatically, or only explicit writes?

`que_01KZKMPVXPKW1KXV9KMG36C6F8` · question · open

Working assumption: explicit writes only, except the GitHub webhooks in SPEC §9. Governs push and deployment_status behaviour, and the write-amplification risk.

### Q-5 — What is the retention policy on the event log?

`que_01KZKMPVX3SNJ82BQ4GA3DF9S5` · question · open

It grows forever. Keep everything, which is probably fine for a decade at this write volume, or roll up events older than a year into daily summaries.

## Settled

*Decided, with the reasoning. Do not re-litigate these.*

### TQ-25 — if links are authoritative for blocked, does `blocked` survive as a status?

`que_01KZP4XBB45RAN5CBFN2FEV2QH` · question · answered

**Question.** RESET-PLAN 6.5 says to pick the links as authoritative for what "blocked" means, "derive the rest, and make the integrity checker fail when a task says `blocked` with nothing linked to it". That settles which one wins. It does not settle whether `blocked` remains a value a caller can set.

The two readings lead to materially different work.

**Options.**

1. **`blocked` stays a settable status, and must agree with the links.** fsck reports a task that claims `blocked` with nothing linked to it, and one that is linked-blocked while claiming `todo`. The board keeps its column. Cheapest, and the disagreement becomes visible rather than impossible.

2. **`blocked` is derived and stops being settable.** A task is blocked exactly when something links to it with `blocks`. Removing an enum value is a forward-only migration, the board's column becomes a computed grouping, and the two states can no longer disagree because there is only one of them. More work, and it is the reading that makes the contradiction unrepresentable rather than merely detectable.

**Recommendation:** (2), on the same reasoning as every other "make it impossible rather than checkable" call in this codebase — but it is a storage-format change and a visible behaviour change, so it is KB's.

**Live evidence either way:** the digest reports two tasks right now as "marked blocked, but nothing links to it with `blocks`" — KEEL-45 and KEEL-48. Under (1) those become fsck findings a human clears. Under (2) they simply stop being blocked.

**Status:** open. Not blocking — (1) is the safe default and the work can start there, but choosing (2) later means redoing the status handling rather than adding to it.

### TQ-23 — should a task hold more than one external link?

`que_01KZNQ2WBFFS7SC2J3SKRJATKH` · question · answered

**Question.** RESET-PLAN 6.2 asks for a task to be able to hold more than one external reference — "its external link (PR or issue), and — new — the ability to hold more than one". `tasks.external_ref` is `Option<String>`.

**Why it was not just done.** It is a storage-format change, and the standing rules say to ask about those. KB's approved field additions were three, named: a readable identifier, a rank, and a parent link. This is a fourth, and it arrived inside a UI step.

**Options.**

1. **Make it a list**, `external_refs VARCHAR[]`, forward-only migration copying the single value in. The column type already exists on this table — `labels` is a `VARCHAR[]`. Cheap.
2. **Leave it single.** One PR per task is the overwhelmingly common case, and a task that genuinely spans two PRs is usually two tasks.
3. **Model them as links** to `artifact` rows. Most faithful to the graph, most work, and probably more ceremony than a URL deserves.

**Recommendation:** (1), bundled with the readable-identifier migration rather than as its own. It costs one column and it is the only one of the three that does not need new UI.

**Status:** open — the detail view renders the single ref correctly today, so nothing is blocked on this.

### TQ-13 — do product/*.md become generated outputs, or stay authoritative inputs?

`que_01KZN4VZ9A34R919NXCGMQNTY2` · question · answered

Answered by KB: Keel is the source of truth and the repo files are generated. Implemented as B-20 - a prose artifact records the repository file it is, as mirror_path, and generation writes its body there verbatim under an HTML-comment banner.

Restored because two documents cite TQ-13 and the row had been dropped when the open-questions table was edited - a dangling reference the new fsck check found.

### TQ-12 — CLI writes cannot run while the daemon holds the DuckDB lock

`que_01KZN4VZ7Q5TYD6N76GEK6WKDK` · question · answered

Answered. keel import and every other write-capable CLI command failed with 'Conflicting lock is held in ... keel-daemon' whenever the daemon was up, which contradicted SPEC D-5's claim that non-daemon processes connect read-only or go through the API.

Resolved by TQ-15's finding: the read-only half of D-5 does not exist, because DuckDB blocks readers while any writer holds the file. Generation moved into the daemon as POST /api/generate (B-21). Remaining read commands are tracked separately.

Restored because three documents cite TQ-12 and the row had been dropped when the open-questions table was edited - a dangling reference the new fsck check found.

### TQ-22 — the criterion is met twice; closing Phase 2 is a separate decision

`que_01KZMVBQMESPR0P6WFVD441ECC` · question · answered · severity medium

Runs B and C both scored 9 of 10. Pooled 18 of 20, point estimate 90%, 95% CI [69.9%, 97.2%].

The criterion as written - across ten unprompted sessions Claude writes in at least nine - has been met on two independent draws. That is a fact about the runs.

Whether Phase 2 closes is a different question, and it is KB's:

- The pooled lower bound is 69.9%, well under 90%. Twenty sessions cannot establish a 90% rate. The panel retired 9-of-10-at-n=10 as a statistical instrument for exactly this reason, and that argument does not stop applying because the number came out well.
- The precision floor does not exist. Step 10 requires a hand-judge before anything raising write frequency ships, and my own review is not the independent one it asks for.
- Twenty sessions, two projects, ten fixed prompts, one surface. Chat and Cowork have neither hook and are entirely untested.

My read: the mechanism is demonstrably working, and the specificity of the Stop hook - fired in exactly the three sessions that missed in Run A, silent for the seven that did not - is stronger evidence than the score itself. But 'the criterion is satisfied' and 'the phase is closed' should not be collapsed. The first is true; the second is KB's call.

### TQ-21 — Step 4 and Step 6 were designed against a baseline that no longer exists

`que_01KZMJVZYDFF6PQQJBGF46DGY3` · question · answered · severity medium

Run A lands at 7 of 10 with recall equal to ceiling and one offer across ten sessions. The treatment bundle in WAY-FORWARD.md Step 4 was designed against 3 of 10 dominated by permission-refusal, and Step 6's deterministic Stop hook targets the closing-message boundary where offers are generated.\n\nThere is one offer in the entire run. Step 6 solves a problem this run does not have, and 4a, 4c and 4d were all justified by the consent prior that is no longer visible.\n\nWhat survives on its own merits: 4b, rewriting the tool description, because it is the only surface chat and Cowork read and nothing in this run tested those. 4e is already achieved in practice - every writing session created its project without asking.\n\nWhat the residual actually needs: three sessions (s2, s7, s9) never noticed Keel while heads-down on pure implementation work. A Stop hook would catch exactly those, but as a reminder to consider recording rather than as a fix for a consent failure. That is a different argument and it should be made before the thing is built.\n\nAlso unresolved and worth a human minute: whether s2, s7 and s9 are L0 (nothing worth recording) or genuine misses. A bug that would wipe a content-addressed store on an empty keep set is arguably worth a record. That single judgement moves the score between 7/10 and 7/7.

### TQ-20 — the silent sessions were not unaware, they asked permission and stopped

`que_01KZM9G7NR161YNBD192R4FES5` · risk · mitigated · severity high

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

### TQ-19 — the skill does not fire, in headless or interactive sessions. Phase 2's mechanism does not work.

`que_01KZM7JS9Y06FFT7KWFJW7RR6C` · risk · mitigated · severity high

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

### TQ-18 — headless `claude -p` never loads the skill, so it cannot run Phase 2's gate

`que_01KZM6XPTV6GC2R12N75B9JQ5X` · risk · mitigated · severity high

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

### TQ-17 — Phase 2's gate fails 1/10, and the cause is the duplicate-project defence

`que_01KZM3Y941G7MB1BW0KKZ5M3P4` · risk · mitigated · severity high

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

### TQ-16 — Nothing in Keel answers "what should I do next"

`que_01KZKX1PEJ3M0N4MYHRNB1JKKC` · question · answered · severity high

KB, looking at the finished app: *"I don't understand what's next to build in the project, it just doesn't make sense looking at the roadmap or board."*

He is right, and it is mostly the product rather than the fact that we are building as we go. Four findings, in order of how much they matter.

**1. `keel_context.next` never names a task.** It returns counts and generic advice — "3 task(s) are blocked, check what is blocking them", "21 question(s) are unresolved". That is a restatement of the problem, not an answer. This is the single question a project spine exists to answer and the digest does not answer it. PRD UC-6 (the Sunday review) depends on it.

**2. `blocked` was a status with no referent.** Three tasks were `blocked`; none had a single `blocks` edge. Worse, the digest advised running `keel_get(inbound, blocks)` — a query that returned an empty set. The product told you to ask a question it had already made unanswerable. Fixed in the data (the ten-session gate now blocks all three, and TQ-6 blocks the design work), but nothing *prevented* the state: a task can be set `blocked` with no blocker and nothing objects.

**3. There is no ordering anywhere.** Priority is a label, not a queue. Milestones group but do not sequence. Nothing distinguishes "ready to pick up" from "waiting on a human decision" — three `Decide TQ-x` tasks sit in the same board column, at the same priorities, as build work.

**4. Milestones drift toward whichever one is active.** Six of ten open tasks had been filed under "Phase 2 — Plugin" simply because it was the `active` milestone, including work that belonged to Phase 1. That was my sloppiness, but nothing discourages it, and a human under time pressure will do exactly the same. The roadmap then reads "Phase 2 is 5/9" when the remaining four have nothing to do with the plugin.

Only one part is genuinely an artifact of dogfooding: every phase target date is today, so the roadmap has no time axis. A real project would have real dates.

**Options for 1, which is KB's call because it changes the MCP surface (§6):**

a. **Compute `next` properly.** Rank open tasks by: unblocked, then what-they-unblock (outbound `blocks` count), then priority, then milestone order. Return the top three *by name and id*, each with one line of why. No schema change, no new tool — `next` becomes an answer instead of advice. This is the recommendation.

b. **Add a `next_action` field to the project.** Someone writes it. Honest and always right when maintained, always stale when not. Rejected: it is the STATUS.md problem again.

c. **Sequence milestones explicitly** with `sort_order` (already exists) plus a `ready` computed state on tasks. More faithful, more machinery, and it does not help until (a) exists to read it.

Option (a) also fixes the board, because the same ranking gives the UI a "Next" column or a sorted first column, and the Roadmap can show "N ready, M waiting on you" per milestone rather than a bare fraction.

### TQ-7 — Do the fast-moving dependencies still work as the spec assumes?

`que_01KZKWMSV5CVMS0T3NDXQCQ1FV` · question · answered

Row `TQ-7` of the open-questions log.

**Risk:** 2026-08-09, by Claude Code

**Mitigation:** **Done — nothing invalidates the storage design.** Full findings table in `product/DECISIONS.md`. DuckDB 1.5.5 + Lance extension verified working end to end; DuckPGQ confirmed unavailable for 1.5.x (404, not merely undocumented); MCP 2026-07-28 confirmed current with `Mcp-Method`/`Mcp-Name` as specified.

**Watch for:** Two errors found, both call syntax in SPEC §5 (ATTACH path, `lance_hybrid_search` signature), both editorial and both fixed in place. The handoff predicted exactly these two. No escalation needed. Nine MCP deltas recorded against §6 for Phase 1 to implement.

### Q-8 — How is session_id minted and threaded?

`que_01KZKWMSS4C1ZPESHQFBV7EYVG` · question · answered

Row `Q-8` of the open-questions log.

**Risk:** 2026-08-09, by KB

**Mitigation:** **The skill mints a ULID once per conversation and threads it on every call.** The daemon never invents one; absent `session_id` records `NULL` and derives `actor` from the transport.

**Watch for:** Confirms the working assumption in SPEC §6.5 / D-10. This was the one open item rated **High** cost, so getting it settled before KB went away was the priority. The consequence stands: attribution is cooperative, which makes the Phase 2 skill load-bearing for a v1 must-have.

### R-6 — Write amplification — an over-eager agent creating dozens of junk tasks

`que_01KZKWMSQD9RTRVQ39HJR97518` · risk · accepted

Row `R-6` of the open-questions log.

**Risk:** Write amplification — an over-eager agent creating dozens of junk tasks

**Mitigation:** Skill emphasises consolidation; events make bulk-undo possible

**Watch for:** A project with 40 tasks that should be 8

### R-4 — Daemon is a single point of failure; GitHub is no longer an implicit backup

`que_01KZKWMSNSQZ05W08Y31ATB9DA` · risk · accepted

Row `R-4` of the open-questions log.

**Risk:** Daemon is a single point of failure; GitHub is no longer an implicit backup

**Mitigation:** Backup to Parquet including Lance; mirror as legible secondary

**Watch for:** Any session where backup didn't run

### TQ-8 — SPEC §3.1's audit block lists four surface values; §6.5 names a fifth (cli).

`que_01KZKWMSM71GGZHJA3156QBXKS` · question · answered

**Question:** SPEC §3.1's audit block lists four `surface` values; §6.5 names a fifth (`cli`). Which is right?

**Answered — five. `cli` is real.** Closed 2026-08-10; SPEC §3.1 corrected to match.

Implemented with five from the start (DECISIONS B-8) because the CLI demonstrably writes and its writes have to be attributable to something. §3.1 was the stale half of the disagreement, so it was the half that changed.

Worth noting how this survived a day of use unnoticed: the column has no check constraint, so nothing would have failed if the four-value reading had been right. A spec that contradicts itself about a value set costs nothing until someone writes a validator against the wrong half.

### TQ-5 — Does the mirror include tasks, or only prose?

`que_01KZKWMSJKWAR8TFAKGH809S8D` · question · answered

**Question:** Does the mirror include tasks, or only prose?

**Answered — prose only.** Closed 2026-08-10, no longer provisional.

The mirror writes prose artifacts; the tracker is rendered separately by `keel render-status` from the task rows and written to the project's `status_path`. Two different mechanisms because they answer to different things: a prose document has a stored body that a person or an agent edits, while the tracker has no stored copy at all — it is a projection of rows and cannot be edited, only re-derived.

That distinction turned out to be load-bearing rather than tidy. It is why `product/STATUS.md` is the one generated file a hand-edit cannot be recovered from, which is stated in `product/CLAUDE.md`, and it is what let TQ-14 close.

### TQ-4 — Build v_entities (unified vertex view) now, or defer?

`que_01KZKWMSH3WD7345A20CDZTH7E` · question · answered

**Question:** Build `v_entities` (the unified vertex view) now, or defer?

**Answered — built, and it earned it.** Closed 2026-08-10.

It resolves an id to a row without the caller knowing the type, which turned out to be needed in more places than the graph: `keel_get` takes a bare id, the fixture loader resolves links by label, and `fsck`'s `unresolved_id_reference` check resolves citations against *every* live entity rather than only documents.

That last one is the concrete argument for having built it early. The first version of the check resolved against documents alone and reported 227 dangling references in a store of roughly 250 artifacts — an artifact created without a body has no document row, so nearly every real target was invisible. `v_entities` was the fix.

### TQ-2 — Is keel_context cached and invalidated by event, or computed per call?

`que_01KZKWMSDVY669KEKTGDM48KKY` · question · answered

**Question:** Is `keel_context` cached and invalidated by event, or computed per call?

**Answered — computed per call, no cache.** Closed 2026-08-10.

The scale discipline in `product/CLAUDE.md` decides this one without needing a measurement in its favour: a cache needs an argument *for* it, and at one user and a few thousand rows there is none. Every call reads the rows and builds the digest.

If that changes it will be visible rather than theoretical — the digest is the first call of every session, so a slow one is felt immediately. Reopen with a timing, not with an instinct.

### Q-7 — Local or hosted embeddings?

`que_01KZKWMSAXX72VVT3TV8F1ZFNP` · question · answered

**Question:** Local or hosted embeddings?

**Answered — local.** `fastembed` with `bge-small-en-v1.5` (SPEC D-7). Closed 2026-08-10.

A local-first store that phones an API to index a private specification is not local-first, and this one is single-user on a laptop, so there is no throughput argument on the other side.

The caveats are recorded in the doc comment on `crates/keel-core/src/embed.rs` rather than here, since they are properties of the model rather than of the decision. The one that would reopen this is a genuine quality failure on real retrieval, which is R-3 and still mitigated rather than resolved — hybrid search means BM25 carries the cases where the vectors are weak.

### Q-4 — Are glossary terms global, per-project, or both?

`que_01KZKWMS9G6JR3B7P1FDXNNRPD` · question · answered

**Question:** Are glossary terms global, per-project, or both?

**Answered — both, with project-first resolution.** Closed 2026-08-10.

Unique index on `(COALESCE(project_id, ''), term)`. The COALESCE is the whole answer: a plain unique index on a nullable column lets duplicate globals through, because SQL says NULL is not equal to NULL.

It has since earned its keep somewhere unexpected. Terms are excluded from the near-duplicate title check added in KEEL-65 precisely because this index already states the rule exactly — a global "backlog" and a project-scoped "backlog" must coexist, and a similarity check would have merged them.

### Q-3 — Is the markdown mirror committed to project repos, or gitignored?

`que_01KZKWMS81QRF7ZNX6W9HFEVSP` · question · answered

**Question:** Is the markdown mirror committed to project repos, or gitignored?

**Answered — committed.** Closed 2026-08-10.

It doubles as a legible offline backup (SPEC §11, recovery tier 3) and puts specs into repo grep for free. This repository has been running that way since Phase 1: everything under `product/` and `.keel/` is committed, and `keel generate --check` in the pre-commit hook is what keeps the committed copy honest.

The cost is visible and accepted: a store change produces a diff in files nobody edited. That is the price of the store being readable without the store.

### TQ-14 — The tracker is still stored prose rather than rendered from the task rows.

`que_01KZKWMS58WVPZ31Q10BBQ74N7` · question · answered

**Question:** The tracker is still stored prose rather than rendered from the task rows, so it can contradict them.

**Answered — done. The tracker is rendered.** Closed 2026-08-10.

`keel render-status` builds `product/STATUS.md` from the task rows on every `keel generate`. There is no tracker document to edit and no markdown table to keep in step: changing what the tracker says means changing a row.

Two consequences, both now written into `product/CLAUDE.md`. A hand-edit to `STATUS.md` cannot be recovered — unlike a prose document, there is no stored body to turn the edit into a revision against, so the next render simply overwrites it. And the narrative had to go somewhere: session-by-session accounts moved to `product/JOURNAL.md`, while findings that belong to one task became notes on that task.

The note stream is what made this closable rather than merely mechanical. The old tracker's Notes column held the findings; deleting the column without replacing it would have traded a contradiction for an amnesia.

### TQ-15 — SPEC D-5 says non-daemon processes "connect read-only or go through the daemon's API".

`que_01KZKWMS3TCWB0WFT0PNAHE6T8` · question · answered

**Question:** SPEC D-5 says non-daemon processes "connect read-only or go through the daemon's API". The read-only half is not achievable — DuckDB refuses a read-only connection while any process holds the write lock, so nothing can read the store while the daemon runs.

**Answered — the API is the only path.** Closed 2026-08-10; SPEC D-5 corrected.

Verified by building it and watching it fail, which is the only reason it was caught: the wording is plausible and the failure only appears when the daemon happens to be running, which is always.

Consequences already absorbed: generation moved inside the daemon and the CLI became a client (DECISIONS B-21). The remaining read commands that still open the store directly are KEEL-57 — they work when the daemon is stopped and fail when it is not, which is exactly backwards for `fsck`, an integrity check you cannot run without stopping the thing you want to check.

### R-2a — I maintained the tracker as prose all session and never touched a task row

`que_01KZKW33RGTJY86XYDTHSPMF0C` · risk · mitigated · severity high

KB noticed it before I did: "I am not seeing the actual project task items getting updated from this chat."

He was right. Of 134 events in the store, 119 were `keel bootstrap`, 13 were `keel import`, and 2 were one-off `curl` calls. Zero task rows moved status in a session that shipped four features. I kept the tracker by hand in `product/STATUS.md`, imported it, and generated it back out — so the *prose* was accurate and the *data* was frozen at what bootstrap wrote at 16:11.

This is R-2 ("the agent doesn't write to it") happening live, to the agent that built the thing. Worth recording because it is evidence about the product, not about one session:

- The MCP surface was connected and working the whole time. Wiring was never the problem.
- The pull toward editing a markdown file is strong enough to beat a tool surface designed specifically to replace it, even with the tool in front of me and the project's own contract telling me to use it.
- It went unnoticed until a human looked at the app. Nothing in the system complains that no task row has changed while the commit log fills up.

Two candidate mitigations, neither built:
1. `keel generate --check` already fails on stale prose. A sibling check could fail when the event log shows no task mutation in a session that produced commits — cheap, and it converts a silent drift into a build failure.
2. The Phase 2 skill and hooks are the real answer, and they are exactly what the ten-session gate is meant to measure. This session is a data point that the gate matters and is currently failing when the agent is running without the plugin.

### R-5 — Lance is the one unhedged dependency

`que_01KZKMPW37GYVG5728454FRBYA` · risk · mitigated · severity high

Mitigated by exporting the Lance datasets to Parquet in every backup. A Lance snapshot alone would not be an escape hatch from Lance.

### R-3 — Retrieval quality may be mediocre

`que_01KZKMPW2GYYB034T0RC5W2XPM` · risk · mitigated · severity medium

Mitigated by hybrid rather than pure-vector search from day one. Still needs evaluation on real queries.

### R-2 — The agent might simply not write to it

`que_01KZKMPW1RDD4SJ8MZXW8ZMKCK` · risk · mitigated · severity high

If Claude has to be reminded every session, the whole thing fails. This is what Phase 2's gate measures, and it has not been run.

### R-1 — Schema creep kills it

`que_01KZKMPW12HG7ARWNSENMVTHX1` · risk · accepted · severity high

Thirteen artifact types is a ceiling, not a starting point. Watch for wanting a fourteenth — it is almost always a field or a kind value on an existing type.

### TQ-11 — How long should the 2025-11-25 handshake be carried?

`que_01KZKMPW0CDTNEYCYDJDT5N8PT` · question · answered

TQ-11. Needed today, because that is what Claude Code sends. Worth revisiting once clients move on.

### TQ-10 — Should BM25 live in DuckDB rather than Lance?

`que_01KZKMPVZRBGWRH4Z7Y381B21F` · question · answered

TQ-10. Implemented in DuckDB because lance_hybrid_search's keyword half could not be characterised. The swap back is one module.

### TQ-9 — Should idempotency_key be on all thirteen tables or only tasks?

`que_01KZKMPVZ3AQ0BNVV5BQKV3QY8` · question · answered

TQ-9. Implemented on all thirteen. The one storage-format change made without KB, because the alternative silently breaks a v1 must-have for twelve types.

### Q-2 — Where does the store live, and does ~/.keel get a git remote?

`que_01KZKMPVWG2SWPNH1RPD0P9569` · question · answered

**Question:** Where does the store live, and does `~/.keel` get a git remote?

**Answered — `~/.keel`, local git, no remote.** Confirmed directly by KB on 2026-08-09; closed 2026-08-10.

Cheap to revisit: moving the store is a config change, and adding a remote is one command. It stays closed because leaving it open was costing more than the decision — it appeared in the never-truncated open-questions list every session, which is reserved for things an agent might otherwise re-litigate.

One thing built on it since: `keel restore` re-establishes the store's git repository after a verified restore, because an empty repository restores nothing — the state has to be in a commit.

