<!-- keel:generated spec spc_01KZNA1ZQPM0MGY86BHKE98DZA
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — Build journal

The session-by-session record: what was tried, what broke, what the
measurements actually said. Split out of the tracker when the tracker became
generated from task rows — this is the half no row will ever carry, and it is
the half worth reading twice.

Newest first.

---

## The three decisions, and what removing MCP cost

**TQ-9 — idempotency keys stay on all thirteen tables.** Confirmed. B-10 is no longer provisional. It earned it on organic traffic: across the gate runs, sessions called `create` twice with an identical title on nine occasions and the key deduplicated every one.

**TQ-10 — BM25 stays in DuckDB.** Confirmed. B-12 is no longer provisional, and **SPEC §5 is now formally wrong** about which engine ranks keywords; it should be corrected.

**TQ-11 — legacy MCP support removed.** KB's call, made knowing Claude Code 2.1.185 speaks 2025-11-25 and would stop connecting. Measured rather than predicted:

| | |
|---|---|
| Claude Code MCP | **✘ Failed to connect** |
| desktop app · both hooks · `generate` · `gate` | all fine |

The blast radius is narrower than "everything" — everything on the local REST API is untouched. What breaks is exactly the MCP tool surface: **how an agent writes to Keel**, which is what all of Phase 2 was about. Six tests now pin the refusal rather than the support, each naming TQ-11 so the next person to hit it knows it is deliberate. `git revert 3d1cc27` restores it.

**The immediate cost, which is worth seeing.** Everything below this line had to be recorded by editing prose and importing it, because the tools that write structured rows are gone. So these decisions exist as paragraphs and not as `decision` artifacts — which is precisely the prose-versus-structure split described under "the prose blob problem", arriving again by a different route. Restoring MCP would let them be written properly.


## Two precision fixes from the hand-judge

**Near-duplicate titles.** `create` now checks for a near-identical title after the exact key misses. The rule is overlap **plus containment** — one token set must be a subset of the other, so the difference can only be *added* words, never *substituted* ones. Overlap alone was wrong and a test caught it: sixty questions differing by one digit scored 0.875 and collapsed into two rows. Two more existing tests caught more — an explicit idempotency key is the caller asserting "these are different", and Q-4 requires a global and project-scoped term of the same name to coexist.

**Unresolved cross-references**, as an `fsck` check. Two wrong versions first: resolving against documents only reported 227 dangling refs in a store of ~250 artifacts, and even fixed it reported 182, because Keel's `B-n` decisions live in a prose table and dangle by construction. Scoped to families a project actually uses in titles, it found **six genuine breaks** — TQ-12 and TQ-13 cited in three documents after those rows were dropped. Both restored; `fsck` clean. It does **not** catch the case that motivated it, and that limitation is recorded on the task.


## `in_progress` had never once been used

KB: *"when Claude Code is working on updates I don't see anything go into the in-progress state."* True, and the data is stark — **57 done, 6 todo, 3 blocked, and zero transitions into `in_progress` across 66 tasks.** The middle column of the board has always been empty.

The cause is structural. `in_progress` needs the work named *before* it happens; agent sessions discover the shape of the work while doing it and record the outcome, so by the time there is something to write down it is finished.

Fixed KB's way: the SessionStart hook now asks a session to claim a task before starting, with the ids sitting directly beneath the instruction. That is still telling the model — but the hook is the one channel that demonstrably works here, and the id is right there.

**With a guard, because this creates the opposite failure.** A task claimed by a session that ended hours ago still reads as active work, and **a stale claim is worse than an empty column: empty says "nothing is tracked here", stale says "this is happening right now" and is wrong.** `fsck` now warns on anything `in_progress` for more than three days, and a test pins both sides — a fresh claim must not be nagged about, a five-day-old one must be.

Unmeasured: whether sessions actually do it. The gate harness could tell us, and has not yet.

---


## Step 10 — the hand-judge. 26 of 30 kept, and two defects recall cannot see

Every artifact from Runs B and C judged one at a time. Full write-up: `product/KEEP-RATE.md`.

**39 create calls → 30 distinct artifacts → 26 worth keeping. 87%.** The nine extras were exact-duplicate retries within a session, deduplicated by the idempotency key — REQ-7 working on organic traffic.

*Caveat stated in the document and worth repeating: I did this judging, and I built the harness. One interested judge on 30 rows.*

**The four drops, and two of them matter:**

- **Two speculative validation tasks.** One prompt about validating phases produced three tasks in each run. The `step ≤ 0` guard I kept — a genuine infinite loop found while in the file is what you want. The amplitude/speed task I dropped. That is R-6 in practice: not forty junk rows, but a third plausible one nobody will prioritise.
- **A clarifying question stored as project knowledge** (C2). Right to say it in the reply, wrong to record it — six months on it captures confusion about a prompt, not something true about the project.
- **A fabricated cross-reference** (C10). A question filed into Pellet cites "D-9" — which is a *Keel* decision. The store was empty when that session started. **The write happened, looked substantial, and is quietly poisoned.** That is exactly what a recall metric cannot see.

