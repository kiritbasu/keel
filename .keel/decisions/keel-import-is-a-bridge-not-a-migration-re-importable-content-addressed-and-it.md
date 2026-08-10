<!-- keel:generated decision dec_01KZKWMT5SMKXQ07NKBKT87SXC v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-18 — keel import is a bridge, not a migration: re-importable, content-addressed, and it…

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT5SMKXQ07NKBKT87SXC`

## Decision

keel import is a bridge, not a migration: re-importable, content-addressed, and it leaves the repo copy alone.

## Reasoning

KB asked whether whole specs can live in Keel and be read in the app. They can — a 51 KB SPEC.md round-trips byte-identical, stays searchable and diffs between revisions — so the only real question was how the repo files and the store stay in step. Import resolves an existing artifact by title before creating one, and `write_revision` is content-addressed, so re-running it on an unchanged file appends nothing. That makes it safe in a script or a hook, and it means `product/*.md` can stay authoritative for exactly as long as KB wants without the two copies drifting. Rejected: a one-way "move the files in and delete them", which forecloses a decision that is his (see TQ-13).

## Reversible?

Yes.

