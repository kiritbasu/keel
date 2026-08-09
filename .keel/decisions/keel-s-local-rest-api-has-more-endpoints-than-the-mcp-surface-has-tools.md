<!-- keel:generated decision dec_01KZKMPVVAC6EZA35F1E87SC0C v1 2026-08-09T18:07:39Z
     source of truth is Keel — edits here are not saved -->
# Keel's local REST API has more endpoints than the MCP surface has tools

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVVAC6EZA35F1E87SC0C`

## Decision

UI-facing endpoints are added freely; the MCP surface stays at nine tools.

## Reasoning

The nine-tool ceiling exists because a *model* chooses worse among forty tools than among nine. That reasoning does not transfer to a UI, which knows exactly what it wants and would otherwise fetch everything and filter client-side.

