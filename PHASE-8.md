<!-- specline:generated spec spc_01KZR487EHQGGE3HV3JH3XN213
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# Keel — Phase 8
## The working loop

## Where this sits

Phases 0–5 are the original plan. Phase 6 (*Make the tracker real*) shipped on 2026-08-11; Phase 7 (*Clean up the footprint*) closed alongside it. This document is one of three that follow:

| | |
|---|---|
| **Phase 8 — The working loop** | verbs for doing work, filing issues from the app, and the app made legible |
| **Phase 9 — One database** | DuckDB + Lance → SQLite |
| **Phase 10 — Release, distribution and install** | one pasted line, nothing compiles |

Written 2026-08-11 for KB, in plain English. Where a term from the codebase appears it is explained the first time.

**Goal:** KB files what he notices, Claude picks up what is ready, and both of them can see which is which.

---

### What Phases 6 and 7 already delivered

So that none of it gets rebuilt. **Phase 6:** routing and URLs, one page shell, a design system, the Cmd-K palette, the task detail view, readable identifiers (`KEEL-42`), a list view beside the board with grouping and sorting, composable filters in the URL, search results that navigate, task rank, sub-tasks via a parent column, one definition of blocked, `closed_at` written and backfilled, STATUS.md made safe to render, plain English in the interface. **Phase 7:** one authority per instruction, the broken mirror hook deleted, the tool surface tidied, the local API and the MCP tools made one thing, protocol honesty, the gate frozen into a single retrospective, and the document reset — twenty files to nine, seventeen open questions to five.

The task model now carries `number`, `parent_id`, `rank`, `closed_at` and `external_refs`. The app is 5,285 lines and has tests.

---

## 8A — The three verbs

The highest-value item left.

### `keel ready`

What can be worked on right now: open, not blocked by anything live, parents excluded because their children are the real work. The ranking already exists and is good — it orders by how many other tasks a task unblocks, on the reasoning that a p1 releasing three things moves the project further than a p0 releasing nothing.

It has never had a front door. Today it is reachable only inside a 3,500-token digest. It becomes a CLI command, an MCP tool, and a view in the app, all reading one computation. Filters: `--unclaimed`, `--label`, `--no-label`, `--limit`.

### `keel claim KEEL-42`

Atomic. Sets the task in progress, records the claiming session and the time. This replaces an instruction with a mechanism. Claiming an already-claimed task fails unless the claim is stale — reusing the three-day threshold `fsck` already warns on — or unless forced. Closing releases it.

### `keel close KEEL-42 --done --message "…" --commit <sha>`

Closing requires a reason; `done` additionally requires a message and at least one piece of evidence.

| Reason | Means |
|---|---|
| `done` | finished; message and evidence required |
| `wont_do` | deliberately not doing it; message required |
| `duplicate` | of another task; writes a `duplicates` link automatically |
| `superseded` | by another task; writes a `supersedes` link automatically |
| `no_change` | looked at it, nothing needed doing; message required |

Evidence is typed and repeatable: `commit:<sha>`, `pr:<url>`, `test:<command>`, `doc:<entity-id>`, `url:<url>`, `image:<blob-id>`.

Keel's definition of done is currently a seven-item checklist written as prose in `product/CLAUDE.md` that an agent is asked to honour. That is a convention. Making evidence an argument of the transition, enforced in the storage layer so the CLI and MCP cannot diverge, makes it an invariant.

### A `triage` status

One new value ahead of `todo`. Issues filed from the app land there; it means "a human threw this in, unrefined"; `ready` excludes it. This is what makes the loop work — you file rough, Claude sorts. Without it everything you file competes with planned work on equal footing.

### The tool count

Ten tools becomes twelve. The standing rule says an eleventh needs an argument at least as good as the one that earned `keel_note` the tenth slot. Here it is: these three are the only actions in the product that constitute *doing the work* rather than describing it, they map to three unambiguous non-overlapping intents, and the alternative leaves "what should I do next" reachable only by paying for a full digest. **A decision for you** — it touches the tool surface.

---

## 8B — Filing issues from the app

The one write path, deliberately fenced.

