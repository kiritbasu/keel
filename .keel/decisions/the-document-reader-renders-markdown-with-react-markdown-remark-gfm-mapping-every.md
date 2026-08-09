<!-- keel:generated decision dec_01KZKWMT7GFNZBYEQBV44NPY4R v1 2026-08-09T18:32:09Z
     source of truth is Keel — edits here are not saved -->
# The document reader renders markdown with react-markdown + remark-gfm, mapping every…

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT7GFNZBYEQBV44NPY4R`

`B-19` · 2026-08-09

**Decision.** **The document reader renders markdown with `react-markdown` + `remark-gfm`, mapping every element by hand.**

**Reasoning.** The bodies were being shown as preformatted text, which made a real spec unreadable — the point of storing it. `react-markdown` over a string-to-HTML library because it does **not** render raw HTML by default: document bodies are written by a model, arrive from the store, and are displayed in an app served from the same origin as the daemon, so an injected `<script>` would be same-origin. Elements are mapped explicitly rather than pulling in a typography plugin, on the same reasoning as B-14. Tables get their own scroll container — the decision log and status tracker are almost entirely tables and would otherwise force the page wide.

**Reversible?** yes

