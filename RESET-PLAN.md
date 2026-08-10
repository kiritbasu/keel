# Keel — what needs to happen

Written 10 August 2026, after a full read of the codebase, the twenty documents, the task rows and the git history.

This document is in plain English. Where I use a term from the codebase I explain it the first time.

---

## Part 1 — What I found

### The short version

The engine is genuinely good. The storage layer, the search, the graph, the concurrency handling — that work is solid and over-built rather than under-built. The problem is that almost nothing was spent on the thing you actually look at.

Some numbers, because they explain the feeling better than any description:

| | Lines of code |
|---|---|
| The storage and core library | 17,768 |
| The MCP protocol layer | 4,109 |
| **The machinery built to measure whether Claude writes to Keel** | **1,671** |
| **The entire desktop app** | **2,243** |
| The seven screens in that app | 1,231 |
| The task board — the thing the product is for | **246** |

The measurement apparatus is three quarters the size of the whole application. The board is 246 lines. That ratio is the frankenstein feeling, and it is not your imagination.

The same pattern in the documents: the PRD and SPEC together are 9,888 words. The six documents about the measurement exercise are ~11,500 words. There are twenty markdown files in a project whose stated success metric is "manual markdown files consulted per week → 0".

### Why it feels stitched together

Five specific causes. Each needs a different fix.

**1. The board is a dead end.** You can look at a task card, and that is all. The cards are not clickable — they are plain `<article>` elements with no click handler, no link, no hover state, not even keyboard focus. There is no task detail view anywhere in the application. Nothing in the app renders a single task on its own.

The most telling detail: the function that would fetch everything a detail view needs — the task, its body, its linked neighbours — is written, tested and **called from nowhere**. Someone anticipated the detail view and it never landed. That is why the only way to see a task's notes is the little `▸ 3 notes` expander you mentioned. It is not a design choice. It is the hole where the detail view should be.

**2. There is no address for anything.** The app has no router. Which screen you are on and which project you have selected are two variables in memory. There is no URL for a project, a board, a task or a search. You cannot link to a task, bookmark one, send one to yourself, or press Back. Refreshing puts you at Home with nothing selected. Linear's entire feel rests on every issue having an address; Keel has none.

**3. Seven screens built to seven different rules.** There is no shared page frame. The board is full-bleed with horizontal scroll; Project and Home are centred at one width; Roadmap, Search and Activity at a narrower one; Documents is a two-pane split. No shared header, no breadcrumb, no consistent title bar.

Underneath that: eleven different font sizes with no scale and no names, six hand-copied duplicates of the same filter-chip styling that have already drifted apart, and no Button component at all — so every clickable thing re-declares its own appearance. The colour system is actually good and consistent. Everything above the colour system is ad hoc.

**4. The internals are showing.** On every card sits a raw identifier like `tsk_01KZKW28CS4Q1WSB0D95B2A01G`, with a tooltip telling you to click and copy it manually. The project dashboard displays `digest ≈ 12,400 tokens` and warns you when it is "over budget" — that is an agent-context measurement on a human's dashboard. The activity feed says `unattributed` and cites decision "D-10" in its tooltip. An empty state tells you to "Ask Claude to write one — `keel_write_doc`". Another says to run `keel fixture` against a scratch store. These are notes-to-self from the build, rendered as product copy.

**5. The tracker can contradict itself.** Two different parts of the system decide what "blocked" means and they do not agree. The generated `STATUS.md` counts a task as blocked if its status field says `blocked`. The "what's next" ranking counts a task as blocked if something is linked to it with a `blocks` edge. A task can easily be one and not the other — and then the file and the app state different facts about the same task, with nothing noticing.

Related: `closed_at`, the field that records when a task finished, is **never written by any live code path**. Every task in the store that is marked done has no completion date. Nothing about throughput, cycle time or "what closed this week" is computable.

And one more that is worth knowing: the middle column of the board has never been used. Across 66 tasks, there have been **zero** transitions into `in_progress`. It was recently patched by having the session-start hook ask Claude to claim a task, but that is a prompt, not a mechanism.

### About STATUS.md

You said tasks used to be stored in `status.md` and that it has now been cleared. Here is exactly what happened.

Until yesterday, `product/STATUS.md` was a hand-written document. Someone — usually Claude — maintained it as prose, and it was rich: each task had findings written under it, and there were long session narratives explaining what was tried and what broke.

Yesterday's commit changed it to be **generated from the task rows in the database**. The rich prose was split three ways:

- Per-task findings became "notes" attached to the task rows (50 of them were migrated by hand).
- Seventeen session narratives that no task row could carry moved into a new file, `product/JOURNAL.md`.
- The rest is now rendered by a command from the rows themselves.

Nothing was destroyed. But the file you were used to reading was replaced by a machine-rendered version with a different shape, and the parts you valued were scattered into two other places. That is the "cleared" feeling, and it is legitimate.

There is also a real hazard here worth fixing: the command that writes that file (`keel render-status --out`) overwrites the target unconditionally — no comparison, no safety check, no backup. It resolves the project by name, slug or ID with fuzzy matching. Point it at a near-duplicate project by accident and it will silently write that project's near-empty tracker over your real one. That is a genuine data-loss path and it is one of the fixes below.

### The measurement detour

Roughly half the project's second day went into one question: will Claude write to Keel without being told to? That produced seven test runs, about 71 real Claude sessions, 1,671 lines of instrumentation, and six documents.

It was not wasted — it found that skills don't fire on ordinary engineering prompts, it produced the session-start hook, and it caught two real product bugs. But roughly half the improvement came from fixing the *measuring instrument*, not the product. And an outside review that was commissioned during it said the thing worth repeating:

> The gate should have blocked Phases 0–3, not Phases 4–5. A nine-relation typed graph with cycle guards, for a store holding 29 links, is the signature of ordering work by what was *buildable* rather than by what was *uncertain*.

You have decided to freeze this. I agree.

### The MCP surface

You asked for the full footprint. It works, but it has accumulated a lot of loose ends in two days. The ones that matter:

**Things that are simply broken:**

- The hook that is supposed to capture your file edits back into the database calls a command, `keel mirror`, **that does not exist**. It fails silently every time. The safety claim written into that hook and into the plugin README — "the database wins unconditionally afterwards, the file is regenerated" — is therefore false. The window for silent divergence that the design says cannot exist, exists.
- That same hook mangles content: it captures the document's heading along with the body, so every edit round-trip adds another copy of the title. Edit a file three times and the title appears three times.
- It also reads an environment variable, `KEEL_SESSION_ID`, that **nothing anywhere sets**. So every revision it writes is recorded as coming from nobody.
- The skill tells Claude to invent its own session identifier. The session-start hook tells Claude to use a specific one and not to invent one. Both are in Claude's context at the same time. The stop hook only recognises the hook's format — so a session that follows the skill, writes correctly, and then gets told at the end that it recorded nothing.

**Things that are duplicated:**

- The instruction "record it, don't ask permission" is written in three separate files. Change one and the others silently disagree.
- The session-identity instruction is in three places, and two of them conflict (above).
- The server's introduction text exists word-for-word in two source files. Edit one and the two ways a client can ask "who are you" give different answers.
- The list of tools and the code that runs tools are two separately hand-maintained lists with no test tying them together.

**Things that have drifted:**

- The codebase says "nine tools" in five places. There are ten. The skill's tool table omits the tenth entirely — `keel_note`, the one for recording findings.
- The server tells clients it speaks one protocol version. It actually accepts two.
- One tool's description shows an example using a parameter name that doesn't exist (`id` where the parameter is `ids`). A model copying it verbatim gets an error.
- One tool accepts seven undocumented parameter names that aren't in its schema.
- The plugin's own README describes one hook. There are three.

**Things that are quietly wrong:**

- The local web API silently drops list-type filters. Asking it to search only specs returns everything, with no error. The same request over MCP filters correctly.
- The same API turns numeric-looking text into numbers, so searching for "404" fails with "query must be a string".
- It returns errors in three mutually incompatible shapes depending on which endpoint you hit.
- The one POST endpoint is unreachable from the desktop app because the cross-origin rules only permit GET.
- Every single tool call scans up to 100,000 event rows twice, just to read the last event ID — while holding the global write lock.
- Asking for a tool that doesn't exist returns HTTP 404, which by the protocol's own documentation is the signal for "there is no MCP server at this address". A typo looks like the server vanished.

**Dead weight:** a constant that is defined and never read; an error code defined and never used; handling for two protocol methods that are neither routed nor advertised; a lookup function used only by tests; two session-ID markers that nothing produces.

### The documents

Twenty markdown files. Fourteen are generated from the database. The problems:

- `README.md` is materially wrong — wrong test count, and it says the gate is unrun when seven runs have happened since.
- `PROBLEMS.md` reads as current and is entirely out of date; its central claim was refuted by the review it was written for.
- There are **two decision registers that are different sets**: a numbered table inside `DECISIONS.md`, and thirty separate artifact files. The product's own integrity checker has to be specially configured to route around this.
- There are **two question registers** with different contents, for the same reason.
- `JOURNAL.md` re-narrates four other documents nearly section by section.
- The SPEC is knowingly wrong in three named places and none has been corrected.
- A decision that was reversed was never marked superseded; the "Reversals" table built for exactly this is empty.
- A task sitting in `todo` has as its own first note the word "Done."
- Three tasks called "Decide TQ-9 / TQ-10 / TQ-11" are still open, although all three were decided and the decisions are recorded elsewhere in the same file.
- The open-questions count on the dashboard is 17. One of them is a debug artifact titled "ZZ write probe after repair". Two more are finished. Seven have working answers nobody has objected to. Genuinely open and unanswered: about seven.

---

## Part 2 — What we're building

Your decisions, recorded:

- **The app stays read-only.** Claude and Keel remain the only writers. The Linear/Shortcut quality is about how it *reads and navigates*, not about editing in place.
- **Add readable IDs and ordering**, and **sub-tasks (parent/child)**.
- **Freeze the measurement work.**
- **Document first, build on your approval.**

That gives two new phases.

---

## Phase 6 — Make the tracker real

Everything here is about the read experience. Nothing in this phase lets the app write.

### 6.1 — Foundations (do these first; everything else depends on them)

**Give everything an address.** Add routing so that every screen, project, task, document and search has a URL. Back and forward work. Refresh keeps you where you were. You can copy a link to a task.

**One page frame.** A single shell used by every screen: same header, same breadcrumb, same title area, same width behaviour. Seven screens stop looking like seven applications.

**A real design system.** Name the type scale — six sizes, not eleven ad hoc ones. Build the missing primitives: Button, Chip, Input, Menu, Dialog, Tooltip. Replace the six drifting copies of the filter chip with one. Fix the light theme, which currently keeps its dark-tuned status colours and washes out.

**A command palette.** Cmd-K opens it; type to jump to any project, task, document or search. Today the app deliberately ignores every modified key combination, so this needs that restriction lifted. This is the single feature that most makes an app feel like Linear.

### 6.2 — The task detail view

This is the centre of the phase and the thing whose absence you felt.

Clicking a card opens the task at its own URL. That page shows, properly laid out rather than crammed into a card:

- The readable ID and title, prominent.
- Status, priority, kind, labels, milestone — as a properties panel, not a row of badges.
- The description, rendered as markdown.
- **The note stream as a real activity feed** — full markdown, chronological, each note attributed to the session and person that wrote it, retracted notes visibly struck through rather than hidden. This is what the `▸ 3 notes` expander should have been.
- **Relationships, stated in English**: what blocks this, what this blocks, what spec it implements, what it duplicates, its parent, its sub-tasks. Every one of them a link you can click.
- **The task's own history** — status changes, field changes, before and after. The data for this already exists in the event log and has never been shown anywhere.
- Its external link (PR or issue), and — new — the ability to hold more than one.

Keyboard: `J`/`K` to move between tasks without leaving the detail view, `Esc` to go back.

### 6.3 — The board, the list, and finding things

**Two layouts, not one.** Keep the board. Add a list/table view — sortable columns, dense, scannable. Most real tracker work happens in a list, not a kanban.

**Grouping you choose.** Today grouping is hardcoded to status. Let it group by status, priority, milestone, label or parent.

**Sorting you choose.** Today sorting is hardcoded and subtly broken — it compares priorities as text, so a task with no priority sorts under the literal word "undefined".

**Filters that compose and survive.** Today there are exactly two, they don't combine, and they vanish on reload. Replace with a proper filter bar: multiple labels, status, priority, kind, milestone, has-blockers, free text. Encode the filter in the URL, so a filtered view *is* a shareable link — that also gives you saved views for free.

**Search that goes somewhere.** Search results are currently dead text. Make every hit a link to the thing it found.

**Keep "Next" visible.** The ranked next-up list is the best thing in the app and it currently disappears the moment you apply any filter. Fix that.

### 6.4 — Data model additions

**Readable identifiers.** Every project gets a short key; every task gets a number. `KEEL-42`, not `tsk_01KZKW28CS4Q1WSB0D95B2A01G`. Displayed everywhere, searchable, usable in conversation with Claude, and — importantly — usable in the generated `STATUS.md` so that file becomes readable too. The long IDs stay underneath as the real identity; the short one is a label.

