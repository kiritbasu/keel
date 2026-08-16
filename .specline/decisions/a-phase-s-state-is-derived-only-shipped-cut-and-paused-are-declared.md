<!-- keel:generated decision dec_01KZX9ZJWEGGFSPXK1MH750G94 v1 2026-08-13T10:22:25Z
     source of truth is Keel — edits here are not saved -->
# B-57 — A phase's state is derived; only shipped, cut and paused are declared

**Status:** `accepted`  
**Id:** `dec_01KZX9ZJWEGGFSPXK1MH750G94`

## Decision

`milestones.status` stops being a word somebody types. What a phase is *doing* is worked out from its tasks and its edges. What a phase has been *decided about* is stored, and there are only three such decisions.

**Derived, never stored, so they cannot disagree with anything:**

- **planned** — no task has moved off `todo`
- **active** — a task has started and something is still open
- **complete** — every task closed, nobody has said what that means yet
- **blocked** — something live links to the phase with `blocks`, exactly as it works for tasks

**Declared, stored, because no amount of looking at tasks can tell you:**

- **shipped** — a person says it shipped. `shipped_at` is written in the same operation, never separately.
- **cut** — dropped. Replaced rather than abandoned is `cut` plus a `supersedes` edge naming what replaced it, which is what tasks already do.
- **paused** — started, stopped, not abandoned.

The stored column holds `open` when nothing has been declared. `active` and `blocked` stop being storable at all.

## Why

Five of twelve phases contradicted their own tasks, and nobody noticed for a week. The damage was not cosmetic: the tracker and the digest name the first phase marked `active`, so every session started this week was told the active phase was Phase 9 — which had finished. The orientation line at the top of every conversation was wrong.

This is the same failure TQ-25 removed from tasks. `blocked` was a task status that could disagree with the `blocks` edges, and the fix was to stop storing it. The argument transfers without modification: a status that can disagree with the rows underneath it is a colour, not information.

## What made it certain rather than likely

Nothing derived it, nothing validated it, and nothing updated it when the last task in a phase closed. Compare what a task gets: `keel_claim` to start and record who, `keel_close` with one of five reasons, a message, and typed evidence for `done` — all enforced in the storage layer so the CLI and MCP cannot disagree. A milestone got five adjectives and an honour system.

## `done` and `wont_do` are not the same thing

Writing this, a session set Phase 5 to `shipped` by the rule "no open tasks". Phase 5 has one task and it was closed `wont_do`. Nothing was ever built, and the roadmap said delivered.

Both reasons empty the column and mean opposite things. A phase whose work was abandoned is `cut`. No rule that counts open tasks can tell the difference, which is the sharpest argument for `shipped` and `cut` staying human declarations.

## Why `paused` is the one new state

It is the only scenario here that is real, common, and underivable. A phase that has been set aside is not `active` — nobody is on it — and not `cut`, because it is coming back. Without it a shelved phase has to lie.

Everything else considered was rejected. **Superseded** is `cut` plus an edge, matching tasks; a sixth status would be a second way to say the same thing. **Ongoing** — for a phase like hardening that arguably never ends — is refused: a phase that cannot finish should be closed and a new one opened, not left running forever. **Designed but not scoped** — Phase 10 exists as a spec with no milestone and is invisible to the roadmap — is a workflow gap, not a status. It needs its own decision.

## What this fixes that was not on the original list

`shipped_at` is a second field saying the same thing as `status`, maintained separately. Phases 7 and 9 were set to `shipped` during this work and left with an empty `shipped_at`, because `keel_update` sets the field it was given and nothing else. Writing both in one operation is part of this decision, not a follow-up.

`sort_order` has the same shape — null on four phases, which is why the roadmap prints them out of order — but ordering is a human preference rather than a fact about the work, so it stays typed. It gets a lint, not a derivation.

## The cost

A migration, which is the second this project has had and the first exercise of the deliberate-migration path built for KEEL-154. Every surface that displays a phase — the digest, the tracker, the desktop roadmap — reads a derived value instead of a column, so the API returns the derived state alongside the row.

