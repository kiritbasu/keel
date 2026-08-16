<!-- specline:generated decision dec_01KZPFPHCFPZ1X930DEC2ZRR7R v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-23 — keel_context answers "what next" with named, ranked tasks

**Status:** `proposed`  
**Id:** `dec_01KZPFPHCFPZ1X930DEC2ZRR7R`

## Decision

keel_context answers "what next" with named, ranked tasks. Ready work is ordered by what it unblocks first, then priority.

## Reasoning

KB, looking at the finished app: *"I don't understand what's next to build."* The digest was returning counts and a query to run — and the query returned nothing, because no `blocks` edge existed. Option (a) of TQ-16, chosen by KB. Ranking on the graph before the label is the part worth defending: a p1 that releases three tasks moves the project further than a p0 that releases none, and the count comes from edges a human already drew rather than a judgement this code invents. Three buckets, not one list — **ready**, **waiting on a human**, **blocked** — because a p0 decision nobody can start must not outrank work someone can. `ready` is capped at three and the truncation is reported: a ranked list of thirty is the same "you work it out" as a count. Rejected: a hand-written `next_action` field on the project, which is the STATUS.md problem again — right while maintained, silently stale after.

## Reversible?

Yes — one module, and the old count-based advice is still there under "Also worth noticing".