**The finding the gate structurally cannot produce:** Run B wrote *"Validate constituent phases to 0–360 degrees"*; Run C wrote *"Validate constituent phases to 0–360"*. The idempotency key hashes the normalised title, and normalisation lowercases and collapses whitespace — it has no idea those are the same task. Per-session stores hid it completely. **In a shared store, ten sessions on one project would produce near-duplicates idempotency does not catch** — UC-8's failure arriving one level below where everyone was watching. Both runs score it as success.

Both filed as p1. The second fix reuses the fuzzy match `keel_projects` already does, applied one level down.

**What is genuinely good**, since precision cuts both ways: typing is right (the harbourmaster call became `feedback`, blake3 stayed a `question` rather than a decision nobody made), several rows are things nobody asked for and everybody would want (a path-traversal hole in `get()`, "a wrong chart datum is a silent safety failure"), and the bodies carry evidence rather than restatement.

---


## Phase 2 is closed, and the p0 is fixed

**Phase 2 shipped 2026-08-10** on 18 of 20 pooled. The decision states what is carried rather than resolved: a 69.9% pooled lower bound, no precision floor yet, chat and Cowork untested.

### The p0 — and I had it wrong twice before getting it right

The real error, once the chain was actually printed:

```
FATAL Error: Invalid Input Error: Failed to delete all rows from index.
Only deleted 0 out of 1 rows.
```

A DuckDB **ART index disagreeing with its table**. A `FATAL` poisons the connection, so every later query fails with whatever operation happened to be running — `count matching rows` on a create, `run a question lookup` on an update. Reads on a freshly started process worked because they never touched the damaged index; `fsck` reported clean because it checks referential integrity, not index consistency. Every observation was true and the conclusion was still wrong.

**My previous entry here blamed the FTS index. That was wrong.** I had searched *after* a write already poisoned the connection. Search on a genuinely fresh daemon returns hits — the FTS index was never involved.

**The cause was cruder than any hypothesis on the table.** Graceful shutdown waits for in-flight connections, and `/api/events` is an SSE stream that by design never ends. So `SIGTERM` never completed, and every restart this session ended in `SIGKILL` — repeatedly, mid-write. That is how an index and its table stop agreeing.

Worth noting against the panel's Step 8, which predicted *two write paths where only the daemon maintains derived state*: a good hypothesis, and not the cause.

**Three fixes:**

1. **Shutdown on a deadline, with a checkpoint.** Five seconds, then close anyway, always `CHECKPOINT` first. Verified with an SSE stream open: stops in 5s and logs the checkpoint, where it used to hang indefinitely.
2. **Error chains surfaced.** `Error::chain()` walks to the root cause and the MCP boundary reports it. The source was attached the whole time and nothing ever printed it — which is why two hours went into guessing instead of reading. This is Step 8's `#[source]` item, and it paid for itself immediately.
3. **A regression test** for the exact cycle: create, checkpoint, reopen, update, and assert the connection is not poisoned.

**One more bug, found one command before I deleted the only copy that still had it.** `~/.keel` is meant to be its own git repository — SPEC §11's recovery tier 1, the one with full fidelity including every revision. `keel restore` rebuilds from tier 2 into a *fresh* directory and handed back a store with no `.git`, so **restoring silently cost you the best recovery tier**. Worse for when it fires: you only restore after something has already gone wrong, so the moment you use tier 2 is the moment you lose tier 1. `verify_restore` passed throughout, because it checks rows rather than recovery properties.

Fixed in `keel-cli` rather than `keel-core`, mirroring `plugin/install.sh` — core does not spawn processes, and "a store should be a git repo" is policy, not storage. After a verified restore it runs `git init`, writes the `models/` ignore, and commits the restored state, because an empty repository restores nothing. It never fails the restore: a missing `git` prints the exact command to run instead. Two tests, and a real backup→restore cycle verified end to end.

**Recovery** used `keel backup` and `keel restore`, which rebuild every table and index from Parquet — 536 rows, verified per table. The damaged store is kept at `~/.keel.corrupt-20260810T053513Z`, the Parquet backup at `/tmp/keel-backup-repair`. **No data was lost**, and the four question updates that failed all evening have landed.

---


## Runs B and C — 9 of 10, twice. The criterion is met.

Step 6's Stop hook built and measured on two independent draws. Full write-up: `product/RUN-B-C.md`.

