# Questions and risks

<!-- keel:generated questions prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from Keel — edits here are not saved.

## Open

*Nothing here is decided. Do not build on any of it without saying so.*

### Does a browser-served write/intake endpoint require amending hard constraint 7?

`que_01KZSQHK2C0CTKN36WJ9G4ZHQC` · question · open

### Enforce the single-writer rule with an advisory lock file, or rely on the health probe alone?

`que_01KZSQHFQ454B2S53T3SCYN4TC` · question · open

### TQ-37 — Six rows of SPEC §13 argue from DuckDB and Lance. Reword, annotate, or leave?

`que_01KZSKBNXFZCB84AV1V0MDM8RA` · question · open · severity low

**Needs KB.** Raised 2026-08-12, closing KEEL-132.

The standing contract says the decisions in SPEC §13 are KB's, so KEEL-132 rewrote §1 to §12 and §14 and did not touch the table. That leaves six rows arguing from an engine that is no longer in the tree:

| Row | What it now says that is not true |
|---|---|
| D-1a | Its closing clause says "§3 onwards still describes the old shape and is being brought up to date separately". That is what KEEL-132 just did, so the sentence has outlived its job. |
| D-2 | "Single unified `documents` **dataset**" — it is a table. |
| D-2b | "Revisions in user columns, not **Lance dataset versions**" — the thing it was distinguishing itself from is gone. |
| D-4 | "DuckPGQ can't run on 1.5.x alongside Lance" — the reason recursive CTEs won, and neither engine exists now. |
| D-5 | "**Quack** changes convenience, not architecture" — Quack is a DuckDB feature. |
| D-6 | "Storage engines are Rust-native" — SQLite is a C amalgamation compiled into the binary. |

Every one of those decisions still reaches the right conclusion. D-4 is the sharpest case: recursive CTEs are more right now than they were then, because Turso was ruled out during the Phase 9 survey for not supporting them at all — the conclusion held while its entire rationale was replaced.

## The three options

**Leave the table and let the sections carry the correction.** A blockquote under §13 already names all six rows and says why they were not touched. Cheapest, and it keeps the record of what was argued at the time. The cost is that a reader who reads only the table is misinformed.

**Annotate each row.** A second rationale line per row, in D-1's style: strike the clause that expired, say what replaced it. Six small edits, no conclusion changes, and it makes the table readable on its own.

**Rewrite the six rationales.** Cleanest to read and the most history lost. This is the one that would make the spec read as though it always said SQLite, which is the thing KEEL-132 was told not to do.

## Recommendation

The second. D-1 already demonstrates the pattern and it is the only option that leaves the table honest without erasing what it used to argue. It is a small edit either way, which is why it is worth asking rather than assuming: the value is in matching however KB wants the decision log to age.

### TQ-36 — The single write path is now a convention. Enforce it, or accept that?

`que_01KZSF88553AJM4WDBMVGP9ZSC` · question · open · severity medium

**Needs KB.** Raised 2026-08-11, from Phase 9's switchover.

## What changed

Hard constraint 1 says the daemon owns the single write path and no other process writes to the store. Under DuckDB that was enforced by the engine: DuckDB takes an exclusive write lock, so a second process trying to open the store read-write was refused. The constraint was a rule *and* a mechanism.

SQLite in WAL mode does not work that way. A second process can open the store and write to it while the daemon holds it, and it simply works — `keel note add` does exactly this and was watched doing it. Nothing is broken. Nothing was lost. But the guard rail is gone, and the constraint is now a convention that people and agents are asked to honour.

## Why this is not obviously bad

The reason for the single write path was never the lock. Six of the seven steps in a Keel write have nothing to do with locking — validation, provenance, the event, the revision, the embedding, the index — and all six still need one place that knows how to do them. That argument is untouched, and it is the argument recorded in D-5.

SQLite handles the concurrency correctly on its own: WAL gives readers a consistent snapshot and serialises writers. The failure mode is not corruption. It is a second writer skipping the five steps that are not the write.

## The three options

**Accept it, and say so.** Change the constraint from "no other process writes" to "everything that writes goes through `keel-core`'s write path", which is the thing that was actually being protected. Cheapest, honest, and it stops the contract claiming an enforcement that does not exist.

