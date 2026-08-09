<!-- keel:generated decision dec_01KZKWMT3ZRNB06RMYBSTAKDV6 v1 2026-08-09T18:32:09Z
     source of truth is Keel — edits here are not saved -->
# Dev builds use debug = "line-tables-only" and debug = false for dependencies; the…

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT3ZRNB06RMYBSTAKDV6`

`B-11` · 2026-08-09

**Decision.** **Dev builds use `debug = "line-tables-only"` and `debug = false` for dependencies; the clippy gate drops `--all-features`.**

**Reasoning.** The vendored DuckDB's full debug info is enormous: `target/` reached **19 GB** and filled KB's disk mid-session (`ranlib: errno=28`). `--all-features` made it worse by building DuckDB a second time under a different feature set, while changing nothing — no workspace crate declares a feature. Line tables keep file-and-line in every backtrace, which is the part that matters; what is lost is stepping through DuckDB's C++ internals, which this project does not do. `product/CLAUDE.md`'s definition of done was amended to match. **Worth KB knowing separately: the machine is at 95% disk (327 GB of 373 GB used) independent of this repo.**

**Reversible?** yes

