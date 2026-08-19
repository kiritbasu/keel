<!-- specline:generated spec spc_01M0CMDKDPWZ0CS317SEPXTDVS v1 2026-08-19T09:15:18Z
     source of truth is Specline — edits here are not saved -->
# How feature requests should work, end to end

**Status:** `draft`  
**Kind:** `spec`  
**Id:** `spc_01M0CMDKDPWZ0CS317SEPXTDVS`

**Draft. Nothing here is agreed** — it is the proposal KB asked for on 2026-08-19, written down so it survives the conversation. Status stays `draft` until he picks a direction.

## The problem, stated properly

KB filed four rows from the app in six minutes on 2026-08-18 (KEEL-302, 303, 305, 306) and then said the rows were done in a hurry and the real thought is bigger: *"feature request ideas can come from many sources, I could think them up or they can come from conversations with friends or from customers, and the point is the entire lifecycle of how to manage feature requests is basically missing for us."*

That is the right diagnosis and it is worth being precise about how much is missing. A feature request has six stages:

1. **Capture** — something arrives, from KB's own head, a friend, a customer, a support thread, a competitor.
2. **Develop** — the one-liner becomes a thought: what problem, whose, what happens if we do nothing.
3. **Decide** — in, out, later, duplicate, already-settled.
4. **Shape** — becomes a spec and a set of tasks.
5. **Build** — the work.
6. **Close the loop** — whoever asked finds out what happened.

**Specline has stage 5 and nothing else.** Every other stage currently collapses into "type a task into the board and hope".

That is a sharper indictment than it looks, because Specline's entire pitch is that the reasoning is the product and the tracker is the easy part. A feature request is precisely the artifact where the reasoning happens *before* any work exists — and it is the one place the product has no home for it.

## What is missing that KB did not file

His four rows name board clutter, decomposition, triage and volume. Reading the schema against the six stages turns up more, and these are the ones that argue for a lifecycle rather than a checkbox:

- **No way to record who asked.** Madhu appears in a parenthesis inside a title on KEEL-306. `feedback.source` and `feedback.contact` exist and have never been written to.
- **No way to say no, durably.** Closing a task `wont_do` leaves a `close_message` on a row nobody will ever look at again. The same idea arriving in four months finds nothing.
- **No dedupe.** Two people asking for the same thing produce two rows forever. Hybrid search over the whole store makes this tractable and nothing calls it.
- **No demand signal.** How many people asked, how recently, how often — there is no way to tell an idea one person mentioned once from one five people keep raising.
- **Nobody ever hears back.** Madhu will not learn what happened to codex support, which is the stage that decides whether he tells you the next thing.
- **The open count is lying.** The digest says "37 open tasks". Four of those are ideas, not work. A count that mixes committed work with unexamined ideas cannot be used to answer "how much is left", which is the only question it is for.
- **No expiry.** An idea from six months ago that nobody has mentioned since sits at `p2` next to release work, indefinitely.
- **The tool forces solutions, not problems.** Three of KB's own four rows are phrased as solutions — "add an option for feature request", "set up some sort of feature where you can slice and dice", "this should work with codex". The problem behind each is nowhere, because the only container available was a task, and a task is a unit of work, so it asks you what to *do*. KEEL-303 is the exception and it is noticeably the most useful of the four.

## The shape

**Four stages, four artifacts, and every one of them already exists.**

| Stage | Artifact | Status today |
|---|---|---|
| Something someone said | `feedback` — verbatim, sourced, dated, never edited | Table exists. Zero rows ever written. |
| The developed idea | `spec` with `kind = 'feature'` — the why, revisioned, Claude-authored | Specs exist; needs one enum value. |
| In or out | `decision` — including `rejected`, with the argument | Exists, unused for this. |
| The work | `task` with `kind = 'feature'` as the epic, children via `parent_id` | Needs one enum value; `parent_id` exists and nothing uses it. |

The connecting edges are all in SPEC.md §3.3 already: `informs` (feedback → spec), `derived_from` (spec → feedback), `implements` (task → spec), `resolves` (decision → question), `duplicates` (task → task).

**So the gap is not tables. It is that nothing connects them and no surface makes the path walkable.** That matters for sizing: this looks like a large feature and is mostly wiring plus two enum values.

### The one structural decision: separate the thinking from the container

A feature is two things with different lifetimes and different authors, and conflating them is what makes epics miserable in every other tracker:

- **The feature spec** holds the why. Written by Claude, in the conversation where the thinking happened. Revisioned. It exists whether or not the thing is ever built, and it is what a session reads to understand why child task nine matters.
- **The epic task** (`kind='feature'`, children by `parent_id`, has `milestone_id`, `status`, `priority`, `rank`) is the unit of work. It is created **at the moment of the decision to build**, not before.

This resolves five things at once:

1. **Hard constraint 7 holds.** The app creates the epic — a person's own action, `actor: human`, `surface: ui`. Claude writes the spec. The line between capture and authoring stays exactly where the contract puts it.
2. **Ideas we never build never touch the board.** They are feedback, specs and decisions. KEEL-305's board-clutter complaint is fixed as a side effect rather than by a filter.
3. **KB's containment works.** "A milestone might contain features (epics), improvements, bug fixes" — epic tasks and loose tasks both carry `milestone_id`, so a phase holds both, and an epic holds its children. Making a feature a `milestone` instead would break this, because milestones do not nest.
4. **No new artifact types.** Thirteen is the ceiling and this stays under it: `tasks.kind` gains `feature`, `specs.kind` gains `feature`. Both are vocabulary changes, which still want KB's nod.
5. **The rollup is free.** "3 of 8 done" is a count of children, not a stored field that can drift.