**What it does.** A **New issue** button, reachable anywhere with `C`. Title, markdown body, optional labels and priority. **Paste or drop images.** **Paste URLs**, which become typed entries in `external_refs`, now that it is a list. Creates a task in `triage`.

**What it cannot do.** Change a status. Change priority after filing. Close anything. Edit an existing task. Write a spec, decision or question. Delete. The app is a capture surface; Claude is the management surface. That boundary belongs in the endpoint's own doc comment so nobody widens it by accident later.

**How.** One mutating route, `POST /api/intake`, multipart, attributed `actor: human`, `surface: ui`. The second non-GET route on the API after `/api/generate`.

### The image problem, which is real and not obvious

Design images arrive as base64 in the tool call, capped at 1 MB decoded, oversized ones refused with their actual size. The refusal behaviour is good. The cap is about ten times higher than anything reachable.

Base64 inflates by a third, and the **model** has to emit those characters as tool arguments:

| Image | Base64 characters | Tokens the model must emit |
|---|---|---|
| 100 KB | ~133 KB | ~35–45,000 |
| 500 KB | ~665 KB | ~175–220,000 |
| 1 MB | ~1.33 MB | ~350–450,000 |

A retina screenshot is 300 KB to 2 MB. So the stated ceiling describes something no session can reach and the useful ceiling is nearer 100 KB — a small mockup, not a screenshot. This is not an implementation flaw; it is a property of passing bytes through a language model. MCP offers no alternative: tool *inputs* are JSON only, there is no binary input type, and the proposal to add one is still an open pull request.

### Three paths in

| Surface | How the bytes get in |
|---|---|
| **Desktop app** | Paste or drop. Straight to the daemon, no model involved. **The primary path, and the only one with no size problem.** |
| **Claude Code** | `keel_attach(id, path)` — the daemon reads the file itself. Same machine, so bytes never enter the model's context. |
| **Any surface, with a URL** | `keel_attach(id, url)` — the daemon fetches, behind a size cap and a flag that is off by default, since the daemon otherwise makes no outbound calls. |

The base64 path stays for genuinely small images, with its description corrected to state the real ceiling and point at the alternatives. `/api/blob/{id}` already exists, so reading them back is done.

This closes TQ-6, open since Phase 0, which is what blocks the design work in Phase 4.

---

## 8C — Make the app legible

Eight pieces. All confirmed against the code; each names where it lives.

**Three attachments accompany this section**, and they are the design rather than a description of it:

| File | What it is |
|---|---|
| `keel-design-system.html` | Typeface and scale, colour in both schemes, status semantics, space, radius, focus, every primitive, and a density comparison. Has a working three-way theme control. |
| `keel-screens.html` | The five reworked screens — board, task, library, what changed, search — switchable, in both themes. |
| `keel-tokens.css` | The token layer. Drops straight into `apps/desktop/src/styles.css`. |

Both HTML files are self-contained: Geist and Geist Mono are embedded as base64, so they render identically with no network. All of it is also pushed to a **Keel** design-system project in Claude Design.

### C0 — The design pass

The existing system got the hard part right. KEEL-73 delivered OKLCH tokens, a named six-step type scale, and centralised status colours — that work stands and most of it is untouched here. What still made the app read as a template was everything left unnamed, plus the font.

Six decisions:

1. **A real typeface.** Geist, self-hosted, SIL Open Font License. The system stack is the loudest possible signal that nobody chose anything, and Geist has true tabular figures — which matters when half the app is identifiers, counts and timestamps sitting in columns. Deliberately not Inter, which has become the sound of a default.
2. **Sans for language, mono for machines.** Identifiers, counts, timestamps, versions, keycaps and diffs are all mono. It is a system rule rather than a flourish, and it is most of what makes a tool read as an instrument rather than a website.
3. **A fourth, sunken surface.** The navigation rail sits *below* the page instead of beside it. Depth with no shadow at all — shadows on near-black are mud.
4. **An identity colour that is never semantic.** Brass, used for the wordmark and the ready indicator and nowhere else. A palette in which every colour means something is a scheme; one with a signature is a brand.
5. **Accent means "here", nowhere else.** Current nav item, focus, links, rank, and `in_progress` — which is genuinely "here". A blue used for every button stops pointing at anything.
6. **Named space, radius and focus.** Eight spacing steps, three radii by role, one focus treatment on `:focus-visible` only. Exactly what KEEL-73 did for type, applied to everything still being chosen by feel.