**Enforce it in the daemon.** A lock file, or a table row the daemon claims on start, checked by anything else opening the store read-write. Restores the mechanism, costs a failure mode of its own — a stale lock after a crash is a store nobody can open, which is worse than the problem.

**Enforce it in the type system.** Opening a store for writing outside the daemon requires something only the daemon can construct. Compile-time rather than runtime, no stale state, but it is a real refactor and the CLI legitimately writes when no daemon is running.

## Recommendation

The first. The constraint's *value* is the seven-step write path, not the exclusivity, and a contract that describes a guarantee the engine no longer provides is worse than one that describes what is true. The CLI writing directly when no daemon is running is a legitimate case that the DuckDB lock made awkward rather than safe.

But it is a hard constraint and hard constraints are KB's, so it is recorded rather than decided.

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

### Q-6 — Should Keel ingest anything automatically, or only explicit writes?

`que_01KZKMPVXPKW1KXV9KMG36C6F8` · question · open

Working assumption: explicit writes only, except the GitHub webhooks in SPEC §9. Governs push and deployment_status behaviour, and the write-amplification risk.

### Q-5 — What is the retention policy on the event log?

`que_01KZKMPVX3SNJ82BQ4GA3DF9S5` · question · open

It grows forever. Keep everything, which is probably fine for a decade at this write volume, or roll up events older than a year into daily summaries.

## Settled

*Decided, with the reasoning. Do not re-litigate these.*

### TQ-35 — Activity is rebuilt as "What changed", grouped by session

`que_01KZR4EZ97WRY0QFGAQ7DPPSZH` · question · answered

**Answered — rebuild it, grouped by session.** KB's call, 2026-08-11, against my recommendation of the cheaper fix.

## What gets built

The screen becomes "What changed": sessions newest first, each a one-line summary of what that session did, expandable to the full list.

> **Claude · 2 hours ago · 14 changes**
> closed KEEL-93, opened KEEL-97, wrote 3 notes, answered TQ-29

Every row links to the thing it changed — today none of them do. A marker for what is new since you were last here, from a timestamp in local storage. A today / this week / everything range, keeping the actor filter.

The "written outside a tracked session" treatment moves into the tooltip. That is a build-time concern showing as product copy, the same class KEEL-85 cleaned up elsewhere.

## The thing that makes this bigger than it looks

**Notes leave no row in `events`.** TQ-29 established that, and it is why the daemon announces notes under their own kind. So a per-session change count built from the event log alone would silently miss every note — which is exactly the part most worth reading, since a note is where a session records what it found.

Grouping by session therefore means unioning events with notes, not regrouping what is already fetched. That is the real cost, and the spec does not mention it.

## Why I recommended otherwise, and why that is fine

I argued for making the rows links and adding a time range first — an hour of work removing the most-cited defect — then deciding about grouping against a screen that worked. KB chose the full rebuild.

That is a reasonable call and the reasoning is on the record either way: "what happened while I was away" is the single most valuable question this app could answer for someone who leaves Claude working and comes back, and nothing else in the product answers it. The half-measure would have removed a complaint without answering the question.

Deletion was the third option and is now closed. The screen is the only place the event log is visible at all, and the event log is the attribution spine.

### TQ-34 — a task needs a summary, checked lightly rather than hard

`que_01KZR4E6MV7W8PYDRXWWSN8VZ1` · question · answered

**Answered — required, with light checking.** KB's call, 2026-08-11.

## What gets built

`summary` becomes a required property on `keel_create` for tasks: one or two plain sentences a colleague could read cold six weeks later, without having been in the conversation. The column is nullable in storage, because roughly 94 rows already exist without one and making them retroactively invalid would break every read.

**Two things are refused, and only two:** an empty summary, and one that only restates the title. The second reuses the containment rule written for near-duplicate titles (KEEL-65) pointed at a different pair of strings — one token set must be a subset of the other, so added words pass and a genuine restatement does not.

Nothing else is refused. No banned-phrase list on this field, no jargon check.

## Why light rather than hard

The mechanism is the required property, not the validator. A model physically cannot complete the call without confronting it, on every surface, whether or not any skill loaded — and that is what changes what gets written.

Checking harder trades that for a real risk: when a model is refused for something it does not agree with, its recovery is to satisfy the letter of the rule. It swaps the word and keeps the same weak sentence, so the prose ends up both bad and compliant while the check reports success. A false rejection is worse than a mediocre summary.

