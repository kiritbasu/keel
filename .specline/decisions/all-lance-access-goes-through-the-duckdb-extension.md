<!-- keel:generated decision dec_01KZKMPVPZ81H8PHKF8RHZK13R v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-2 — All Lance access goes through the DuckDB extension

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVPZ81H8PHKF8RHZK13R`

## Decision

All Lance access goes through the DuckDB lance extension. The lance and lancedb Rust crates are not dependencies.

## Reasoning

Verified empirically (see P0-2 table): `ATTACH … (TYPE lance)` gives full `SELECT`/`INSERT`/`UPDATE` over Lance datasets, and the three search functions work. Using the extension means one connection, one SQL surface, one transaction story — and it drops `lance` v10 + `arrow` v59 from the build entirely. Rejected: the native crate, which would have meant marshalling Arrow record batches by hand and keeping two Lance versions in step.

## Consequences

DocumentStore is a trait precisely so this can be swapped.

## Reversible?

Yes — `DocumentStore` is a trait precisely so this can be swapped.

