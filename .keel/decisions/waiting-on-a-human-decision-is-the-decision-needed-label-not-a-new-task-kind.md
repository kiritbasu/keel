<!-- keel:generated decision dec_01KZPFPS0KK4YE59E3A8GJQ0VW v2 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-25 — "Waiting on a human decision" is the decision-needed label, not a new task kind

**Status:** `proposed`  
**Id:** `dec_01KZPFPS0KK4YE59E3A8GJQ0VW`

## Decision

"Waiting on a human decision" is the decision-needed label, not a new task kind or column.

## Reasoning

The bootstrap already used the label, so the data existed. A new `TaskKind` would be a schema change to express something a label expresses, and `product/CLAUDE.md` is explicit that a new type or field is almost always the wrong answer to awkward modelling. The cost is that it is a convention: nothing enforces it, and a decision task without the label ranks as ordinary work.

## Reversible?

Yes — trivially.