The evidence that a rejection works at all is on the record: two gate sessions sent `priority: "high"` against an enum of p0–p3 and both retried successfully in the same turn. One round trip, no human.

## What this does not settle

The house style itself is B-46 and already shipped for prose bodies, notes and documents. This decides that tasks must carry a summary and how strictly that one field is judged. The existing 94 rows are reported by `keel lint`, never rewritten — a machine inventing a summary produces exactly the confident, plausible, wrong prose the requirement exists to prevent.

## The honest limit, unchanged

A validator catches structure, not quality. It cannot tell a good sentence from a bad one and never will. What the requirement does is put the standard in front of the model at the moment of writing, every time, with no dependence on anyone remembering.

### TQ-33 — no URL fetching, and the daemon may read a file off the disk

`que_01KZR4D9DFQ3MCKMF72Q7Y8N8D` · question · answered

**Answered — the URL fetch stays closed, and `keel_attach(id, path)` is approved.** KB's call, 2026-08-11.

## The URL half was never open

TQ-6 settled it the same morning: the daemon does not make outbound network requests on a model's instruction. Phase 8 listed it as an open decision, which was stale by hours rather than wrong in principle. Confirmed, unchanged.

## The half that was genuinely new

The daemon may read a file already on the same machine. TQ-6's reasoning does not touch this: there is no outbound request, nothing has to be published first, and the bytes never enter the model's context.

This is the path that makes a real screenshot possible from Claude Code, which matters more now that app filing is declined (TQ-30). Base64 through a tool call costs the model 350,000 to 450,000 output tokens for a 1 MB image, so the useful ceiling there is nearer 100 KB — a small mockup, not a screenshot.

## Also agreed, and needing no decision

The base64 description advertises a 1 MB cap when roughly 100 KB is the most any session can actually reach. A description promising ten times what is usable is a trap, and correcting it is ordinary work rather than a change of policy.

## The boundary to hold

Reading a local path and fetching a URL look similar and are not. One touches the machine Keel is already running on; the other gives a model the ability to make the daemon talk to the internet. If a future change makes the path argument accept anything URL-shaped, that is this decision being reversed by accident.

### TQ-32 — no `triage` status for now, and `dropped` still needs correcting

`que_01KZR4CNDT72K8AXJV55KFNEMP` · question · answered

**Answered — not now.** KB's call, 2026-08-11: "since we are not doing new task right now let's skip."

## What this settles

The task status enum stays at five: `todo`, `in_progress`, `review`, `done`, `wont_do`. No sixth value, no migration.

The reasoning follows TQ-30 rather than standing on its own. `triage` existed so that something filed in a hurry from the app did not land in the same pile as planned work and compete with it. With app filing declined, nothing files in a hurry, so the holding pen has nothing to hold.

Worth keeping for whenever app filing returns: this would be the **sixth** value, not the seventh. The spec called it a seventh because it was counting a `blocked` status that KEEL-82 removed and TQ-25 settled.

## Still outstanding, and unrelated to the decision

`product/CLAUDE.md` tells every session: *"Never delete a task. Mark it `dropped` with a reason."* **There is no `dropped` status and never has been.** A session following that instruction literally gets an enum rejection listing five values, none of them the word. The intended value is `wont_do`.

That line sits in the contract loaded into every session, so it has been quietly failing for as long as it has been written down. It is a one-word correction and it does not depend on this question's answer.

### TQ-31 — Ten MCP tools becomes thirteen: all three verbs get their own tool

`que_01KZR4B6DX3572F0WS6ZAB0HWA` · question · answered

**Answered — thirteen. All three verbs become tools.** KB's call, 2026-08-11.

## The question

Phase 8 adds three verbs: "what should I work on", "I am starting this", "I am finished with this". The spec said that takes ten tools to twelve, but ten plus three is thirteen, and it never said which one was not meant to be a tool.

## What settled it

The thing that decides this is **findability at the moment of use**, not the count. This project already settled the same trade once: `keel_note` earned the tenth slot because a note *parameter* on `keel_update` was not findable, while a tool named for findings was — and note-writing went up when it got one.

That argument applies word for word to claiming and closing. Both are already possible through `keel_update`; both are simply not done. Across 66 tasks, the number of transitions into `in_progress` before work began was zero.

The two effects KB will actually see:

