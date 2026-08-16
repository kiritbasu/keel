<!-- keel:generated decision dec_01KZKMPVS9XTWRN7303BPY0F18 v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-17 — Serve MCP 2025-11-25 alongside 2026-07-28

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVS9XTWRN7303BPY0F18`

## Context

Claude Code 2.1.185 opens with the legacy initialize handshake and declares 2025-11-25. A daemon speaking only the current revision reported "Failed to connect".

## Decision

The daemon serves 2025-11-25 as well as 2026-07-28.

## Reasoning

Found the moment KB pointed a real client at it: `claude mcp list` said "Failed to connect". Captured the wire traffic — **Claude Code 2.1.185 opens with the legacy `initialize` handshake and declares `2025-11-25`**, and sends none of the mirrored headers the current revision requires. A daemon that speaks only 2026-07-28 is unusable with the client this entire product exists to serve, which would have made Phase 2's gate impossible to even attempt. The spec's backward-compatibility section makes this a MAY; here it is the difference between working and not. `initialize`, `notifications/initialized` and `ping` are answered; `Mcp-Method`/`Mcp-Name` are required only of a 2026-07-28 caller; `resultType` and `_meta.serverInfo` are sent only to clients whose revision defines them. 2025-06-18 and 2025-03-26 are served the same way — they differ from 2025-11-25 only in ways the tool surface never touches. **SPEC §6's opening line is now wrong** and is flagged as TQ-11.

## Consequences

Mirrored headers are required only of a 2026-07-28 caller; resultType goes only to clients whose revision defines it. Flagged as TQ-11.

## Reversible?

Yes.

