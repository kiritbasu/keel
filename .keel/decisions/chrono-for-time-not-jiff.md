<!-- keel:generated decision dec_01KZKMPVPM94XEZGCSFS73XQ9T v1 2026-08-09T18:07:40Z
     source of truth is Keel — edits here are not saved -->
# chrono for time, not jiff

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVPM94XEZGCSFS73XQ9T`

## Decision

`chrono`.

## Reasoning

duckdb-rs ships a first-class chrono feature with ToSql/FromSql for TIMESTAMP; there is no jiff feature. Choosing jiff would mean a conversion shim at every storage boundary — the exact place a timezone bug would hide — for no domain benefit.

