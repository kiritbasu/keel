<!-- specline:generated decision dec_01KZKMPVVAC6EZA35F1E87SC0C v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-15 — Keel's local REST API has more endpoints than the MCP surface has tools

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKMPVVAC6EZA35F1E87SC0C`

## Decision

Keel's local REST API has more endpoints than the MCP surface has tools.

## Reasoning

The nine-tool ceiling exists because a *model* chooses worse among forty tools than among nine (SPEC §6.1). That reasoning does not transfer to a UI, which knows exactly what it wants and would otherwise fetch everything and filter client-side. `/api/entities`, `/api/document/{id}` and `/api/graph/{id}` are UI-facing and thin. The MCP surface is untouched at nine.

## Reversible?

Yes.

