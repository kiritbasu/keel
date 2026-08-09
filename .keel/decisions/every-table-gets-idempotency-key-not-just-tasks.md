<!-- keel:generated decision dec_01KZKMPVR92GNRQXTE8836ZD1E v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Every table gets idempotency_key, not just tasks

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVR92GNRQXTE8836ZD1E`

## Context

SPEC §7.2 and REQ-7 say every create is idempotent; §3.2 gives the column only to tasks.

## Decision

All thirteen tables.

## Reasoning

The alternative silently drops idempotency for twelve types including projects — the one type where duplicates are called out as the failure that ruins the aggregate view.

## Consequences

Marked PROVISIONAL; raised as TQ-9 because adding a column is a storage-format change and those are KB's call.