- A task visibly sitting in `in_progress` while it is being worked on, so the app answers "what is happening right now" rather than only "what has finished".
- A commit attached to every completed task, because the tool asks for one.

## What I got wrong first

I recommended twelve, on the grounds that two ways to close a task is how the two come to disagree. That reasoning does not survive: **the storage layer enforces the reason-message-evidence rule on every path into a terminal status regardless**, so drift is impossible under any option. With that removed, twelve is the least principled of the three — it gives claiming a front door and closing none, when they are the same shape of thing, purely to match a number in the spec.

## The cost, accepted

Thirteen tools rather than ten means a slightly larger menu and marginally more chance of a wrong selection on an unrelated call, plus roughly 200 tokens per request. `keel_update`'s description must point at the close tool so the overlap is signposted rather than left to chance.

**Thirteen is the new ceiling** and it should be defended the way ten was. The argument for a fourteenth has to be at least as good as this one.

### TQ-30 — filing issues from the app: not now, and the read-only rule stands

`que_01KZR4AHS7GC45EM0HSNN20SDM` · question · answered

**Answered — not now.** KB's call, 2026-08-11: "let's skip the ability to create a new issue at this time."

## What this settles

Hard constraint 7 stands unchanged. The desktop app stays read-only: no forms, no write endpoints on the daemon for it, Claude and Keel remain the only writers. Nothing in §8B is built.

The constraint was never amended, so there is nothing to reverse if this comes back.

## What it costs, stated plainly

- **The 30-second stopwatch exit criterion goes with it.** Filing a bug with a pasted screenshot from a cold start is not possible, and Phase 8 cannot claim that criterion.
- **The paste-a-screenshot path is the only one with no size problem.** Bytes going straight from the app to the daemon involve no model, where a real retina screenshot sent as base64 through a tool call costs 350,000 to 450,000 output tokens. Declining the app path means large images have no route in at all — except the file-path variant settled in TQ-33, which does cover Claude Code.
- **The `triage` status is moot for now** and TQ-32 is closed on the same reasoning: it existed so that things filed in a hurry did not compete with planned work, and nothing files in a hurry any more.

## If it returns

The argument in §8B is good and worth keeping: the app may *capture*, never *manage*. If that is wanted later, the thing to do is amend constraint 7 with the fence written into the rule itself rather than into an endpoint's doc comment — a comment is read by whoever is already editing the endpoint, which is exactly the person about to widen it.

### TQ-29 — a note does not announce, so the app does not live-refresh on one

`que_01KZQQXYTVDN1KCR6DHDEEV36M` · question · answered

**Question:** The daemon announces an SSE change only when the latest `events` id advances, and `keel_note` writes no event row — so a note produced no announcement and an open app showed a stale note stream with nothing to say it was stale.

**Answered — option (b): announce notes under their own kind.** Built 2026-08-11.

`Change` now carries a `kind` — `entity` or `note` — plus the `entity_id` the change is about when it is known. A note is announced through `announce_note`, which the `tools/call` arm reaches when the event id did *not* advance and the tool was `keel_note`.

**A field rather than a second SSE event name**, so a client that ignores it keeps working. The desktop app refreshes on every change as before; what it has now is the information to be smarter — a board need not redraw because a note landed on a task, and a task page should.

Verified on the live stream: a note emits `{"kind":"note","event_id":null,"entity_id":"tsk_…"}` and an update emits `{"kind":"entity","event_id":"evt_…"}`, in that order, from one connection.

The client parses defensively: a payload it cannot read still fires a refresh. Refetching on an unreadable change is the safe direction — the cost is one wasted read, and the alternative is showing stale state because a payload shape moved.

**Why not (c), giving notes an event row:** that changes what an event *is*. Events are the attribution spine — `keel_activity`, the changelog and the "what changed since" cursor all read them — and putting notes in there would put a second stream into every one of those without anyone asking. The narrow fix was the honest one.

### TQ-28 — renaming an artifact leaves an orphaned mirror file that reads as current

`que_01KZPNN754J219CB2A059J1TBE` · question · answered

**Question:** Renaming an artifact changes its mirror slug, and `generate` only ever writes — it never removes a file it used to produce. The old file stays on disk carrying a `keel:generated` banner and a real id, reading as current.

**Answered — prune on write, report on check.** KB's call, 2026-08-10. Built the same day.

