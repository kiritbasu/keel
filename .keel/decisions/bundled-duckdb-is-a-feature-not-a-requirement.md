<!-- keel:generated decision dec_01KZKMPVQD8N0ZYBZZBMWTCP04 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Bundled DuckDB is a feature, not a requirement

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVQD8N0ZYBZZBMWTCP04`

## Context

Compiling DuckDB from source costs about ten minutes on a cold build.

## Decision

`bundled` on by default, `--no-default-features` links a system libduckdb.

## Reasoning

The original justification overstated it: INSTALL lance re-fetches for whatever version is running, so a system library self-heals. The real reasons are a self-contained binary that survives `brew upgrade duckdb`, and a build that needs no setup on a fresh machine — both about the *installed* binary.

## Consequences

System-linked builds the workspace in 54s versus roughly ten minutes, with all tests passing either way.