Objection worth answering: *does this mean two objects per feature?* No — KB creates one thing, an inbox entry. The spec is written during the conversation that develops it. The epic appears at the moment he says yes, with its decomposition already proposed. From his side it is one capture and one "yes".

### Capture must stay six seconds long

The whole design fails if filing an idea requires choosing between feedback, spec and task. **Capture is always `feedback`.** One box, one keystroke, no type picker. `kind` defaults to `idea`; naming a source is optional and is what distinguishes Madhu's request from KB's own. It carries the `triaged` flag, which is the inbox. It never appears on the board.

The word is wrong, though: calling your own idea "feedback" reads badly. The table name need never be shown — the surface can be called Inbox, or Signals, or Requests. That is an open question rather than a detail.

## Why this is not a ticketing system

KB asked for this explicitly, and it is the part worth getting right, because the temptation is to copy Jira's epic and stop.

In a conventional tracker every one of the six stages is human labour, so the tool optimises for making each click cheap. Here, stages 2, 3 and 6 are **reading and writing at volume** — which is what a model is good at and a person is bad at. So the design principle is: *the human supplies judgement at exactly two moments — is this in or out, and is this the right decomposition — and everything else is proposed rather than performed.*

Six concrete differences:

1. **The inbox is a conversation, not a form.** "Madhu wants codex support" said in a session lands sourced and dated without opening the app. The app is for when you are not in a session.
2. **Triage is proposed, not performed.** A session reads the untriaged pile, clusters it, checks each item against every decision ever made, and returns *"these six are one idea; this one contradicts B-47; this is already KEEL-289"*. KB answers yes or no. This is KEEL-302 and KEEL-303, and it is the half a tracker structurally cannot do.
3. **Decomposition is proposed, not typed.** "Break this up" produces an argued task tree to edit. Old trackers make you type eight tickets and then wonder why nobody writes epics.
4. **Rejections are as durable as acceptances.** A "no" becomes a decision with the argument in it, so the same idea arriving in four months finds the reasoning instead of silence. In Jira a closed ticket is a tombstone; here it is a retrievable argument. This is the single strongest thing Specline can claim over every competitor in the category.
5. **The pile talks to you.** KEEL-303's "periodically points that out to you" is not a new feature — the digest already reaches every session at start. It can say *"17 untriaged, oldest 40 days, 3 look like duplicates"*. That is a rendering change.
6. **Provenance is already free.** Every write carries actor, session and surface, so "who asked for this, when, from where" needs no new columns.

## What to build, in order

**A — Capture and the inbox.** Write to `feedback` for the first time. An inbox surface in the app and a way to file one from a session. Untriaged items excluded from `specline_next`, from the board, and counted separately in the digest. Backfill KEEL-302/303/305/306 into it.
*This alone fixes the pain KB actually feels today, and it is the smallest piece.*

**B — Develop and decide.** `specs.kind = 'feature'`. Triage moves an inbox item to a feature spec (accept), a decision (reject, with the argument), or a link to the thing that already covers it (duplicate). `informs` and `derived_from` edges drawn on the way through.

**C — Shape into work.** `tasks.kind = 'feature'` as the epic, `parent_id` for children, `implements` to the spec. Epic display on the board and child rollup. Proposed decomposition as an MCP-side operation.

**D — The pile, and closing the loop.** Clustering and dedupe against the whole store. Digest section for the untriaged backlog with an age. Outcome visible on the feedback row so KB can go back to Madhu.

The customer stream is real but later — KB confirmed the pain today is his own ideas. The design accommodates it because `feedback.source` and `contact` are already there; nothing in A–D needs redoing when it arrives.

## What is deliberately not proposed

- **A `features` table.** Thirteen artifact types is a ceiling KB agreed to, and this needs nothing it would provide.
- **Voting, scoring, RICE, or any prioritisation formula.** One user. `specline_next` already ranks.
- **A public portal.** Not until there is a customer stream, and probably not then.
- **Reviving the `triage` status on tasks.** TQ-32 declined it, and this design makes it unnecessary — an untriaged idea is not a task at all, so it needs no status to hide behind. TQ-32 still needs superseding, because its stated reason ("app filing is declined") is no longer true.

## The cheap alternative, priced honestly

If the lifecycle is more than KB wants: add `feature-request` as a fifth `tasks.kind`, filter it out of `specline_next` and the open count, and stop. Three files in the app, one enum value, one migration — perhaps half a day, against several days for A–D.

It buys the board-clutter fix and nothing else. It does not record who asked, does not survive a "no", does not dedupe, and leaves every idea phrased as a solution. Worth saying plainly because it is a legitimate answer, and because "we already have a kind field" is the version of this that gets built by accident if nobody decides otherwise.

## Open questions for KB

1. What is the inbox called on screen? "Feedback" is the table and reads wrong for your own idea.
2. Does an epic need a progress rollup on the board, or is the child list enough?
3. Should every rejection become a numbered decision, or only the ones worth remembering? Numbering every no would flood the decision log.
4. Does the app get a file-an-idea button, or is capture session-only? Constraint 7 permits the button.
5. Two enum values — `tasks.kind += feature`, `specs.kind += feature`. Under the thirteen-type ceiling, but vocabulary, so yours to agree.