`generate` reads the previous `.keel/manifest.json` before writing anything, and any path it lists that this run no longer produces is removed in `Write` mode and reported in `Check` mode. Orphans count towards `is_current`, so a repository carrying one fails `--check` and the pre-commit hook refuses the commit.

Bounded three ways, because this is the only place generation deletes: the path must have been produced by a previous run of this project, must live under `.keel/`, and must still be a file. A missing or unparseable manifest means "nothing known", never "everything is an orphan" — that reading is the one that could delete a tree, and it has its own test. So does a manifest naming `product/SPEC.md`, which must not be able to make generation delete a spec.

Every removal is named individually in the output rather than counted. This is the only thing generation deletes, and a deletion nobody can see is how a tool stops being trusted.

**Limit, stated deliberately:** pruning covers the mirror root only. An adopted document under `product/` orphans when its artifact is archived — as `QUESTIONS.md` and `DECISIONS.md` did this session — and those are still removed by hand. That is rare, deliberate, and a person is present; automatic deletion in `product/` is a larger act than this question asked for.

### TQ-27 — an accepted decision's title is immutable but its body is not

`que_01KZPKVNHC6P5HJ30KSFAVZ3QD` · question · answered

**Question:** `keel_update` refuses to change the title of an `accepted` decision, but `keel_write_doc` will replace that decision's entire body without complaint. All 25 decision bodies were rewritten this way while migrating the reasoning out of the prose table, and nothing objected.

**Answered — option 2: guard neither, and rely on the revision chain.** KB's call, 2026-08-10. Recorded as B-43; SPEC §3.2's note corrected.

The guard was on the wrong door. A title is a label; the body is the argument, and the argument is the decision. Guarding the label while leaving the argument writable stopped the harmless edit and permitted the harmful one.

What replaces it was already in place: every change is an attributed revision with a diff and an event naming the field, so a reworded decision is *visible* rather than prevented — and visible is the property the rule was reaching for. "Supersede rather than edit" survives as advice, which is what it always was.

Consequences already taken: the seven truncated titles are corrected (B-5, B-7, B-8, B-11, B-18, B-19, B-22), and the retitling exposed TQ-28 — a rename leaves an orphaned mirror file that reads as current.

### TQ-26 — the installed plugin is a hand-copy, and it drifts silently

`que_01KZP8C2BNKSN7GMJV4JSBZ29Q` · risk · answered · severity medium

**Question:** The hooks that actually run on this machine are copies under `~/.claude/skills/keel/`, made by hand. Nothing keeps them in step with `plugin/`, so a plugin change lands inert and nothing says so.

**Answered — option 1: `install.sh` copies them, and still will not touch `settings.json`.** KB's call, 2026-08-10. Built the same day.

`./plugin/install.sh` now installs `SKILL.md`, `session-start.sh` and `stop.sh` into `~/.claude/skills/keel/` (override with `KEEL_SKILL_DIR`), reporting each as installed, updated or unchanged. "Unchanged" is printed deliberately: it is the evidence that the copy is in step, which is the one fact the step exists to establish.

`--skill-only` skips the build and copies just those three files. That is the part that makes it work rather than merely exist — a full release build of a vendored DuckDB to copy three files is the friction that made hand-copying attractive in the first place.

**The distinction that makes this installation rather than interference:** `~/.claude/settings.json` is the user's file and the script still refuses to write it. `~/.claude/skills/keel/` holds this repository's files and nothing else authors them. The script does read `settings.json` to check whether it references the installed hooks, and says so if not — hooks that are installed but never invoked look exactly like hooks that do not work.

**Confirmed live.** At the moment of the fix the copies were already stale again — `stop.sh` and `SKILL.md` both differed, one session after being copied by hand — so this session's `stop.sh` change had been inert the whole time. Running `--skill-only` updated both; all three now match the repository.

**Also fixed:** `plugin/README.md` recommended the exact `cp -r` that caused the drift, and told anyone running the gate to use it.

**Not done, deliberately:** nothing *detects* drift between installs — re-running the command is what keeps them in step. A check would be option 3, and it earns its place only if this turns out to be forgotten in practice.

**Related, and unchanged:** `~/.local/bin/keel` may still be an old build. A full `./plugin/install.sh` replaces it. The false-drift problem it used to cause in the pre-commit hook is gone independently: that hook now picks the newest usable binary rather than the first one it finds.

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