**And a theme the user chooses.** Today there is only a `prefers-color-scheme` block, so the operating system decides and there is no control anywhere in the app. `data-theme` on `<html>` takes three values — system, light, dark — with a segmented control in the rail.

The light scheme is also restructured rather than re-tuned. It was a second copy of the palette inside a media query, which is exactly how three status colours came to be missing from it. Every pair is now declared once with `light-dark()`, so a token cannot exist in one scheme and be absent from the other.

### C1 — Navigation is inside out

**The problem.** `App.tsx` lists eight screens at the top of the sidebar, and the project list *underneath* them. Five of those eight need a project, so on launch they render at 35% opacity with "Pick a project first" and refuse clicks. The one control that would fix that — the project list — is below them, and it is a flat list that grows with every project.

So the order of operations is inverted: you arrive, five of eight items are dead, you scroll past them to the bottom, choose a project, then come back up. That is exactly the friction you described, and it gets worse as projects accumulate.

**The fix — project first.**

- A **project switcher at the top**, under the Keel wordmark: the current project's name, clicking it opens the same list (or hands off to Cmd-K, which already indexes projects). One row instead of N.
- Beneath it, **that project's screens** — Overview, Board, Roadmap, Library, Metrics. Always live, because a project is always selected.
- Below a divider, the **global** things — All projects, Search, and What changed. These are the only ones that mean anything without a project.
- **Remember the last project** in local storage and select it on launch, so the disabled state stops existing rather than being styled.

This is the shape Linear uses and it is not an aesthetic preference: it removes an entire class of dead control from the first screen you see.

The number-key shortcuts stay, renumbered to follow the new order.

### C2 — The Library: one screen per kind of thing

**The problem.** `Documents.tsx` fetches `spec,decision,question,feedback,design` into one flat sidebar list, each row a type badge plus a status badge plus a title. Five genuinely different kinds of artifact, rendered identically, in creation order. With forty-four decisions and five questions in the store, a decision looks exactly like a spec looks exactly like a design, and there is no way to see the shape of any of them.

They are different things and they are read differently:

| Kind | How you actually read it | Right shape |
|---|---|---|
| **Spec** | long prose, whole, versioned, diffed | the current reader — which is good |
| **Decision** | a register you scan or look up by number; you want to see what superseded what | **a table**: number, title, status, when, supersedes |
| **Question** | open ones first; the answer matters as much as the question | **grouped by open/answered**, answer inline |
| **Feedback** | who said it, when, how they felt | chronological cards with source and sentiment |
| **Design** | you look at the picture | **a thumbnail grid**, not a list of titles |

**The fix.** Keep one screen — call it **Library** — with a type switcher across the top and a layout per type. The document reader stays exactly as it is for specs, and remains the destination when you click a row in any of the other views, so nothing is lost and revision history and diff still work everywhere.

Two things this buys immediately: the decision register becomes something you can actually scan, which is the whole point of having numbered it in Phase 7; and design images stop being invisible text rows.

Add a **filter box in the sidebar** (see C6) — with ~60 documents there is currently no way to narrow the list at all.

### C3 — Time, everywhere

**The problem.** There is a good relative-time helper, `when()` in `ui.tsx`, and three screens do not use it. `Roadmap.tsx:93` renders `shipped ${new Date(shipped).toLocaleDateString()}` → **"shipped 8/9/2026"**, which is the one you saw. `Metrics.tsx:212` does the same for the latest observation.

There are also three real defects in `when()` itself:

- **It cannot express the future.** Milestone target dates are future dates; `when()` subtracts and would render "-3d ago". The roadmap is the one screen most likely to hit this.
- **It drops the year** past seven days — "Aug 9" is ambiguous the moment this project is more than a year old.
- **There is no precise value available.** For something that shipped four minutes ago, "4m ago" is right; but sometimes you want the timestamp, and only the Documents revision menu offers one.

**The fix.**

