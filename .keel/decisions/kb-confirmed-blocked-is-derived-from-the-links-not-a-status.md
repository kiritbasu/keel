<!-- keel:generated decision dec_01KZP5189J3N9R1BJESQ0PGJNZ v1 2026-08-10T16:05:17Z
     source of truth is Keel — edits here are not saved -->
# KB confirmed: blocked is derived from the links, not a status

**Status:** `accepted`  
**Id:** `dec_01KZP5189J3N9R1BJESQ0PGJNZ`

## Context

TQ-25. RESET-PLAN 6.5 settled that the links win for what "blocked" means. It did not settle whether `blocked` survived as a value a caller could set, and the two readings lead to materially different work.

## Decision

**KB confirmed, 2026-08-10: derive it.** A task is blocked exactly when something links to it with `blocks`. `blocked` stops being a `TaskStatus`; the board's column becomes a computed grouping; the counts in the app, the digest and the generated tracker all come from the same derivation.

## Reasoning

The same call this codebase has made everywhere else: make the contradiction unrepresentable rather than detectable. Option 1 would have kept two facts that must agree and added an integrity check to notice when they do not — which is a check that fires *after* someone has already read the wrong number.

The evidence was on screen while the question was being asked. The digest reported two tasks as "marked blocked, but nothing links to it with `blocks`" — KEEL-45 and KEEL-48. Under the rejected option those become findings a human clears, one at a time, forever. Under this one they simply stop being blocked, because nothing is blocking them and nothing ever was.

It costs a forward-only migration and a visible behaviour change, which is why it was KB's to make rather than mine.

## Reversible?

The migration is forward-only. Re-adding an enum value later is easy; the rows that were moved out of `blocked` would not come back, and should not — they were wrong.

