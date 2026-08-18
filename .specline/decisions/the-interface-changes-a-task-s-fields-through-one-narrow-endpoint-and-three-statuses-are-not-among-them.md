<!-- specline:generated decision dec_01M0B18PA7WYJ57E5P5HZBD712 v1 2026-08-18T18:21:12Z
     source of truth is Specline — edits here are not saved -->
# B-87 — The interface changes a task's fields through one narrow endpoint, and three statuses are not among them

**Status:** `accepted`  
**Id:** `dec_01M0B18PA7WYJ57E5P5HZBD712`

#### Decision

A person can change a task's status, priority, kind, phase and labels from the app. It goes through `PATCH /api/tasks/{id}`, which takes those five named fields and a `version`, and nothing else.

Two of the five statuses are refused there, and one more is refused as well:

- **`done` and `wont_do`** keep going through `/api/tasks/{id}/close`, which collects the reason, the message and the evidence the storage layer demands on every path into a terminal status.
- **`in_progress`** is refused outright. Starting work is a claim, and a claim records *which session*.

Moving *out* of `in_progress` clears the claim.

#### Why this needed no new permission

Hard constraint 7, as B-78 rewrote it, already names this: *"Creating a task, commenting on one, archiving or closing a row, moving a status or a priority — those are a person's own actions, and the interface performs them."* Kind, phase and labels are the same class of thing. What was missing was the endpoint and the controls, not the argument, and it is worth saying plainly that the constraint anticipated this rather than being stretched to fit it.

#### Why one narrow endpoint rather than a general one

A generic `PATCH /api/entity/{id}` would serve every artifact type and would be less code. It was rejected on B-78's own test: *"an endpoint that accepts a document revision is on the wrong side of it."* A generic patch would have to grow a rule refusing `body`, and a rule can be forgotten in a way that a parameter list cannot. Five named fields make prose unreachable by construction rather than by vigilance.

#### Why `in_progress` is refused rather than allowed

This is the one that cost the most thought, and it was KB's call.

Claiming exists because across sixty-six tasks the number of transitions into `in_progress` before work began was zero. `specline_claim` fixed that by making it a tool, and the thing that makes it work is that it records who — it is the one call refused outright without a `session_id`, on the grounds that a claim naming nobody says the task is taken and not by whom.

A person clicking a dropdown has no session. Three options, and the shape of each:

1. **Refuse it.** The dropdown offers `todo` and `review`; starting work stays something Claude does, because it is the only actor with a session. *Chosen.*
2. **Claim as the human** — a `claimed_by` that means a person rather than a session. Keeps the invariant, at the cost of widening what the column holds.
3. **Set it with no claim.** Rejected: it reintroduces precisely the state the claim tool was built to eliminate, and it would do so through the surface a person looks at most.

The cost of (1) is real and worth naming: the transition made most often is the one the app will not do. It is accepted because the alternative is a board that says work is in flight and cannot say by whom, which is worse than a board that sends you to the conversation where the work is actually happening.

#### Why leaving `in_progress` releases the claim

`close` does not clear `claimed_by`, and does not need to — a closed row cannot be claimed again, so what is left there is history. A row moved back to `todo` can be claimed, and a claim still standing on it has `specline_claim` refuse it for up to three days in the name of a session that walked away. So the patch clears it on the way out.

#### What a closed task can still change

Its priority, kind, phase and labels; not its status. Recategorising something finished is ordinary. Reopening is not: it means deciding what becomes of the close reason and the evidence, and that is a question rather than a control.

#### What this leaves open

Dragging a card between board columns, which is the same rules over a different gesture, is KEEL-308. The IN PROGRESS column is not a drop target there, for the reason above, and the column says so rather than silently refusing.

