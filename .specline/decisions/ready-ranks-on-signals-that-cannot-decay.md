<!-- specline:generated decision dec_01M09S1GX2DPBW402EN4V08NQX v1 2026-08-18T06:51:38Z
     source of truth is Specline — edits here are not saved -->
# B-83 — Ready ranks on signals that cannot decay

**Status:** `accepted`  
**Id:** `dec_01M09S1GX2DPBW402EN4V08NQX`

Ready orders work by what each task unblocks, then by priority. Measured against the real store today, both inputs are flat:

- `unblocks` is 0 for all 29 open tasks. There are 76 `blocks` edges, and every one points at a task that is already closed.
- Priority is 21 p2 and 8 p3. No p0, no p1.

So the order inside the p2s falls through to a tiebreak, and the page renders a numbered list from 1 to 29 built on nothing. That is worse than showing no order at all, because the numbering implies a judgement that was never made, and KB cannot audit a reason that reads the same on every row.

**The decision.** Ready ranks on signals that are always computable, and stops implying a total order it cannot support. It leads with a short "next up" of two or three, each carrying a reason that differs from the others, and groups the rest.

**What was rejected, and why.** The alternative was to feed the ranking: keep priorities spread and draw `blocks` edges between open tasks. KB ruled it out. The 76 stale edges are the argument — they were drawn when that work was live and nobody pruned them, so the input decayed on its own. A ranking that needs someone to remember something will be wrong exactly when nobody remembered.

**What this costs.** Milestone is the only signal carrying intent, and 19 of 29 open tasks have none, so grouping will put two thirds of the work in one bucket ordered by age. That is honest rather than good. If it becomes annoying, the fix is milestones on more rows, which is a person's judgement and not bookkeeping a machine can fake.