- One helper, used on every screen. No screen formats a date itself.
- Handle both directions: `in 3 days`, `tomorrow`, `just now`, `4m ago`, `2h ago`, `yesterday`, `3d ago`, then an absolute date with the year when it is not this year.
- The exact timestamp always available on hover, via the `Tooltip` primitive that already exists.
- Roadmap milestones read "shipped 2h ago" and "due in 3 days" rather than a slash-separated date, which is what makes a roadmap that moves in hours legible.

### C4 — Activity: keep it, but make it about sessions

**What it is today.** A reverse-chronological feed of every mutation, up to 300, filterable by actor. Each row is a timestamp, an actor badge, a one-line summary, and a session id. The rows are not links.

**Is it valuable?** As built, barely. It is a firehose of "created task X", "status todo → done" with no grouping, no way to reach the thing that changed, and no time range. Its own header says its purpose is "what did Claude do today", and it does not answer that — it answers "what were the last 300 events". And now that per-task history lives on the task detail view, the per-entity case is served better elsewhere.

**But the job is real, and nothing else does it.** You leave, Claude works, you come back. "What happened while I was away" is the single most valuable question this app could answer for someone in your position, and right now there is no screen for it.

**The fix — rename it "What changed" and group by session.**

A session is the unit of work, and `session_id` is already threaded through every write. So instead of 300 flat rows:

> **Claude · 2 hours ago · 14 changes**
> closed KEEL-93, opened KEEL-97, wrote 3 notes, answered TQ-29
> *(expand for the full list)*

- **Group by session**, newest first, with a one-line summary of what the session did.
- **Every row links** to the entity it changed. Today they are dead text.
- **A "new since you were last here" marker**, from a timestamp in local storage.
- **Time range**: today, this week, everything. Keep the actor filter.
- Drop the `written outside a tracked session` treatment from the main row into the tooltip. That is a build-time concern being shown as product copy — the same class of thing KEEL-85 cleaned up elsewhere.

If you would rather not spend the time: **delete the screen**. Per-task history is on the task now, and a half-useful firehose is worse than nothing. My recommendation is to keep and rework it, because the returning-user question is genuinely unanswered — but it is a close call and either answer is defensible.

### C5 — Search that knows where it is

**The problem.** The search box placeholder reads *"Ask a question — 'why is billing slow', 'what did customers say about onboarding'"*, and the empty state repeats the billing example. Neither has anything to do with Keel. Both were lifted from the MCP tool description, which is written for a generic project — the same build-time-copy-as-product-copy pattern KEEL-85 fixed elsewhere, surviving in the one screen whose entire job is to invite a question.

**The fix — starter queries built from the project's own content.** The data is already there: the digest knows the project's open questions, its recent decisions and its glossary terms. Show three or four clickable chips drawn from them:

- an open question, verbatim — *"How does a design image get into Keel from a chat session?"*
- a recent decision framed as a why — *"why did we choose DuckDB over SQLite"*
- a glossary term — *"what is a mirror path"*

That is honest, specific, and teaches what semantic search is for using material the reader recognises. Fall back to generic prose only when a project is genuinely empty.

Two smaller things while in there: the eleven type facets are rendered as eleven equal chips, which is a lot of identical weight for something used occasionally — the `Menu` primitive already used for scope would collapse them. And pressing `/` from the Board should land in search **scoped to this project and filtered to tasks**, rather than everything.

### C6 — Find-in-place, and how it differs from Search

**Where things stand.** The Board has a text filter plus status, priority and label menus — that part works. The Library sidebar has **no filter at all**. And Cmd-K already does title matching across projects, documents and tasks, which is a genuinely good jump-to that most people will never discover.

**The fix.**

- **A filter box in the Library sidebar**, matching the Board's, filtering titles within the selected type.
- **Make the distinction visible in the words.** The board's box says `Filter…`, which does not say what it filters or how it differs from the Search screen. Better: *"Filter these 94 tasks"* on the board, *"Filter 62 documents"* in the library — a count tells you the scope, and the verb tells you it is narrowing what is already on screen rather than searching the store.
- **Surface the palette.** There is a "Jump to…" button at the bottom of the sidebar; with the navigation rework in C1 it belongs at the top, next to the project switcher, where it reads as the search affordance it is.

The rule to hold: **Filter narrows what is in front of you and is instant and literal. Search queries the whole store and understands meaning.** Two different jobs, and the interface should never make you guess which one you are using.

