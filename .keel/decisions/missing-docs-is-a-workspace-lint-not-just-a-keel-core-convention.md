<!-- keel:generated decision dec_01KZKWMT0JWXM2JGX7MZ0QZ7DV v1 2026-08-09T18:32:09Z
     source of truth is Keel — edits here are not saved -->
# missing_docs is a workspace lint, not just a keel-core convention

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT0JWXM2JGX7MZ0QZ7DV`

`B-6` · 2026-08-09

**Decision.** **`missing_docs` is a workspace lint, not just a `keel-core` convention.**

**Reasoning.** The contract only requires doc comments on `keel-core` public items, but scoping the lint per-crate is more machinery than it saves, and documenting the daemon's public surface costs little.

**Reversible?** yes

