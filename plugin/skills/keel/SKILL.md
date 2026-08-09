---
name: keel
description: Use for any conversation about an ongoing software project — specs, decisions, tasks, roadmap, bugs, customer feedback, open questions, what shipped. Read from Keel at the start of such a conversation and write to it whenever something is decided, planned, learned, or asked and left unanswered. Triggers on "what's the state of", "what did we decide about", "add a task", "we should", "let's go with", "I spoke to a customer", "why did we", "what's blocking", or any mention of a project by name.
---

# Keel

Keel is where everything about a software project lives except the code: specs,
decisions, tasks, milestones, questions, risks, customer feedback, the glossary,
what is deployed.

You are the main way anything gets in or out. There is a desktop app, but it is
for reading. If you do not write to Keel, nothing does.

---

## You are already oriented

A `SessionStart` hook has put Keel's digest into this conversation before you
read anything — what the project is, the active milestone, what is urgent and
what is blocked, recent decisions, every open question, the glossary.

You did not have to ask for it and you do not have to fetch it. This exists
because relying on the model to load a skill did not work: across thirty
headless sessions and an interactive one, this file was never opened, so
everything in it was advice nobody read (TQ-19).

Two things follow:

- **Do not re-litigate what the digest already settles.** If a decision is
  listed, it is decided. If a question is open, it is open — do not answer it
  as though it were new.
- **Use the glossary's words.** The digest carries them because a project's
  vocabulary is the cheapest thing to get wrong and the most annoying.

Call `keel_context` yourself only when the conversation moves to a *different*
project, or when you need `depth: "full"` because the digest reported that it
trimmed something you need. Pass `cwd` when you do.

If the digest said no project matches this directory, read
"Before creating a project" below — the short version is that you create the
first one and say so.

---

## Thread a session id through every call

Mint one identifier at the start of the conversation and pass it as `session_id`
on **every** Keel call, read and write.

Use a ULID-shaped string with a `ses_` prefix — anything stable and unique is
fine, it does not have to mean anything. Generate it once, keep it for the whole
conversation, do not regenerate it.

```
session_id: "ses_01K7Q2X8N4RVBM3TZC9WPD5A6E"
```

This matters more than it looks. Keel's provenance guarantee is that every
change can be traced to the conversation that made it, and MCP has no protocol
session to borrow — so the identifier has to come from you. Keel will accept a
write without one rather than refuse it, but the change is then attributed only
to "some Claude session", which is nearly useless a month later when the human
is asking why something changed.

Also pass `surface`: `code` in Claude Code, `chat` in Claude chat, `cowork` in
Cowork.

`keel_context` echoes the `session_id` back. If it comes back `null`, you are
not threading it — fix that before writing anything else.

---

## Write when something becomes true

Not at the end of the conversation. Not when asked. When it happens.

| When the human… | Write |
|---|---|
| decides something, or agrees to an approach | a **decision** — with the context and what was rejected, not just the choice |
| describes work to be done | a **task** — or several, if it is genuinely several |
| asks something nobody can answer yet | a **question** — this is the one everyone forgets |
| worries that something might go wrong | a **question** with `kind: risk` |
| describes what to build, at length | a **spec** |
| relays what a customer said | **feedback** — verbatim in the body, not your summary of it |
| uses a domain word in a way you had to infer | a **term** — cheap to add, and it stops the next session guessing |
| says something shipped | update the **milestone**, and the **environment** if a version changed |

The two most valuable and most-skipped are **questions** and **decisions**.
A question that evaporates when the conversation ends gets re-asked in three
weeks. A decision without its reasoning gets re-argued.

### Record the reasoning, not just the outcome

"Chose DuckDB" is nearly worthless. What is worth writing:

> ## Context
> We need relational queries over mutable rows, semantic search over prose, and
> multimodal blobs.
>
> ## Decision
> DuckDB for entities, Lance for documents and blobs.
>
> ## Consequences
> Both are native Rust crates, so no sidecar process. Lance is young and is the
> one unhedged dependency, which is why the Parquet export exists.

In six months neither you nor the human will remember why. That paragraph is
the whole point of writing it down.

---

## Before creating a project, ask

**Always call `keel_projects` first.** It fuzzy-matches on name, slug, aliases
and repository URL.

If it comes back with `requires_confirmation: true` — meaning something that
*looks like* this project already exists — **stop and ask the human**:

> I don't see an exact match. There is "Harbour" (`harbour`) which looks close —
> is this the same thing, or should I create a new project called
> *Harbour Billing*?

