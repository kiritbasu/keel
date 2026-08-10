<!-- keel:generated decision dec_01KZKWMTDQZPZJ46PCEWATF0XY v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-22 — projects gets a nullable status_path; a path claimed by both a document and the tracker…

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMTDQZPZJ46PCEWATF0XY`

## Decision

projects gets a nullable status_path; a path claimed by both a document and the tracker is reported, not resolved.

## Reasoning

The tracker is rendered from task and milestone rows, so no single artifact *is* `product/STATUS.md` the way the spec artifact is `product/SPEC.md` — the destination is a property of the project. Migration 4, additive and nullable. The collision case is real today: Keel's own `product/STATUS.md` is both an adopted prose document and the project's `status_path`. Rather than let whichever writer runs last win — which is how a file silently loses half its content — neither is written and the conflict is reported. The prose survives because it cannot be regenerated and the tracker can.

## Reversible?

Expensive — it is a schema column, though a nullable additive one.

