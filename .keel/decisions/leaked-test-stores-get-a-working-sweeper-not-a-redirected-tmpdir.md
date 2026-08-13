<!-- keel:generated decision dec_01KZXA7K5NXDGTVTEG9G26JPBB v1 2026-08-13T10:22:25Z
     source of truth is Keel — edits here are not saved -->
# B-58 — Leaked test stores get a working sweeper, not a redirected TMPDIR

**Status:** `accepted`  
**Id:** `dec_01KZXA7K5NXDGTVTEG9G26JPBB`

KEEL-119 offered three ways to stop killed test runs leaving stores in TMPDIR. A fourth came up while working on it: point `TMPDIR` at a repo-local directory from `.cargo/config.toml`, which cargo applies to every process it spawns. That would have covered all 157 `tempfile::tempdir()` call sites without touching one of them, and confined the leak to a directory you can see.

Rejected, on the measurement. A test binary that runs to completion leaks nothing — `TempDir::drop` works. Only a killed process leaks, and the accumulation on disk traces to a single `cargo mutants` run whose mutants time out. So the leak is local, occasional, and 388 KB a time since Phase 9 dropped DuckDB.

Against that, redirecting TMPDIR globally changes where rustc and the linker put their scratch files too, and it fails in a confusing way if the directory is ever missing — every temp-using process in the build breaks at once. That is a lot of blast radius for housekeeping, and the scale-discipline rule says the measurement has to argue for the machinery. This one argues against it.

What was done instead: fix the sweeper in `scripts/sweep-build-artifacts.sh`, which was globbing `"$TMP".tmp*` and therefore only worked where TMPDIR ends in a slash, and make it report its count even when that count is zero.

Reversible. If mutation testing becomes routine rather than a weekly scheduled job, the redirect is still there to reach for.

