<!-- specline:generated decision dec_01KZS7XBT5GZXPG7CGYN75WWYZ v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-51 — Phase 9 runs before Phase 10, and DuckDB and Lance come out of the tree entirely

**Status:** `accepted`  
**Id:** `dec_01KZS7XBT5GZXPG7CGYN75WWYZ`

## Context

The Phase 9 spec ended with one thing needing KB: whether the SQLite move runs before Phase 10, or whether `DUCKDB_DOWNLOAD_LIB=1` is a good enough interim.

## Decision

Phase 9 runs now, before Phase 10. And it does not stop at "SQLite is the store" — once the migration is verified, every DuckDB and Lance dependency comes out of the repository: the crate dependencies, the code paths, the install scripts, the CI workflow, the profile settings that only exist because a C++ database is vendored, and the two-format backup path.

KB's call, 2026-08-11, on the record in this session.

## Reasoning

The interim removes the 22-minute build and leaves everything else: two formats, two backup paths, a keyword index rebuilt wholesale, and a release story that ships a 40–60 MB library beside every binary. It is a cost paid at every release rather than once.

Going further than the spec asked — full removal rather than a store swap — is what makes the phase worth its cost. A tree with both engines in it is a tree where either can come back by accident, and the second engine is not free even when nothing calls it: it is in the lockfile, in CI, in `cargo deny`, and in the dev profile settings written to stop DuckDB's debug info filling a disk.

## What this commits to

The work happens on a branch, not on master. Master keeps a working store until the migration is verified by row count per table and hash per document, which is what the spec's step 5 already says.

A third thing joins the phase that the spec does not name: a measurement of what the app and the daemon actually cost to load, taken before the swap and repeated after. Without a before, "SQLite made it faster" is a thing nobody can check, and the intermittent board stall KB reports has never been measured at all.

