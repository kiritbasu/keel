#!/usr/bin/env bash
#
# Reclaim disk from build artifacts Cargo will never reclaim itself.
#
# Cargo has no garbage collector. Every time a fingerprint changes it builds a
# new artifact under a new hash and leaves the previous one on disk forever. A
# workspace lint, a new dependency, a feature flag — any of them invalidates the
# whole dependency graph, and the old generation stays.
#
# That is normally a rounding error, and for most of this project's life it was
# not one. Every test binary statically linked DuckDB, so a single fingerprint
# change duplicated gigabytes of test binaries; on 2026-08-11 the disk hit 100%
# and surfaced as `rustc-LLVM ERROR: IO failure on output stream: No space left
# on device`, which reads as a compiler bug rather than a full disk. That is the
# afternoon this script was written for.
#
# Phase 9 removed the cause. SQLite's amalgamation is about 1.5 MB of object
# code where DuckDB's static library was 237 MB, so a superseded generation now
# costs megabytes rather than gigabytes. The script is kept because Cargo still
# never collects anything and the directories still accumulate — but it is
# housekeeping now, not a rescue. Run it when `target/` has grown untidy, not
# because the disk is about to fill.
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

  # The same rule for the largest rlibs. Named explicitly rather than applied to
  # every rlib, because most rlibs are kilobytes and the risk of a clever
  # pattern is that it matches something it should not.
  for stem in liblibsqlite3_sys libspecline_core libspecline_daemon libspecline_mcp \
              libfastembed libort_sys; do
    stale=$(find . -maxdepth 1 -name "${stem}-*.rlib" -print0 2>/dev/null \
            | xargs -0 ls -t 2>/dev/null | tail -n +2 || true)
    [ -n "$stale" ] || continue
    while IFS= read -r f; do [ -n "$f" ] && run "$f"; done <<< "$stale"
  done
done

# ---- 3. Superseded build-script output -------------------------------
#
# A build script that compiles C — `libsqlite3-sys`, `ort` — gets a fresh
# output directory per fingerprint, and its compiled objects stay in the old
# ones. Only the newest is referenced by the current build.
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
# KEEL-119. `tempfile::TempDir` deletes on Drop, so a run that finishes leaves
# nothing behind — measured on 2026-08-13, a completed test binary leaks zero.
# A run that is *killed* leaks every store it had open, because Drop never
# runs. One Ctrl-C partway through `cargo test --workspace` left 27.
#
# That is why the accumulation is bursty rather than steady. The 2,318 stores
# found on 2026-08-13 came from a single `cargo mutants` run the day before:
# 24 of its 113 mutants timed out, and each timeout kills a test binary that
# had already opened ninety-odd stores.
#
# The glob below used to be `"$TMP".tmp*`, which is only correct when TMPDIR
# ends in a slash. macOS sets one, so it worked here and nowhere else: with
# TMPDIR=/tmp it expanded to `/tmp.tmp*`, matched nothing, and reported
# nothing — a sweeper that silently swept zero. Stripping the slash and using
# an explicit `/` makes it independent of how TMPDIR is spelled.
#
# Only directories that actually contain a Specline store, and only those untouched
# for an hour, so nothing a running test owns is taken out from under it.
# `specline.duckdb` is here for the stores left by runs that predate Phase 9; a
# DuckDB store was 4.8 MB against SQLite's 388 KB, so the few that remain are
# worth more than their number suggests.
TMP="${TMPDIR:-/tmp}"
TMP="${TMP%/}"
leaked=0
cd "$TMP" 2>/dev/null || true
for d in "$TMP"/.tmp*; do
  [ -d "$d" ] || continue
  [ -f "$d/keel.sqlite" ] || [ -f "$d/keel.duckdb" ] || continue
  [ -n "$(find "$d" -maxdepth 0 -mmin +60 2>/dev/null)" ] || continue
  run "$d"
  leaked=$((leaked + 1))
done
say "leaked test stores older than an hour: $leaked"

after=$(avail_kb)
say ""
if [ "$DRY_RUN" = "1" ]; then
  say "dry run — nothing was removed. Run without DRY_RUN=1 to sweep."
else
  say "freed $(human $((after - before)))"
  say "target is now $(du -sh "$TARGET" 2>/dev/null | cut -f1), $(human "$after") free"
fi
