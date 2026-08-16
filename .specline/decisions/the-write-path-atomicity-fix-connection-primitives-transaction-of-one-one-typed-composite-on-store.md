<!-- keel:generated decision dec_01KZSQJ05N4TSXDETPAZKD685F v1 2026-08-16T07:20:54Z
     source of truth is Keel — edits here are not saved -->
# B-53 — The write-path atomicity fix: &Connection primitives, transaction-of-one, one typed composite on Store

**Status:** `proposed`  
**Id:** `dec_01KZSQJ05N4TSXDETPAZKD685F`

**Reconstructed on 2026-08-16 from the code that implements it, and it should be read as that.** This row landed with a title and no body — the create path allowed it, which is the bug KEEL-171 has now closed. The title names three things; what follows is what each of them turned out to mean, read out of `crates/keel-core/src/store/`. The session's own argument is gone.

The problem: creating a design with a caption and a screenshot was four store calls orchestrated from `keel-mcp` over untyped JSON — insert the row, write the first revision, store the blob, then update the row a second time to record which blob it was. A crash anywhere in that sequence left an entity with no body, or a blob nothing points at. `fsck` had no blob check, so an orphaned blob was invisible and therefore unreclaimable for ever.

The three parts:

- **`&Connection` primitives.** The steps a write is made of — `insert_created`, `append_event_inner`, `write_revision_in`, `insert_blob_in` — each take a connection or a transaction rather than opening their own. That is what lets them compose inside one transaction instead of being four transactions in a row.
- **Transaction-of-one.** Every write path opens a transaction even when it has a single statement, so the row and the events describing it land together. An update that lands its version bump and loses its events is worse than one that fails: the optimistic-concurrency check accepts the next write happily, so nothing ever notices the hole.
- **One typed composite on `Store`.** `create_with_document(entity, body, image, provenance)` replaces the orchestration. The blob id is minted before the row is inserted, so the row carries `blob_id` from the start and the second `update` round-trip disappears entirely — the correctness fix is also a simplification.

Still `proposed` rather than `accepted`, which is a status nobody moved rather than a decision anybody reversed: the code has been in place since Phase 11.

