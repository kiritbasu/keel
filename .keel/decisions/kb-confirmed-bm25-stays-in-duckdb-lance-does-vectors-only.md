<!-- keel:generated decision dec_01KZN5H4FFR7VHD92Z1PWRTMRA v1 2026-08-10T06:41:18Z
     source of truth is Keel — edits here are not saved -->
# KB confirmed: BM25 stays in DuckDB, Lance does vectors only

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZN5H4FFR7VHD92Z1PWRTMRA`

TQ-10 answered 2026-08-10. B-12 stands and is no longer provisional. SPEC section 5 is now formally wrong about which engine ranks keywords and should be corrected.

The original design put both halves of hybrid search in lance_hybrid_search. Its keyword half could not be characterised: 'onboarding metering' returned a document containing only metering, 'onboarding slow' returned nothing despite a document containing onboarding, and a third query returned an unrelated document with a score identical to an unrelated query's.

DuckDB's fts extension is a real BM25 index with documented behaviour, and it covers every artifact type rather than prose alone, so a spec and a task compete in one ranking instead of two. Lance keeps the vector index and the blobs. Search has behaved correctly throughout.

