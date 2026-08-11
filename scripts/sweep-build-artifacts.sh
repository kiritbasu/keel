#!/usr/bin/env bash
#
# Reclaim disk from build artifacts Cargo will never reclaim itself.
#
# Cargo has no garbage collector. Every time a fingerprint changes it builds a
# new artifact under a new hash and leaves the previous one on disk forever. A
# workspace lint, a new dependency, a feature flag — any of them invalidates the
# whole dependency graph, and the old generation stays.
#
# That is normally a rounding error. It is not one here, because every test
# binary statically links DuckDB: `libduckdb.a` is 237 MB, each linked test
# binary is 100–165 MB, and there are around thirty of them. One full rebuild
# after a fingerprint change is roughly 8.5 GB of test binaries, and the
# previous 8.5 GB is still sitting there.
#
# Measured on 2026-08-11, when the disk hit 100% and it surfaced as
# `rustc-LLVM ERROR: IO failure on output stream: No space left on device` —
# which reads as a compiler bug, not a full disk:
#
#   target/                          17 GB
#     debug/deps                     12 GB  (8.5 GB of it linked test binaries)
#     debug/incremental               4 GB
#   nine generations of keel_daemon, five of keel, four of libduckdb-sys
#   135 leaked test stores in TMPDIR   813 MB
#
# After this script: 8.1 GB, and all 490 tests still pass.
#
# Phase 9 is what actually fixes this. SQLite's amalgamation is about 1.5 MB of
# object code against DuckDB's 237 MB, so the same thirty test binaries cost
# megabytes rather than gigabytes and none of the above is worth a script.
# Until then, this is the thing to run when the disk gets tight.
#
# Safe by construction: everything it deletes, Cargo rebuilds on demand. The
# newest generation of each artifact — the one the last build produced — is
# always kept.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TARGET="$ROOT/target"
DRY_RUN="${DRY_RUN:-0}"

avail_kb() { df -k "$ROOT" | awk 'NR==2{print $4}'; }
human() { awk -v kb="$1" 'BEGIN{ printf "%.1f GB", kb/1048576 }'; }

before=$(avail_kb)
say() { printf '%s\n' "$*"; }
run() { if [ "$DRY_RUN" = "1" ]; then say "  would remove: $*"; else rm -rf "$@"; fi; }

say "sweeping build artifacts under $TARGET"
say ""

# ---- 1. The incremental cache -----------------------------------------
#
# Pure rebuild cache. Deleting it costs one slower build and nothing else.
for profile in debug release; do
  dir="$TARGET/$profile/incremental"
  [ -d "$dir" ] || continue
  say "incremental cache ($profile): $(du -sh "$dir" | cut -f1)"
  run "$dir"
done

# ---- 2. Superseded generations of linked binaries ----------------------
#
# `use_cases-b5742f7b47ab3941` and `use_cases-9c956f7b565c92c5` are the same
# test built twice under different fingerprints. Keep the newest, drop the rest.
# These are the extensionless files in deps/ and they are where the gigabytes
# are.
for profile in debug release; do
  deps="$TARGET/$profile/deps"
  [ -d "$deps" ] || continue
  cd "$deps"
  find . -maxdepth 1 -type f ! -name '*.*' \
    | sed -E 's|\./||; s/-[0-9a-f]{16}$//' | sort -u \
    | while read -r name; do
        # -t sorts newest first; everything after the first line is superseded.
        stale=$(find . -maxdepth 1 -type f -name "${name}-*" ! -name '*.*' -print0 2>/dev/null \
                | xargs -0 ls -t 2>/dev/null | tail -n +2 || true)
        [ -n "$stale" ] || continue
        say "superseded binaries for $name:"
        while IFS= read -r f; do [ -n "$f" ] && run "$f"; done <<< "$stale"
      done

  # The same rule for the fat static libraries. Named explicitly rather than
  # applied to every rlib, because most rlibs are kilobytes and the risk of a
  # clever pattern is that it matches something it should not.
  for stem in liblibduckdb_sys libduckdb liblibsqlite3_sys libkeel_core \
              libkeel_daemon libkeel_mcp libarrow libfastembed libort_sys; do
    stale=$(find . -maxdepth 1 -name "${stem}-*.rlib" -print0 2>/dev/null \
            | xargs -0 ls -t 2>/dev/null | tail -n +2 || true)
    [ -n "$stale" ] || continue
    while IFS= read -r f; do [ -n "$f" ] && run "$f"; done <<< "$stale"
  done
done

# ---- 3. Superseded build-script output -------------------------------
#
# `libduckdb-sys` had four output directories, each holding its own 237 MB
# `libduckdb.a`. Only the newest is referenced by the current build.
for profile in debug release; do
  build="$TARGET/$profile/build"
  [ -d "$build" ] || continue
  cd "$build"
  for stem in $(ls -1 | sed -E 's/-[0-9a-f]{16}$//' | sort -u); do
    dirs=$(ls -1dt "${stem}"-* 2>/dev/null || true)
    [ -n "$dirs" ] || continue
    [ "$(echo "$dirs" | wc -l)" -gt 1 ] || continue
    say "superseded build output for $stem"
    echo "$dirs" | tail -n +2 | while read -r d; do [ -n "$d" ] && run "$d"; done
  done
done

# ---- 4. Test stores leaked into TMPDIR --------------------------------
#
# KEEL-119. `tempfile::TempDir` deletes on Drop, so every killed or timed-out
# test run leaves a 4.8 MB DuckDB store behind. 135 of them had accumulated when
# this was written.
#
# Only directories that actually contain a Keel store, and only those untouched
# for an hour, so nothing a running test owns is taken out from under it.
TMP="${TMPDIR:-/tmp}"
leaked=0
cd "$TMP" 2>/dev/null || true
for d in "$TMP".tmp*; do
  [ -d "$d" ] || continue
  { [ -f "$d/keel.duckdb" ] || [ -f "$d/keel.sqlite" ]; } || continue
  [ -n "$(find "$d" -maxdepth 0 -mmin +60 2>/dev/null)" ] || continue
  run "$d"
  leaked=$((leaked + 1))
done
[ "$leaked" -gt 0 ] && say "leaked test stores older than an hour: $leaked"

after=$(avail_kb)
say ""
if [ "$DRY_RUN" = "1" ]; then
  say "dry run — nothing was removed. Run without DRY_RUN=1 to sweep."
else
  say "freed $(human $((after - before)))"
  say "target is now $(du -sh "$TARGET" 2>/dev/null | cut -f1), $(human "$after") free"
fi
