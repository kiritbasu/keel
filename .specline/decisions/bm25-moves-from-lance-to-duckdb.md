<!-- specline:generated decision dec_01KZKMPVRTXA717P2854N17HQ5 v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-12 — BM25 moves from Lance to DuckDB

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVRTXA717P2854N17HQ5`

## Context

SPEC §5 put both halves of hybrid search inside lance_hybrid_search.

## Decision

BM25 moves from Lance to DuckDB. lance_hybrid_search is not used; the Lance index does vectors only.

## Reasoning

SPEC §5 put both halves of hybrid search inside `lance_hybrid_search`. Its keyword half could not be characterised. On an un-indexed dataset, multi-term queries match inconsistently: `"onboarding metering"` returns a document containing only *metering*, while `"onboarding slow"` returns **nothing** despite a document containing *onboarding*. A third query returned an unrelated document with a score identical to an unrelated query's. The extension's documentation shows only single-word examples (`'puppy'`) and documents no way to build the index that would presumably fix this. A search returning plausible-but-wrong results is the same failure class as an inverted graph traversal, so it gets the same answer: put it where the semantics are known. DuckDB's `fts` extension is a real BM25 index with documented behaviour, and the index now covers *every* searchable artifact — prose titles and bodies joined in from Lance — so a spec and a task compete in one ranking instead of two. Lance keeps what it is uniquely for: the vector index and the multimodal blobs. `keel-core` was already doing the cross-index RRF fusion, so nothing else moved. **This is a §5 design change and is flagged to KB as TQ-10.**

## Consequences

The DuckDB index now covers prose too, so a spec and a task compete in one ranking. Flagged as TQ-10.

## Reversible?

Yes — it is one module, and `DocumentStore` is a trait.

