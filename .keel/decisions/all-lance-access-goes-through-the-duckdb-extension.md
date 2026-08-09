<!-- keel:generated decision dec_01KZKMPVPZ81H8PHKF8RHZK13R v1 2026-08-09T18:07:40Z
     source of truth is Keel — edits here are not saved -->
# All Lance access goes through the DuckDB extension

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVPZ81H8PHKF8RHZK13R`

## Decision

No `lance` or `lancedb` Rust crate.

## Reasoning

Verified that ATTACH (TYPE lance) gives full SELECT/INSERT/UPDATE, and that the search functions work. One connection, one SQL surface, one transaction story — and it drops lance v10 and arrow v59 from the build.

## Consequences

DocumentStore is a trait precisely so this can be swapped.

