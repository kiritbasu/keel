<!-- specline:generated spec spc_01KZR487RKNSTBD8V9WXV27NBP v1 2026-08-19T08:16:32Z
     source of truth is Specline — edits here are not saved -->
# Keel — Phase 9

**Status:** `draft`  
**Kind:** `note`  
**Id:** `spc_01KZR487RKNSTBD8V9WXV27NBP`

# Keel — Phase 9
## One database

## Where this sits

Phases 0–5 are the original plan. Phase 6 (*Make the tracker real*) shipped on 2026-08-11; Phase 7 (*Clean up the footprint*) closed alongside it. This document is one of three that follow:

| | |
|---|---|
| **Phase 8 — The working loop** | verbs for doing work, filing issues from the app, and the app made legible |
| **Phase 9 — One database** | DuckDB + Lance → SQLite |
| **Phase 10 — Release, distribution and install** | one pasted line, nothing compiles |

Written 2026-08-11 for KB, in plain English. Where a term from the codebase appears it is explained the first time.

**Goal:** one SQLite file in place of DuckDB and Lance. Invisible to a user, and the thing that makes Phase 10 cheap to maintain rather than merely possible.

**Sequencing note:** this was called optional and last in an earlier draft. Phase 10's zero-compile requirement changed that — the reasoning is at the end of this document.

---

## What it is

Replacing DuckDB and Lance with a single SQLite file: `rusqlite` with the bundled amalgamation, FTS5 for keyword search, `sqlite-vec` for vectors, blobs in an ordinary table.

## The case, now that the urgent parts are gone

Two of the four original arguments have expired — `fsck` already runs alongside the daemon, and the schema changes are already made. What remains is footprint, and it is real:

- **The build is 22 minutes and would be 1 minute.** Measured on the same two cores: DuckDB bundled at 22m11s wall / 38m31s CPU; `rusqlite` + `sqlite-vec` at 1m00s / 56s. This is the difference between shipping release binaries being easy and being a chore — so 8D gets much cheaper if this lands first, which is the one argument for reordering.
- **The keyword index is rebuilt wholesale on every write.** DuckDB's full-text index does not update when the table changes — its current documentation still says the workaround is "recreating the index to refresh" — and `store/docs.rs` does exactly that: `CREATE OR REPLACE TABLE fts_entities` followed by `PRAGMA create_fts_index(… overwrite = 1)`. It works, and it will not scale past this project.
- **Two formats, two backup paths.** `keel backup` exports DuckDB and separately dumps Lance to Parquet, and restore must refuse a backup missing its Lance half. In SQLite that is `VACUUM INTO`, measured at 1.2 ms against a live database.
- **Blobs would live with their rows** — a 5 MB image writes in 49.6 ms and reads in 10.8 ms, in the same transaction as the artifact it belongs to.

## The survey

Fourteen candidates, current as of August 2026, with build times and capabilities measured by running probe programs rather than reading documentation.

| Option | Verdict |
|---|---|
| **SQLite via `rusqlite` + FTS5 + `sqlite-vec`** | **Chosen** |
| libSQL | Rejected — its own README steers new projects elsewhere; async-only; SQLite base frozen at 3.45.1; self-hosted sync server unreleased since Feb 2025 |
| Turso Database | **Ruled out** — `Parse error: Recursive CTEs are not yet supported`. Keel's graph traversal cannot be expressed. Also no FTS5, no incremental blob I/O, no BM25 score |
| SurrealDB embedded | Rejected — SurrealKV beta, blobs experimental and stored outside the database, three-artifact backup, `SURREAL_SYNC_DATA` defaults to false so it is not crash-safe out of the box |
| KuzuDB | Rejected — archived Oct 2025, acquired by Apple; the surviving fork's Rust crate does not currently build |
| CozoDB | Rejected — no release since Dec 2023, an unanswered "is this maintained?" issue, an open data-correctness bug |
| LanceDB as primary store | Rejected — no primary keys or uniqueness enforcement, unstable row IDs, no recursive traversal |
| redb, fjall | Rejected — key-value only |
| HelixDB, PGlite, pg-embed | Rejected — need a server process or run under WASM |