**Ordering.** Add a rank to tasks so a column has a deliberate order rather than reverse-creation-order. Since the app is read-only, rank is set by Claude ("move that above the auth work") and respected by every view. The app can still offer you local sort options on top.

**Sub-tasks.** Add a parent link between tasks. A parent shows its children with a progress count; a child shows its parent. Rollups (how much of this epic is done) come from that. This is currently impossible — the only edge available is `blocks`, which means "must happen first", not "is part of".

### 6.5 — Make the tracker tell the truth

**One definition of blocked.** Status and links must never disagree. Pick one as authoritative — the links, since they say *what* is blocking — derive the rest, and make the integrity checker fail when a task says `blocked` with nothing linked to it, or is linked-blocked while claiming to be `todo`.

**Write `closed_at`.** Set it when a task reaches a terminal status. Backfill from the event log for tasks already closed. This unlocks every "what got done this week" question, which is currently unanswerable.

**Same numbers everywhere.** The app, the digest Claude reads, and the generated `STATUS.md` must all count open, urgent and blocked identically. Today they don't.

**Make `STATUS.md` safe.** No more unconditional overwrite. Compare before writing, require an exact project match rather than a fuzzy one, and refuse to write a dramatically smaller file without saying so. Also: render the "what's next" section into it — the best computation in the system is currently missing from the file a human actually reads.

### 6.6 — Speak English in the interface

Remove from the UI: raw identifiers as primary labels, token counts and budget warnings, tool names, internal decision references like "D-10", CLI instructions in empty states. Replace with plain language. "Unattributed" becomes "written outside a tracked session". The digest size readout moves to a diagnostics screen or disappears.

---

## Phase 7 — Clean up the footprint

### 7.1 — One authority per instruction

Right now three files each tell Claude the same three things in slightly different words, and two of them contradict each other. Fix by deciding, for each instruction, which file owns it:

- **Session identity** → the session-start hook owns it. Delete the skill's competing instruction. Make the stop hook accept whatever the hook issued. This one change fixes the false "you recorded nothing" nag.
- **When to write** → the skill owns it. The hook points at it rather than restating it.
- **The end-of-session check** → the stop hook owns it, and says one sentence.

Then make the session-start hook fire only at actual session start, not on every context compaction — today it re-injects a 250-word preamble every time the conversation compacts.

### 7.2 — Fix or delete the file-edit hook

Two of its three moving parts don't work and its stated safety guarantee is false. Two honest options:

- **Fix it**: point it at a command that exists, stop it capturing the heading, and give it a real session identity.
- **Delete it**: an outside review already recommended rejecting this mechanism as a write path, on the grounds that it turns structured edits into document revisions and loses them.

My recommendation is **delete it**, and replace it with something that fails loudly: a check that refuses a commit when a generated file has been hand-edited, telling you to make the change in Keel instead. A mechanism that silently doesn't work is worse than no mechanism.

### 7.3 — Tidy the tool surface

- Settle on ten tools and say ten everywhere. Add `keel_note` to the skill's table.
- Fix the description that uses a parameter name that doesn't exist.
- Put the seven undocumented parameters into the schema, or stop accepting them.
- Mark the required fields on `keel_note`, which currently declares none.
- Mark archive and remove as destructive; they currently claim not to be.
- Make `keel_update`'s description say when to use `keel_write_doc` instead — currently only one side of that pair explains the split, so a model reading the other will put a document body in the wrong place.
- Delete the dead code: the unread constant, the unused error code, the handling for two unadvertised protocol methods, the test-only lookup, the two phantom session markers.
- Add one test asserting the tool list and the code that runs tools agree. That class of drift then cannot recur.

### 7.4 — Make the local API and the MCP tools genuinely one thing

The code claims they can never drift. They already have.

- Route every endpoint through the same dispatch path. Four currently bypass it.
- Fix list-type parameters being silently dropped — a filter that is ignored without complaint is worse than one that errors.
- Stop coercing numeric-looking text into numbers.
- One error shape across the whole API, not three.
- Allow POST in the cross-origin rules so the one POST endpoint is reachable.
- Stop scanning 100,000 rows twice per call under the write lock.

### 7.5 — Protocol honesty

- Advertise both protocol versions, since both are accepted.
- Return "tool not found" as a tool error, not as the HTTP status that means "there is no server here".
- Keep the server's introduction text in one place.