Nine near-identical projects is the failure that quietly ruins the cross-project
view, and merging them afterwards is far more work than asking now.

### But do not stall on an empty store

**If nothing resembles it at all, create the project and get on with it.** Say
that you did, in one line, and carry on:

> Nothing in Keel matched this repository, so I've created the project
> **Tideline** and recorded the decision under it.

The rule above exists to stop you creating a *second* project for something that
already exists. Creating the *first* one for a directory Keel has never seen is
not that failure, and treating it as though it were has a cost that was measured
rather than guessed: in the ten-session gate, **nine sessions understood exactly
what should be recorded, said so, and wrote nothing** — because there was no
project to write into and they were waiting for permission that a working
session never pauses to give.

Pass `cwd` to `keel_context` and it will tell you outright whether any project
owns the directory you are in. "No project matches this directory" means create
one. It does not mean stop.

---

## Consolidate. Do not shred.

A project with forty trivial tasks that should be eight is worse than useless —
the human stops reading the list, and then stops trusting it.

- One task per meaningful unit of work, not per step you imagine.
- "Add the login page" is a task. "Create the file", "add the route", "write the
  test" are not — they are how you would do it.
- Long-form detail belongs in a **spec**, linked from the task with `implements`,
  not in twelve task bodies.
- If you find yourself creating a fifth task in one turn, stop and ask whether
  it is really one task with a spec behind it.

Creates are idempotent, so a retry is safe: calling twice with the same project,
type and title returns the existing artifact with `created: false` rather than
duplicating it. Capitalisation and spacing are normalised, so "Add login page"
and "add  Login  Page" are the same task.

---

## Link things, and get the direction right

Direction reads left to right: **`from` does the verb to `to`.**

| Say it like this | Not like this |
|---|---|
| task **implements** spec | spec implements task |
| blocker **blocks** the thing waiting | the waiting thing blocks its blocker |
| newer decision **supersedes** older | older supersedes newer |
| decision **resolves** question | question resolves decision |
| feedback **informs** spec | spec informs feedback |
| spec **derives from** feedback | feedback derives from spec |

If "A depends on B" is the natural way to say it, use `depends_on` — Keel stores
it the right way round and tells you it did.

Use `anchor` to link to one requirement inside a spec rather than the whole
document:

```
keel_link(from: task_id, rel: "implements", to: spec_id, anchor: "REQ-4")
```

That is what makes "is this spec actually built?" answerable requirement by
requirement instead of as a yes/no guess.

---

## Updating: pass the version you read

`keel_update` needs the `version` from when you read the artifact. `keel_get`
returns it at the top level of the entity, so it is a straight copy.

If someone else changed it in between, you get a 409 carrying the current state
and the events since your read. **Merge and retry** — do not clobber, and do not
give up:

1. Look at `current_state` in the error.
2. Decide whether your change still makes sense against it.
3. Re-send with the new `latest_version`.

Most conflicts resolve themselves this way without troubling the human.

---

## Things to avoid

- **Don't invent a `session_id` per call.** One per conversation.
- **Don't create a project without asking.** See above.
- **Don't write a task for every step.** See above.
- **Don't edit an accepted decision.** Supersede it with a new one linked by
  `supersedes`. Keel will refuse the edit and tell you this.
- **Don't summarise customer feedback into the body.** Put the verbatim words
  there and your reading in the linked spec. The verbatim version is the part
  that stays useful.
- **Don't use `keel_update` to change a document body.** That is
  `keel_write_doc`, which versions it. `keel_update` is for the fields around
  it: title, status, kind.
- **Don't ask permission to write.** If something was decided, write it down.
  Writing is cheap, reversible (nothing is ever deleted), and the whole point.

---

## When you are unsure whether something is worth recording

Record it. Nothing in Keel is ever deleted — archiving is a soft delete — so the
cost of writing something that turns out not to matter is close to zero, and the
cost of losing a decision is a re-litigated argument in three weeks.

The exception is the shredding failure above: one meaningful artifact beats five
trivial ones.

---

## The nine tools

| Tool | Reach for it when |
|---|---|
| `keel_context` | starting any project conversation — **first**, always |
| `keel_search` | "what do we know about X", "has this come up before" |
| `keel_get` | you have an id, or you want the graph around something |
| `keel_projects` | before creating a project; resolving a name |
| `keel_activity` | "what changed since I last looked" |
| `keel_create` | anything new |
| `keel_update` | status, priority, fields |
| `keel_write_doc` | the prose body of a spec, decision, question or feedback |
| `keel_link` | connecting two artifacts |

Each tool's own description says more. Read them.