SQLite is the only option covering all seven needs at once: rows, recursive graph traversal, BM25, vectors, blobs, a monotonic event cursor, and per-row optimistic concurrency — with a single-file backup and no C++ build orchestration.

## How it is done safely

Storage is reached through exactly three traits — `EntityStore`, `GraphStore`, `DocumentStore` — with no raw SQL outside their implementations. That boundary was insisted on in Phase 0 and has never been collected on. It is what makes this a branch rather than a rewrite.

1. Implement the three traits against SQLite. Mechanical, roughly 4,000 lines.
2. **Run the existing test suite unchanged.** Round-trip, optimistic concurrency, idempotency, graph direction both ways for all nine relations, event cursor, backup round-trip. These tests are engine-agnostic; they *are* the migration specification.
3. `keel migrate` reads the DuckDB and Lance store and writes a new SQLite store. **It never touches the old one.**
4. Verify by per-table row count and per-document content hash, not by eye — reusing the assertion the backup test already makes.
5. Nothing switches until verification is clean.

## The risks

**`sqlite-vec` is 0.1.9**, its author says to expect breaking changes, and the stable release is brute-force only. Survivable: a full scan over a few thousand vectors is 1–3 ms, and because the vectors are ordinary blobs rather than a proprietary index, replacing the search is about fifty lines.

**No built-in sync**, and Phase 5 wants a phone to read status. Three answers exist that do not require choosing now: a read-only `VACUUM INTO` snapshot, Litestream-style replication, or moving the file to libSQL later — a matter of opening it, since it is a standard SQLite file.

**Migration during active use.** KEEL-95 is the live lesson: a migration added a column, a daemon 84 seconds behind wrote a NULL into it, and every decision in another project became unreadable. B-44 came out of it — *a column added by a migration is NULL for every writer that has not restarted yet*. A whole-engine migration is that hazard at maximum size, which is why step 3 writes a new store rather than modifying one in place, and why the version guard added in `3b02f26` matters.

## Phase 9 exit criteria

- Every existing test passes against SQLite with no test modified to accommodate it.
- Cold release build under 90 seconds.
- `keel migrate` on the real store, verified by row count per table and hash per document.
- `keel backup` produces one file; `keel restore` reconstructs a byte-identical store.
- A 5 MB PNG round-trips byte-identically.
- Keyword search returns a row created in the same transaction, with no index rebuild.

---


---

## Why this now runs before Phase 10

Shipping four platform targets, at every release, with a vendored C++ database in the tree means one of two things: a cross-compile that `duckdb-rs`'s own README calls "best-effort (not covered by CI)", or a 40–60 MB dynamic library shipped beside every binary with an rpath pointing at it. With SQLite the release artifact is one static file and the problem stops existing.

If KB would rather not move it, `DUCKDB_DOWNLOAD_LIB=1` links DuckDB's official prebuilt library and removes the 22-minute build immediately. That works. It is a cost that recurs at every release rather than one that is paid off.

## What needs KB's decision

1. **Whether Phase 9 runs before Phase 10 at all**, or whether the interim above is good enough.

---

## Appendix — what this rests on

| Claim | Source |
|---|---|
| DuckDB cold release build | 22m 11s wall, 38m 31s CPU, 2 vCPU, measured |
| SQLite + `sqlite-vec` cold build | 1m 00s wall, 56s CPU, same machine |
| Turso Database cannot express the graph | `Parse error: Recursive CTEs are not yet supported`, v0.7.2, probed directly |
| libSQL steers new projects away | its own repository README |
| DuckDB full-text index freshness | current DuckDB docs, and `store/docs.rs` rebuilds the index wholesale on write |
| DuckDB vector index crash safety | current DuckDB docs warn of "data loss or corruption of the index" |
| `VACUUM INTO` on a live database | 1.2 ms, measured |
| 5 MB blob write / read in SQLite | 49.6 ms / 10.8 ms, measured |
| SQLite reader during an open write | 150 µs, correct pre-transaction snapshot |
| DuckDB cross-compilation | `duckdb-rs` README: bundled cross-compilation is "best-effort (not covered by CI)" |

