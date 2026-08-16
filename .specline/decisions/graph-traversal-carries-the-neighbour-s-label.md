<!-- specline:generated decision dec_01KZNQ3BCRH4CM0CAVV3DYC7TQ v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-38 — Graph traversal carries the neighbour's label

**Status:** `accepted`  
**Id:** `dec_01KZNQ3BCRH4CM0CAVV3DYC7TQ`

## Context

`Neighbour` was `{id, entity_type, rel, anchor, depth, path}`. Everything that rendered or reasoned about a traversal therefore had to go back and look up what each id was.

Two callers had already gone wrong in the same way. The document reader printed bare ULIDs under "Connected" — the id was all it had. And an agent walking the graph got a list of identifiers it had to follow with a `keel_get` per hop to learn what it had found.

## Decision

**`neighbours()` joins `v_entities` and returns a `label`.**

## Reasoning

`v_entities` exists for exactly this: SPEC §4 built it to resolve an id without knowing its type, and unifying the four different name columns — `name`, `title`, `term`, `summary` — is what its `label` column is for. The join is a `LEFT JOIN` on the walk's final select, which costs one lookup per returned row on a store of a few thousand.

`LEFT`, not inner, and this is the part worth keeping: an edge pointing at a row that no longer resolves comes back with an empty label rather than being dropped. Dropping it would turn a visible integrity problem into a silently shorter graph — hiding precisely what `fsck`'s dangling-link check exists to report, and in the direction that makes everything look fine.

## Reversible?

Yes, but there is no reason to. It is one additive field on a read shape.

