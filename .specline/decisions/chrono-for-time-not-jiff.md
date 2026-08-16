<!-- keel:generated decision dec_01KZKMPVPM94XEZGCSFS73XQ9T v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-1 — chrono for time, not jiff

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVPM94XEZGCSFS73XQ9T`

## Decision

chrono for time, not jiff.

## Reasoning

`duckdb-rs` ships a first-class `chrono` feature with `ToSql`/`FromSql` for `TIMESTAMP`; there is no `jiff` feature. Choosing `jiff` would mean a hand-written conversion shim at every storage boundary — the exact place a timezone bug would hide — for no domain benefit. Recorded here because `product/CLAUDE.md` requires picking one and never mixing.

## Reversible?

Yes, painfully.

