<!-- specline:generated decision dec_01KZPTGB10RJERCFAJR9C1R71B v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-44 — A missing readable number is read as unassigned, not as an error

**Status:** `proposed`  
**Id:** `dec_01KZPTGB10RJERCFAJR9C1R71B`

## Context

Reported from another project: `keel_create` with `type: "decision"` failed reproducibly with `read column number of decisions: Invalid column type Null`. A decision saved earlier the same morning had worked.

## Decision

A missing readable number is read as zero — "not yet assigned" — rather than as an error, and the write paths assign a real one. Migration 13 repairs rows already written.

## Reasoning

Every schema change opens a window. Migration 10 added `decisions.number` at 18:43:57 and backfilled everything that existed; it could not reach forward. At 18:45:21 — **84 seconds later** — a daemon that had the column but not the struct field inserted a decision, and the row got a NULL.

Reading that as a hard error was catastrophically out of proportion. One row with a NULL made **every** decision in that project unreadable, and because the idempotency check is a read, `create` failed too. So a single unnumbered row presented as "this artifact type is broken" rather than "this one row has no label".

Proportion is the principle: an unnumbered row costs its own label and nothing more. Zero already meant "not yet assigned" everywhere else in the codebase.

Reading NULL as zero alone would trade one failure for a worse one — two rows written back at zero would collide on the unique index — so the update path now assigns a number to anything holding zero, matching what create already did.

`tasks` had the identical shape and survived only because migration 6 landed in a quieter minute. Migration 13 covers both.

## Consequences

The general lesson, which is worth more than the fix: **a column added by a migration is NULL for every writer that has not been restarted yet.** Any read of a newly-added non-nullable-in-practice column has to tolerate that window, or the migration becomes an outage for whoever writes during it. The window here was 84 seconds and it still caught a real project.

## Reversible?

Yes. The lenient read is one function; the migration is a backfill that is a no-op on a clean store.