---

### C7 — The milestone is missing from the one place you look

**The problem.** A task carries `milestone_id`, the Roadmap screen renders milestones, and the board shows neither. A card gives you identifier, title, priority, kind and labels — so the single most important organising question, *what is this part of*, is answerable only by opening the task or by going to a different screen.

That is backwards for this project. Phases are how the work is actually structured: every task in the store belongs to one, the tracker groups by them, and "what is left in Phase 8" is a question asked far more often than "what is left at p1".

**The fix.**

- **On every row**, in the right-hand gutter beside status — a quiet mono chip, `Phase 8`. Rendered **dashed and faded when a task has no milestone**, so an unassigned task is visibly unassigned rather than silently indistinguishable. That gap is information: it usually means something was filed and never placed.
- **Group by milestone**, as a first-class option beside status and priority, with a done-count per group so a phase reads as `Phase 8 · 4 of 15`.
- **Filter by milestone**, and the chip is the control — click `Phase 8` on any row to see only that phase.
- **In `keel ready`**, so "what is next in Phase 8" is one flag rather than a mental filter.
- **Both directions between Board and Roadmap.** A milestone on the roadmap links to its tasks; a chip on a task links to its milestone. They describe the same thing and currently do not know about each other.

## 8E — Two small things

**Rate limiting on `/mcp`.** There is none — no `governor` or equivalent anywhere in the workspace. Cheap insurance against an agent in a loop, and the MCP specification lists it under what servers **must** do.

**The "ask Claude" affordance.** Since the app cannot write, each task detail carries a few copy-ready prompts — *"close KEEL-42 as done with the commit"*, *"what is blocking KEEL-42"*, *"split KEEL-42 into sub-tasks"*. One click copies; you paste into Claude Code. This is what makes read-only feel deliberate rather than inert, and it is nearly free now the detail view exists.

---

## 8F — The project's own words

You asked what happens if a session calls it a *milestone* when this project says *phase* — or an epic, a sprint, a release, a cycle. Today: nothing good. `keel_create(type: "phase")` fails with an enum error listing thirteen types, none of which is the word the project actually uses on every screen.

Three layers fix it, and none of them adds a type.

**1. A display noun, per project.** The project records what it calls a milestone — `milestone_noun: "Phase"` — and the interface, the renderer and the digest all use that word. The board says *Phase 8*, `render-status` writes *Phases*, and `keel_context` tells the session "this project calls milestones **phases**" in its first paragraph. The concept stays one thing; only the label is the project's.

**2. Aliases on input.** A small resolution table applied before validation:

| Said | Resolves to |
|---|---|
| phase, epic, sprint, cycle, iteration, release, version | `milestone` |
| issue, ticket, story, bug, chore, defect | `task` |
| adr, rfc, choice | `decision` |
| risk, unknown, open question | `question` |
| requirement, design doc, prd | `spec` |

The resolution is **reported, not silent**: the response says `created milestone "Phase 9" — this project calls those phases`. A silent success teaches the session nothing and it makes the same guess next time; a narrated one teaches it the project's vocabulary in a single round trip.

**The rule that keeps this safe: an alias is a spelling, not a concept.** It must never create a fourteenth type. Thirteen is the ceiling and this is precisely the pressure that would erode it — "sprint isn't quite a milestone" is how a schema grows a type it cannot later remove.

**3. The glossary already does most of this and nobody wired it up.** `term` is one of the thirteen types, and glossary terms are the one thing `keel_context` is forbidden from truncating — the reasoning being that a missing term makes an agent use the wrong word for a domain concept, which is exactly this problem. So the project's vocabulary is already delivered to every session; the milestone noun should simply be seeded as a term when a project is created, and the alias table should consult the glossary before falling back to the built-in list.

That third point generalises past milestones. A project that says *customer feedback* rather than *feedback*, or *incident* rather than *question*, gets the same treatment for free.

---

## 8G — Every task readable by a human, by default

You want this to be something Claude Code simply does, not something a check catches afterwards. Agreed — and the distinction that matters is *where* the enforcement sits, because there are four candidate places and only some of them act before anything is stored.

