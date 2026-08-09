# Keel — Decision log

> Maintained by Claude Code. Every non-obvious choice made during development gets a row.
> Architectural decisions made *before* development are in `product/SPEC.md` §13 (D-1 … D-11, including D-2b — twelve rows) — those are settled and are not repeated here.

**Why this exists:** in six months neither KB nor a fresh Claude session will remember why a library was chosen or why an approach was abandoned. One line written now saves an hour of archaeology later. It's also the seed data for Keel's own `decisions` table at the Phase 1 dogfooding switch.

---

## Format

| ID | Date | Decision | Reasoning | Reversible? |
|---|---|---|---|---|

- **ID**: `B-1`, `B-2`, … (B for build-time, to avoid colliding with the spec's D-series)
- **Reasoning**: one or two sentences on *why*, including what was rejected. "Chose X" without "over Y because Z" is not useful.
- **Reversible?**: `yes` / `no` / `expensive`. If `no` or `expensive`, it should probably have been a question for KB first.

---

## Decisions

| ID | Date | Decision | Reasoning | Reversible? |
|---|---|---|---|---|
| — | — | *(none yet)* | | |

---

## Findings from dependency verification (task P0-2)

Record what was actually true when checked, with the date and the source. `product/SPEC.md` was written in August 2026 from documentation, not from running code, and several claims sit on fast-moving ground.

| Claim in SPEC | Verified? | What's actually true | Date checked | Source |
|---|---|---|---|---|
| Lance extension available for current DuckDB | | | | |
| `ATTACH … (TYPE lance)` syntax as written in §5 | | | | |
| `lance_hybrid_search()` signature and behaviour | | | | |
| DuckPGQ still incompatible with the Lance-supporting DuckDB line | | | | |
| Quack status and timeline (§7.1) | | | | |
| Current MCP spec version and transport model | | | | |
| MCP `Mcp-Method` / `Mcp-Name` header names (§6) | | | | |
| DCR deprecated in favour of CIMD (§11) | | | | |
| `fastembed-rs` version, model availability, ONNX dependency | | | | |

**If any of these are wrong in a way that invalidates part of the spec, stop and ask KB before building on it.** A wrong storage layer is expensive to unwind; a wrong MCP detail is not.

---

## Reversals

When a decision turns out to be wrong, add a row here rather than editing the original. Knowing something was tried and abandoned is as useful as knowing what was chosen.

| Original | Date reversed | What replaced it | What we learned |
|---|---|---|---|
| — | — | — | — |
