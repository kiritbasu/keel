<!-- keel:generated decision dec_01KZP1E78WZXXTJZK7YBHATJCZ v1 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-41 — KB confirmed: a task holds a list of external links

**Status:** `accepted`  
**Id:** `dec_01KZP1E78WZXXTJZK7YBHATJCZ`

## Context

TQ-23. RESET-PLAN 6.2 asked for a task to be able to hold more than one external reference. `tasks.external_ref` was `Option<String>`, and changing it is a storage-format change, so it was raised rather than assumed.

## Decision

**KB confirmed, 2026-08-10: a task can hold more than one.** `external_ref VARCHAR` becomes `external_refs VARCHAR[]`, backfilled from the single value and then dropped, in the same migration as rank and the parent link.

## Reasoning

Option 1 of the three offered. The column type already exists on this table — `labels` is a `VARCHAR[]` — so it costs one migration step and no new machinery, and it is the only one of the three field additions that needs no new UI beyond rendering a list where a string was rendered.

The old column is dropped rather than kept alongside. Two columns meaning the same thing is drift with a schedule attached, and the one that stops being written is the one everything keeps reading.

Renaming rather than aliasing: a caller passing `external_ref` now gets serde's own "task has no field `external_ref`… any of: …", which names the replacement. Accepting both would be the undocumented-parameter problem RESET-PLAN 7.3 exists to remove, created deliberately.

## Reversible?

Forward-only, like every migration here. The data survives either way — the backfill copies before the drop.

