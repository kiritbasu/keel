<!-- keel:generated decision dec_01KZKMPVS9XTWRN7303BPY0F18 v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Serve MCP 2025-11-25 alongside 2026-07-28

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVS9XTWRN7303BPY0F18`

## Context

Claude Code 2.1.185 opens with the legacy initialize handshake and declares 2025-11-25. A daemon speaking only the current revision reported "Failed to connect".

## Decision

Serve both.

## Reasoning

A daemon that only speaks the newest spec is unusable with the client this product exists to serve, which would make Phase 2's gate impossible to attempt. The spec makes backward compatibility a MAY; here it is the difference between working and not.

## Consequences

Mirrored headers are required only of a 2026-07-28 caller; resultType goes only to clients whose revision defines it. Flagged as TQ-11.

