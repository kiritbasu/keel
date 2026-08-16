<!-- specline:generated decision dec_01KZSKKGWMG73H09G4Q20XMDSZ v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-52 — Taking the payload out of a tool result is one named function, not two lines

**Status:** `accepted`  
**Id:** `dec_01KZSKKGWMG73H09G4Q20XMDSZ`

## Context

`dispatch` returns the MCP `tools/call` envelope — `{content, structuredContent, isError}`. Three surfaces are not speaking MCP and need what is inside it: the CLI's daemon call, the CLI's fall-back-to-the-store, and the daemon's own `/api` responses. Each had its own copy of the same two lines, and the CLI's fallback did not have them at all.

The result was KEEL-133. `keel ready` printed "nothing ready" whenever no daemon was listening, for as long as that path has existed.

## Decision

`keel_mcp::structured` and `keel_mcp::summary_text`, used by all three.

## Reasoning

Forgetting the unwrap is invisible. The envelope is a perfectly good JSON object, so `.get("ready")` on it returns `None` rather than failing, and every renderer here reads a missing field as an absent value — which for a list means an empty list, and an empty list has a sentence of its own that sounds like an answer. The failure mode is the one the standing instructions single out for graph direction: a plausible, calm, empty result.

Two lines copied three times is not worth naming for its own sake. Two lines whose absence is undetectable is.

The CLI now has one `run_tool` rather than a copy per command, and the unwrap sits inside `directly` — so it is not something a new caller has to remember, which is the part that failed.