**Start from what this project already measured.** Across thirty gate sessions the skill was never once invoked — the model does not load a skill on ordinary engineering prompts. That is why the SessionStart hook exists and why it tripled the write rate. So putting the house style only in the skill would be repeating a failure this project has already paid to discover.

The other piece of evidence points the way: when a write failed validation — two sessions sent `priority: "high"` against an enum of p0–p3 — **both retried successfully in the same turn.** An actionable rejection at the tool boundary is not an after-the-fact check. It happens before the row exists, and the model fixes it and moves on without a human involved.

So, four layers, ordered by when they act:

**1. The tool schema — this is the mechanism.** `summary` is a required property on `keel_create` for tasks. A model physically cannot complete the call without confronting it, on every surface, in every session, whether or not any skill loaded. The description carries the contract and one worked pair:

> One or two plain sentences a colleague could read cold, six weeks from now, without having been in this conversation. Say what is wrong or wanted, who or what it affects, and what "done" looks like.
>
> **Good:** "The board shows a task's priority and labels but never which phase it belongs to, so you have to open each task to find out. Done when every row shows its milestone and you can group by it."
>
> **Bad:** "Implement milestone surfacing on the board view per TQ-31 to improve organisational legibility."

The descriptions already say *when to reach for this* rather than restating the signature, and a test enforces that — so this is the established pattern, not a new one.

**2. Synchronous rejection, in the storage layer**, so the CLI and MCP cannot diverge. The row never exists in a bad state. What is genuinely detectable:

- empty, or a restatement of the title — the containment rule already written for near-duplicate titles, pointed at a different pair of strings
- a bare `TQ-15`, `B-44`, `REQ-7`, `KEEL-96` with no gloss beside it; the `unresolved_id_reference` parser already recognises these families
- a short, maintainable banned-phrase list — *leverage, utilize, robust, seamless, delve, in order to, it's worth noting, this task, this change*
- `snake_case` or `SCREAMING_CASE` that is neither a glossary term nor explained in the sentence

Each rejection names what was wrong and what would be valid, which is the standard the MCP surface already holds itself to.

**3. The SessionStart hook states the house rule in one line**, because it is the channel that demonstrably reaches the model in Claude Code. Not a paragraph — one sentence, next to the existing orientation text. The skill carries the longer version for sessions that do load it.

**4. `keel lint` is a backfill, not the mechanism.** Roughly ninety rows already exist without summaries and something has to look at them once. It **reports and never rewrites** — a machine inventing a summary would produce exactly the confident, plausible, wrong prose this section exists to prevent, which is the same reasoning that stops the mirror ever reading a file back.

**And one rendering rule that keeps the requirement felt.** Lists show the summary, never the body. A task with no summary shows a visible gap rather than falling back to the first line of the body, because a silent fallback is how a requirement quietly stops being one.

**The honest limitation, unchanged.** A validator catches structure, not quality. It cannot tell a good sentence from a bad one and never will. What the tool boundary does is put the standard in front of the model at the moment of writing, every time, with no dependence on anyone remembering — and make a bad summary cost one retry then rather than an unreadable row six weeks later.

## Phase 8 exit criteria

- `keel ready` answers in one command, one tool call, and one click.
- A task cannot reach `done` without a reason, a message and evidence — enforced at the storage layer, with a test proving both the CLI and MCP paths refuse.
- Filing a bug with a pasted screenshot, from a cold start, takes **under 30 seconds** — measured with a stopwatch, by you.
- On launch, **no navigation item is disabled**.
- The app uses a self-hosted typeface, and the theme is switchable in the rail between system, light and dark.
- No colour token exists in one scheme and not the other — guaranteed by `light-dark()` rather than by review.
- Every one of the five artifact kinds in the Library has a layout that suits it, and the decision register is scannable as a table.
- No screen formats a date itself; the roadmap reads "shipped 2h ago" and "due in 3 days".
- Search offers starter queries drawn from the project's own questions, decisions and glossary.
- Both the board and the library have a filter box that names its scope and count.
- Every task row shows what it is part of, and an unassigned one shows that it is unassigned.
- `keel_create(type: "phase")` succeeds and says what this project calls those.
- **No task can be created without a summary** — proved by a test asserting both the CLI and MCP paths refuse, and by a session that supplies a bad one and recovers in the same turn.
- `keel lint` reports zero unexpanded identifiers across the rows that already exist.

