<!-- specline:generated decision dec_01KZKWMSX25E73XSGB9Q9A0P5W v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-4 — No vector or FTS index on the Lance dataset initially — brute-force scan

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMSX25E73XSGB9Q9A0P5W`

## Decision

No vector or FTS index on the Lance dataset initially — brute-force scan.

## Reasoning

Verified that `lance_fts`, `lance_vector_search` and `lance_hybrid_search` all return correct results with no index present. At a few thousand documents an index is pure cost. Per the scale-discipline rule, a measurement comes before an index.

## Reversible?

Yes.

