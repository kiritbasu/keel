<!-- keel:generated decision dec_01KZPFPPEMGCEB5HXXPF1RFWDC v2 2026-08-10T19:40:43Z
     source of truth is Keel — edits here are not saved -->
# B-24 — A task marked blocked with no blocks edge is reported as a data problem, not ranked

**Status:** `superseded`  
**Id:** `dec_01KZPFPPEMGCEB5HXXPF1RFWDC`

## Decision

A task marked blocked with no blocks edge is reported as a data problem, not ranked.

## Reasoning

Keel's own store was in exactly this state: three blocked tasks, zero edges. The tempting behaviour is to trust the status and hide the task; the honest one is to say the status has no referent, because otherwise the thing that made the board unreadable stays invisible. The desktop board and the digest share one ranking from `keel-core`, so they cannot word it differently or disagree.

## Reversible?

Yes.

## Superseded

**Reversed 2026-08-10.** `blocked` is no longer a status — the enum value was removed in migration 8 and blocked is derived from the `blocks` edges, with `blocked_tasks()` as the single definition every surface reads.

This was the right fix for the wrong shape. Reconciling two sources of truth is work that only exists because there are two, and the reconciliation itself was load-bearing, which is what made it look like a feature rather than a symptom. Removing the status removed the disagreement instead of reporting it. KB's call, since it cost a forward-only migration and a visible behaviour change.