---

---

## What needs your decision before this starts

1. **Twelve MCP tools instead of ten** — §8A. Touches the tool surface, which the standing rules say to bring to KB.
2. **A `triage` status** — a seventh value on the task status enum. §8A.
3. **Activity: rework or delete** — §C4. I lean rework, and it is close.
4. **Whether `keel_attach` may fetch a URL at all** — recommended off by default. §8B.
5. **A `summary` field on tasks, required and rejected at the tool boundary** — §8G. A schema change plus a write-path refusal.

## Order within the phase

**8C** the app made legible → **8A** the verbs and triage → **8G** readable summaries → **8B** intake and attachments → **8F** the project's own words → **8E** the small things.

8C leads because it is what KB looks at every day and none of it depends on anything else. 8G sits early for a compounding reason: every task written after it lands is one that never needs fixing, and the rows that already exist can be linted in the background while that is true.

## What Phase 8 does not do

- **No writes from the app beyond filing an issue.** No status changes, no drag-and-drop, no inline editing, no comments.
- **No assignees, no multi-user.** There is no person model and none is being added.
- **No sprints, points, velocity or burndown.**
- **No new artifact types.** Thirteen remains the ceiling — and §8F's aliases must never become a fourteenth.
- **No Cowork integration.** Dropped. One finding is worth keeping so nobody rediscovers it: a Cowork session cannot reach `127.0.0.1:7654`, but locally-registered MCP servers *are* proxied into Cowork, verified live. If it is ever wanted, it is an installer change, not hosting.
- **No further gate runs.**

---

## Appendix — what this rests on

| Claim | Source |
|---|---|
| Five artifact kinds in one flat list | `screens/Documents.tsx` — `PROSE_TYPES = "spec,decision,question,feedback,design"`, one `<ul>` |
| Five of eight nav items disabled without a project | `App.tsx` — `NEEDS_PROJECT`, `opacity-35`, "Pick a project first" |
| The project list sits below the screens | `App.tsx` — the Projects block follows the SCREENS block |
| "shipped 8/9/2026" | `screens/Roadmap.tsx:93` — `toLocaleDateString()`, bypassing `when()` |
| `when()` cannot express the future | `components/ui.tsx:538` — subtracts, with no negative branch |
| Activity rows are not links | `screens/Activity.tsx` — `<li>` with no anchor |
| The billing placeholder | `screens/Search.tsx` — placeholder and empty-state hint, lifted from the MCP tool description |
| The Library has no filter box | `screens/Documents.tsx` — the `<aside>` renders the list unfiltered |
| Milestone absent from the board | `screens/Board.tsx` renders priority, kind and labels; `milestone_id` is never read |
| The app is read-only | one `fetch()` in `lib/api.ts`, no init object |
| No rate limiting | no `governor` or equivalent anywhere in the workspace |
| Skills do not fire unprompted | 30 gate sessions, zero Skill invocations — which is why the SessionStart hook exists |
| Validation errors are recovered from | two gate sessions sent `priority: "high"` against an enum of p0–p3; both retried successfully in the same turn |
| Zero transitions into `in_progress` | across 66 tasks, before the hook was asked to prompt for it |
| Base64 image cost | 4/3 inflation; 1 MB decoded is ~350–450k tokens for the model to emit |
| MCP binary tool inputs | not in the specification; the proposal is an open pull request |

---

## Design attachments

Three files accompany §8C. They are the design rather than a description of it.

| File | What it is |
|---|---|
| `keel-design-system.html` | Typeface and scale, colour in both schemes, status semantics, space, radius, focus, every primitive, density. Working three-way theme control. |
| `keel-screens.html` | The five reworked screens — board, task, library, what changed, search — switchable, in both themes. |
| `keel-tokens.css` | The token layer. Drops into `apps/desktop/src/styles.css`. |
| `Geist-Variable.woff2`, `GeistMono-Variable.woff2` | Self-hosted, into `apps/desktop/public/fonts/`. |

Both HTML files embed the fonts as base64 and render with no network. The same bundle is in the **Keel** project in Claude Design.
