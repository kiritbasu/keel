<!-- keel:generated decision dec_01KZKWMT28K0HMJ1Y5JQ16TT8T v2 2026-08-10T20:25:03Z
     source of truth is Keel — edits here are not saved -->
# B-8 — Surface carries five values, not four: chat, cowork, code, ui, cli

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT28K0HMJ1Y5JQ16TT8T`

## Decision

Surface carries five values, not four: chat \| cowork \| code \| ui \| cli.

## Reasoning

SPEC §3.1's audit-block comment lists four; §6.5 separately names `cli` as a fixed sentinel for the command line. The two passages disagree and something had to give. Five is the reconciliation — `keel-cli` writes fixtures and restores backups, and those writes need an honest surface rather than a borrowed `ui`. The column is a bare `VARCHAR` with no check constraint, so this costs nothing at the storage layer. Raised with KB as TQ-8 rather than treated as settled.

## Reversible?

Yes.