| Run | Condition | Wrote | Offers |
|---|---|---|---|
| 1–3 | skill only (never loaded) | 0–1 of 10 | 3 each |
| 4 | SessionStart hook, single-turn harness | 5 of 10 | 8 |
| A | repaired instrument, no new treatment | 7 of 10 | 1 |
| **B** | **+ Stop hook** | **9 of 10** | 1 |
| **C** | **confirmatory** | **9 of 10** | 1 |

**Pooled 18 of 20 · 90% · 95% CI [69.9%, 97.2%].**

**The hook's specificity is better evidence than the score.** In Run B it fired in sessions 2, 7 and 9 — *exactly* the three that missed in Run A — and in no others, converting two. It stays silent for sessions that already recorded, which is what stops a forcing function becoming noise someone disables. s7 is the one it could not reach: it got the nudge and answered the user's follow-up instead.

**Three failures, three fixes, none of them the wording:**

- **SessionStart hook** → fixed *noticing and intent*. Ceiling ~30% → 80%.
- **Continuation turn** → fixed *execution*. Offers 8 → 1.
- **Stop hook** → fixed the *residual*: heads-down implementation sessions where the digest is thousands of tokens back with no salience.

**Precision, since a recall metric is least trustworthy when recall is high.** 19 create calls → ~15 distinct artifacts across nine sessions, typed correctly (`feedback` for the harbourmaster call, `decision` for the resolution choice). Two honest blemishes: four sessions called `create` twice with identical titles — idempotency deduplicated them, REQ-7 working on organic traffic for the first time, but the retry suggests the success response is not clearly acknowledging the write — and s6 made three tasks from a prompt about one validation, the first organic R-6 instance.

**Phase 2 is not closed, and I have not closed it.** The criterion is met; the precision floor (Step 10) does not exist, the pooled lower bound is 69.9%, and chat and Cowork have neither hook and are untested. Raised as **TQ-22** — "criterion satisfied" and "phase closed" are separable, and the second is KB's.

---


## Run A — 7 of 10, and the permission failure is gone

Step 2 built, Run A executed against it. No treatment; only committed fixes and instrument repairs. Full write-up: `product/RUN-A.md`.

```
   3  L1 did not notice
   7  L5 wrote

  recall      70%    ceiling  70%    offers  1
```

| Run | Condition | Wrote |
|---|---|---|
| 1–3 | skill only (never loaded) | 0–1 of 10 |
| 4 | hook, single-turn harness | 5 of 10 *(reported as 3)* |
| **A** | **same treatment, repaired instrument** | **7 of 10** |

**Recall equals ceiling** — every session that formed the intent completed it — and offers fell from eleven to **one**. The panel's central prediction is confirmed: those offers were never refusals, they were addressed to a turn the harness could not supply. Given one, they resolve into writes. Every writing session also created its project unprompted, so the TQ-17 cold-start deadlock is gone too.

The writes are real, not noise — decisions, questions and tasks matching each conversation ("Tide table default resolution is 15 minutes", "Chart datum is hardcoded, not per-station", "Size cap on the store with LRU eviction").

**The residual is a different problem.** The three silent sessions — s2, s7, s9 — are all pure implementation prompts. None offered; none noticed. That is orientation-in-context, not consent. Whether it is even a failure is a human judgement (TQ-21), and it moves the score between 7/10 and 7/7.

**Step 2's repairs:** transcript-based scoring (one file per session, ids cannot collide) · launcher-injected session id · a neutral continuation turn · parallel with one store per session, 7 minutes not 20 · an `observed == launched` assertion · known-answer fixtures · archive before teardown.

**Two bugs the repaired instrument caught in itself**, both flagged by an implausible score rather than a test: scoring checked one store for ten per-session stores (recall 0% against ceiling 70%), and `t0` came from a directory mtime that updated after the events it was meant to bound. Both were fixable without re-running because the stores were kept up.

**All 41 archived sessions rescored**, which separates two changes that had been tangled:

| Run | Condition | Did not notice | Ceiling | Offers | Wrote |
|---|---|---|---|---|---|
| 1–3 | skill only | 5 / 4 / 4 | 40% / 30% / 27% | 3 each | 1 / 0 / 0 |
| 4 | + SessionStart hook | 2 | **80%** | 8 | 5 |
| A | + continuation turn | 3 | 70% | **1** | **7** |

The hook fixed *noticing and intent* — ceiling from ~30% to 80%. The continuation turn fixed *execution* — run 4 had a **higher** ceiling than Run A and wrote fewer, the whole difference being eight offers versus one. So the single-turn harness was suppressing roughly three writes per run, and run 4's real ceiling of 80% was the highest ever recorded. The run that triggered "the premise may be dead" was the one where the model most reliably worked out what should be recorded.

**Caution:** 7/10 at n=10 from two projects is one draw. It does not establish 70%, and it does not establish that 9 is reachable.

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
