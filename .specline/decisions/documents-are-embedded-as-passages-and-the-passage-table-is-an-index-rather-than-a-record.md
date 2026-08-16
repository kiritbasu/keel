<!-- specline:generated decision dec_01KZX83HF50F7T90B2CD1P7EZ7 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-55 — Documents are embedded as passages, and the passage table is an index rather than a record

**Status:** `accepted`  
**Id:** `dec_01KZX83HF50F7T90B2CD1P7EZ7`

KB decided, 2026-08-13, after the truncation measurement in KEEL-174.

`bge-small-en-v1.5` reads 512 tokens and a document goes to it whole, so 41% of current documents were never going to be embedded past their opening. Documents get split into passages instead: headings first, then a hard wrap around 1,400 characters with roughly 15% overlap, and the heading path prepended to each passage's text so a passage from §5 of the spec still carries what it is a section of.

A new `document_chunks` table holds them, keyed to `doc_id`, carrying `ordinal`, `heading_path`, the character span, the text, the vector and the source revision's `body_hash`. Query side groups by entity and takes the **best** matching passage per document — mean would punish a long document for having sections about other things, which is backwards. The passage doubles as the excerpt, which is better than the fixed window around the first matching term that it replaces.

**`documents.embedding` stops being written and is dropped in a later migration.** One place for vectors. The argument against a `vec0` table already in `store::search` — a second copy of every vector, and something has to keep it in step — applies just as well to a whole-document vector sitting beside per-passage ones. Nothing in Keel asks "what is this document broadly about", so the second copy would exist to drift.

**Passages are hard-deleted when the revision they came from is replaced, when the entity is archived, or when the model changes.** This is an explicit exception to hard constraint 3, and the distinction it rests on is one the codebase already relies on: `fts_source` is a derived index whose triggers already delete, and nobody has ever called that a violation, because the record is the revision in `documents` — immutable, append-only, and untouched. A passage is a derived artefact of a revision in the same way a BM25 posting is. Constraint 3 gains a carve-out naming derived indexes, and a test proves a passage can always be recomputed from its revision.

The alternative was an `archived_at` on every passage and a filter on every query, which is consistent with the constraint as written and means the passage table outgrows everything else in the store within a year while holding nothing a person can read.

The model stays full-precision `bge-small-en-v1.5` — 134 MB rather than the 67 MB compressed variant. It downloads once, in the background, while keyword search already answers, so nothing blocks on it, and the quality cost of the compressed one is not predictable on a corpus this shape. Reversible either way: same 384 dimensions, so switching is a re-embed pass and not a schema change, which is what `embedding_model` on the row is for.

Resolves QUE "May the chunk index be hard-deleted". Related: TQ-3, which asks the re-embedding question this makes cheaper — a model change is now a delete-and-recompute over a derived table rather than a rewrite of the document rows.

