<!-- specline:generated decision dec_01KZS0SFC4YAGPC58TGDG677T9 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-47 — The close reason is a column, and closing is checked on the transition

**Status:** `accepted`  
**Id:** `dec_01KZS0SFC4YAGPC58TGDG677T9`

KEEL-110's body left one thing open: `duplicate`, `superseded` and `no_change` are reasons, but only `done` and `wont_do` are statuses, so where the reason lives had to be settled when the task was picked up. It is a `close_reason` column on the task.

## What was chosen

Five reasons over two statuses. `done` maps to `Done`; `wont_do`, `duplicate`, `superseded` and `no_change` all map to `WontDo`, and the column says which of the four it was. `close_message` and `evidence` sit beside it.

The alternative was mapping the last three onto `wont_do` and recording the reason nowhere, which loses the only thing that distinguishes them. Adding four statuses was the other alternative, and it would have put the same information somewhere every query filtering on `is_open` has to learn about.

## Where the rule is enforced, and why it matters

In `DuckStore::update`, not in `work::close`. Any path into a terminal status is held to it, so a caller reaching for `keel_update(status: done)` to avoid answering the question is refused by the same check. That is the difference between an invariant and a second convention, and it is what makes the definition of done in this file more than a list somebody is asked to honour.

## Checked on the transition only

A hundred and seven tasks closed before any of this existed and carry no reason, no message and no evidence. Two things follow.

Running the check on every write would freeze every one of them: moving an old row's priority would be refused for a message nobody was being asked to write. And backfilling would mean inventing a reason for work nobody remembers — a store that cannot tell an invented reason from a stated one is worse than one with holes, because the holes are at least visible.

So the rule sits on the transition, and `keel lint` reports what falls through. That is the same shape TQ-34 settled for task summaries, for the same reason.

## The hole left open on purpose

`keel_create` with `status: done` still bypasses the rule. `bootstrap` and `import` both create already-closed rows, and enforcing on create would break both. Creating a task that is already finished is a migration shape rather than doing work, but it is a hole and it belongs on `keel lint`'s list.

