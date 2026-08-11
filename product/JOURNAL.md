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

## Phase 8's working loop, and the instruction that became a mechanism

Twenty-one of Phase 8's twenty-three rows are closed. What landed is the loop itself — `keel ready`, `keel claim`, `keel close` — plus the last of §8C, `keel lint`, and the image path that makes a real screenshot possible.

**The numbers that made the three verbs worth building.** `product/CLAUDE.md` has told every session to move a task to `in_progress` before starting it. Across sixty-six tasks, the number of transitions into that state before work began was zero. The definition of done is a seven-item checklist an agent is *asked* to honour, and a hundred and seven closed tasks carry no record of what happened. Both are instructions, and instructions in a file lose to a model's own momentum every time.

So both became mechanisms, and the mechanism is in the storage layer rather than in the tool. A task cannot reach `done` or `wont_do` without a reason, a message and — for `done` — evidence, and a caller reaching for `keel_update(status: done)` to get round the tool is refused by the same check. That is what separates an invariant from a second convention.

**Ten tools became thirteen** (TQ-31, KB's call). I recommended twelve and the reasoning did not survive: I argued that two ways to close a task is how the two come to disagree, and the storage-layer check makes drift impossible under any option. With that gone, twelve was the least principled of the three — a front door for claiming and none for closing, purely to match a number.

**A claim needed no lock.** It goes through the ordinary optimistic-concurrency update with the version it read, so two sessions racing both read version 7, the first writes 8, and the second is rejected naming the current holder. Releasing lives in the store's update path, not in `close`, so no route into a terminal status can leave a claim standing.

**One ranking, three doors.** `keel ready` is a CLI command, an MCP tool and a screen, and a daemon test asserts the tool and the endpoint return the same references in the same order. Verified against the live store too: both said 11 ready, and 9 under Phase 8. An app that disagreed with the session about what to do next would be worse than one that stayed silent.

### The exit criteria, honestly

`keel lint` reports zero unexpanded identifiers, which was one criterion. It reported nine, and those nine task bodies said things like "Waiting on TQ-35." and "Decision B-45." — a sentence that names a thing and says nothing about it. Glossed by hand, because a machine writing them is precisely the failure the rule exists to prevent. The other 231 findings are missing summaries and historical closes, and those are not this task's to invent.

**The 30-second stopwatch criterion cannot be claimed, and will not be.** It measured filing a bug with a pasted screenshot from a cold start, and KB declined app filing (TQ-30). Hard constraint 7 stands unamended, so nothing has to be reversed if it comes back — but the criterion goes with it, and pretending otherwise would be the more expensive lie.

### Three things that only showed up by running it

**A test's margin depended on a response's size.** `a_client_in_a_loop_is_rate_limited` hammered `tools/list` a thousand times and relied on outrunning a fifty-per-second refill, which needs every call under 20ms. Adding three tool definitions made each call slower than that and the test began failing — the limiter was fine and the test was not. It fires `ping` in concurrent batches now.

**A count described the page rather than the project.** `keel lint --limit 12` reported "12 task_without_summary" under a total of 240, because the per-rule tally was derived from the truncated list. That number is what a person reads to decide what to work on. Caught by writing the test after the code, which is the wrong order and worked anyway.

**A close reads as five rows on the new What changed screen**, because the store writes one event per field and `keel_close` sets four. Collapsing them needs something identifying the call that produced them, and an event carries no correlation id. Left alone and recorded rather than papered over.

### Where a decision and a ceiling disagreed

TQ-33 approved `keel_attach(id, path)` by name. TQ-31, hours earlier, set thirteen tools as the ceiling. Both KB's. B-49 resolves them in favour of the capability without spending the slot: `image_path` is a field on `keel_create` and on `keel_update`, because the substance of TQ-33 is that the daemon may read a local file and the form is naming — the reversible half.

The boundary TQ-33 named is held with a test. Anything URL-shaped is refused with the reason in the message, for `https:`, `http:` and `file:`, because a path argument that quietly starts accepting a URL is that decision reversed by accident.

Verified against a live daemon: a 683 KB PNG went from a path to the store and back out of `/api/blob/{id}` byte-identical. Through base64 that file would have cost the model roughly 240,000 output tokens, which is why the description now states 100 KB rather than the 1 MB nobody could reach.


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


## The gate, told once instead of five times

*2026-08-10.* The account of the seven gate runs — the invalid instrument, the
validity audit that moved run 4 from 3 of 10 to 5, Run A at 7, Runs B and C at
9 apiece, and the hand-judged keep-rate of 87% — now lives in one place,
`product/GATE.md`. Five entries here used to re-tell it section by section,
alongside the six results documents that told it again at 11,700 words.

Cutting them is the point rather than housekeeping. The journal's job is what a
session *did* and what it felt like to be wrong; the gate's job is what was
measured. Those had merged, and the merged version was longer than the PRD and
the SPEC together while being harder to answer a question from.

Two things worth keeping here because they are journal rather than result:

**I built an instrument and then measured it for five evenings without knowing
that was what I was doing.** Every run from 1 to 4 produced a behavioural
conclusion — the agent asks permission, the agent will not write, the consent
prior is the problem — and all of them were reading a harness that ended the
conversation before the model's next turn. The tell was there from run 1: eleven
offers to record something, in ten transcripts, addressed to a human the harness
could not supply. I read that as evidence about the agent for five evenings.

**The thing that finally caught it was a rule, not an insight.** `observed ==
launched`, asserted before a score is reported. Seven silent sessions had been
vanishing from a run that then announced "3 of 3", because the units that fail
are exactly the ones that remove themselves from observation. No amount of
reading transcripts more carefully would have found that; a refusal to report a
number without accounting for every unit found it immediately.

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
