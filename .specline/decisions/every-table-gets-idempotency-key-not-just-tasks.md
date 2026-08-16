<!-- specline:generated decision dec_01KZKMPVR92GNRQXTE8836ZD1E v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-10 — Every table gets idempotency_key, not just tasks

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVR92GNRQXTE8836ZD1E`

## Context

SPEC §7.2 and REQ-7 say every create is idempotent; §3.2 gives the column only to tasks.

## Decision

Every table gets idempotency_key, not just tasks.

## Reasoning

SPEC §7.2 and PRD REQ-7 say *every* create is idempotent, but §3.2 only gives the column to `tasks`. Honouring the requirement means honouring it everywhere; the alternative silently drops idempotency for twelve of thirteen types, including `projects` — the one type where duplicates are called out as the failure that ruins the aggregate view (UC-8, REQ-8). Marked `PROVISIONAL` and raised as TQ-9, because adding a column is a storage-format change and those are KB's call.

## Consequences

Marked PROVISIONAL; raised as TQ-9 because adding a column is a storage-format change and those are KB's call.

## Reversible?

Expensive — it is a schema column.