### TQ-24 — keel_activity gained an `entity` parameter without asking

`que_01KZNQ3KCH7JQ0Z48JKQWRX3Y0` · question · answered

**Question:** `keel_activity` gained an `entity` parameter that nobody asked for. It returned one row's whole history, short-circuiting the project feed.

**Answered — removed.** KB's call, 2026-08-10.

It was a second question wearing the first one's name. "What changed across the project since I last looked" pages forward from a cursor; "how did this task get here" wants one row's whole story at once. Bolting the second onto the first meant a parameter that silently ignored `project`, `since` and `cursor` — three arguments a model could pass and watch do nothing.

The tool now advertises `project`, `since`, `cursor` and `limit`, and its description points at `keel_get` for a single row: a note says what was *learned*, where an event says only which field moved.

**The capability was real, and it moved rather than dying.** The desktop app's task-detail history panel was the only caller. It now reads `GET /api/entity/{id}/history`, which is B-15's own pattern — the local API has more endpoints than the tool surface has tools, because a UI knows exactly what it wants and a model chooses worse among more options.

A parameter on `/api/activity` would have been the wrong fix even though it needed no new route: that URL *is* `keel_activity`, and a query parameter the tool does not declare is precisely the two-surfaces-that-resemble-each-other problem KEEL-89 removed. Its own endpoint keeps the two honest.

Two snapshot tests went with it. Note what they had been asserting: that a mistyped entity produced an actionable error. Good tests of a parameter that should not have existed — being well tested is not the same as being wanted.

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

### TQ-6 — How does a design image get into Keel from a Claude chat session?

`que_01KZKMPVYBQVZG4MSWFSKHCNTB` · question · answered

**Question:** How does a design artifact's image get *into* Keel from a Claude session? Cowork can send files; Claude Code can read them; Claude chat is harder.

**Answered — base64 in the tool call.** KB's call, 2026-08-11. Built the same day; KEEL-46 closed.

`keel_create` with type `design` or `artifact` takes an `image` field: standard base64, or a full `data:image/png;base64,…` URL, because a model that has just been handed an image will produce either. Whitespace is stripped first — a payload wrapped across lines is valid intent and invalid base64, and failing on it would be a papercut with no upside.

It won on the one criterion that mattered: it is the only path that works from **every** surface. A filesystem path works only where there is a filesystem, which excludes chat and Cowork — the two places design images actually come from — so it would have answered the question by declining it. Fetching a URL would have made the daemon issue outbound network requests on a model's instruction, which is a larger capability than this needed, and requires the image to be public first.

**Capped at 1 MB decoded.** The constraint is context, not storage: base64 costs about a third again on top of the bytes, so a 1 MB image is roughly 1.4 MB of a context window. Lance would happily take 50 MB and the model would drown carrying it there. An oversized image is refused **with its actual size and the limit**, never truncated — a truncated image is a corrupt file that looks like a successful write.

Three details worth keeping:

- **The media type is sniffed from the magic bytes**, not taken from the `data:` URL's declaration. The declared type is whatever the sender wrote, and the app decides how to render from it.
- **Validation happens before anything is written**, so a refused image leaves no half-made design behind.
- **The blob names its owner.** Stored after the entity exists and then linked, rather than in one write, because a blob with a null `entity_id` is invisible to `fsck`'s referential checks — and an image nothing can trace back to an artifact is how a store fills with bytes nobody dares delete.

The bytes are served raw at `GET /api/blob/{id}`, with `immutable` caching since a blob id names one sequence of bytes forever. That is what the app's `<img src>` points at; base64 in JSON would have paid the encoding tax twice.

### Q-2 — Where does the store live, and does ~/.keel get a git remote?

`que_01KZKMPVWG2SWPNH1RPD0P9569` · question · answered

**Question:** Where does the store live, and does `~/.keel` get a git remote?

**Answered — `~/.keel`, local git, no remote.** Confirmed directly by KB on 2026-08-09; closed 2026-08-10.

Cheap to revisit: moving the store is a config change, and adding a remote is one command. It stays closed because leaving it open was costing more than the decision — it appeared in the never-truncated open-questions list every session, which is reserved for things an agent might otherwise re-litigate.

One thing built on it since: `keel restore` re-establishes the store's git repository after a verified restore, because an empty repository restores nothing — the state has to be in a commit.

