<!-- keel:generated decision dec_01KZKWMT0JWXM2JGX7MZ0QZ7DV v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-6 — missing_docs is a workspace lint, not just a keel-core convention

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT0JWXM2JGX7MZ0QZ7DV`

## Decision

missing_docs is a workspace lint, not just a keel-core convention.

## Reasoning

The contract only requires doc comments on `keel-core` public items, but scoping the lint per-crate is more machinery than it saves, and documenting the daemon's public surface costs little.

## Reversible?

Yes.

