<!-- specline:generated decision dec_01KZKMPVQD8N0ZYBZZBMWTCP04 v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-3 — Bundled DuckDB is a feature, not a requirement

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVQD8N0ZYBZZBMWTCP04`

## Context

Compiling DuckDB from source costs about ten minutes on a cold build.

## Decision

bundled is the default, but it is now a feature — --no-default-features links a system libduckdb instead.

## Reasoning

Originally justified as "the binary can never disagree with the extension versions it loads", which **overstated it**: `INSTALL lance` re-fetches for whatever version is running, so a system library self-heals given network. The genuine reasons are narrower — a self-contained binary that keeps working after `brew upgrade duckdb`, and a build that needs no setup on a fresh machine. Neither justifies a ten-minute wait on a machine that already has the right library, so it is a feature now. **Verified both ways:** system-linked builds the workspace in 54s versus roughly ten minutes, and all 264 tests pass against Homebrew's libduckdb 1.5.5, Lance extension included. The default stays `bundled` because the *installed* binary should not break when Homebrew moves underneath it; the fast path is for development.

## Consequences

System-linked builds the workspace in 54s versus roughly ten minutes, with all tests passing either way.

## Reversible?

Yes.

