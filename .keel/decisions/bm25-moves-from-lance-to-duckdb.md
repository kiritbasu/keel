<!-- keel:generated decision dec_01KZKMPVRTXA717P2854N17HQ5 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# BM25 moves from Lance to DuckDB

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVRTXA717P2854N17HQ5`

## Context

SPEC §5 put both halves of hybrid search inside lance_hybrid_search.

## Decision

BM25 in DuckDB's fts extension; Lance does vectors only.

## Reasoning

The keyword half could not be characterised. "onboarding metering" returned a document containing only *metering*; "onboarding slow" returned nothing despite a document containing *onboarding*. The extension documents only single-word examples and no way to build the index that would presumably fix it. A search returning plausible-but-wrong results is the same failure class as an inverted traversal.

## Consequences

The DuckDB index now covers prose too, so a spec and a task compete in one ranking. Flagged as TQ-10.