### 7.6 — One name for one setting

The daemon's address is currently spelled three different ways across the hooks, hardcoded a fourth time in the plugin config, and defaulted a fifth time in the CLI. Pick one environment variable, make everything read it, make the plugin config respect it.

### 7.7 — Documents

- **Freeze the measurement work.** Keep the code, stop running it. Collapse the six gate documents into one short results page. Close the remaining gate tasks as "not now" with the result recorded.
- **Rewrite `README.md`** to say what is actually true today.
- **Retire `PROBLEMS.md`**, or date it clearly as a historical snapshot.
- **One decision register.** The numbered prose table and the thirty artifact files must become one thing. Same for the two question registers.
- **Prune the questions.** Delete the debug artifact. Close the two that are done. Mark the seven with working answers as decided. That takes the count from 17 to about seven real open questions.
- **Correct the three known errors in the SPEC**, and mark the reversed decision as superseded.
- **Reconcile the stale task rows**: the task whose own note says "Done", and the three "Decide TQ-9/10/11" tasks that were decided.
- **Cut the duplication in `JOURNAL.md`** so it stops re-telling four other documents.

---

## Order of work

Phase 6 and Phase 7 are independent — Phase 7 touches the agent surface, Phase 6 touches the human surface. I would do them in this order because each step makes the next one visible:

| # | Step | Why here |
|---|---|---|
| 1 | Routing, page shell, design system, command palette | Nothing else can look right until this exists |
| 2 | Task detail view | The hole you noticed; biggest single improvement |
| 3 | Readable IDs | Makes every other screen and the generated file legible |
| 4 | Board + list, grouping, sorting, filters in the URL | The tracker starts behaving like a tracker |
| 5 | Sub-tasks and ordering | Structure, once there's a place to show it |
| 6 | One definition of blocked, `closed_at`, safe `STATUS.md` | The tracker stops contradicting itself |
| 7 | Plain-English interface copy | Cheap, and removes the last of the "internal build" feel |
| 8 | Instruction authority + delete/fix the file-edit hook | Highest-risk correctness items in the MCP surface |
| 9 | Tool surface, API unification, protocol, config | The rest of the cleanup |
| 10 | Document reset and the question prune | Last, so it describes the finished state |

Steps 1–3 are what will change how the app feels. Steps 8–9 are what stop the quiet failures.

---

## Decisions this needs from you

These reverse or amend things already written down, so I want them explicit rather than assumed.

1. **The task model grows.** Readable IDs, rank and parent/child are new fields. The standing rule says no new *artifact types* without your agreement — these are fields, not types, so it holds. Confirming anyway.
2. **The desktop app stops being "read and search only, everything else is Claude's job" as a design ceiling** — it stays read-only, but it becomes the primary window, and it gets invested in accordingly. The PRD's ranking of the app as secondary should be amended to say so.
3. **The file-edit hook is deleted** (my recommendation) rather than repaired.
4. **The gate is frozen**, and Phase 2's result stands as measured, understanding that Phase 7 changes the surface it measured.
5. **`STATUS.md` gets a "what's next" section and readable IDs**, so it becomes something worth reading again rather than a machine dump.

---

## What "done" looks like

You should be able to:

- Open the app, press Cmd-K, type three letters, and land on a task.
- Click any card and get a real page: description, full note history, what's blocking it, what it blocks, its sub-tasks, its history.
- Copy that page's URL and send it to yourself.
- Filter the board to p0 bugs in the current milestone, and have that filter still be there tomorrow because it's in the URL.
- Refer to `KEEL-42` in a conversation with Claude and have both of you mean the same thing.
- Open `STATUS.md` and find it readable — named tasks, readable IDs, a "what's next" section, no wall of ULIDs.
- Trust that when the board says three things are blocked, three things are actually blocked.

And on the agent side: one instruction in one place, ten tools that describe themselves accurately, one API that behaves the same way whichever door you come in by, and no hooks that silently do nothing.

---

## One thing I'd say plainly

The spine of this project is well made and does not need rebuilding. What has never existed is the loop back to a person looking at a screen. Every time that loop closed — when you said the roadmap didn't tell you what was next, when you noticed nothing ever entered `in_progress`, when you asked where TQ-15 was — the product improved sharply within an evening. That has happened three times, and each time it was worth more than a gate run.

Phase 6 is that loop, built deliberately instead of by accident.
