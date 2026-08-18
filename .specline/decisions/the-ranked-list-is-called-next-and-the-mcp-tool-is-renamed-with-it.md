<!-- specline:generated decision dec_01M09W33CT3J7KQ951QX56YS46 v1 2026-08-18T07:30:32Z
     source of truth is Specline — edits here are not saved -->
# B-85 — The ranked list is called next, and the MCP tool is renamed with it

**Status:** `accepted`  
**Id:** `dec_01M09W33CT3J7KQ951QX56YS46`

One concept has had two names, split by who was reading. The code and the digest called it **next** — `next.rs`, `NextUp`, `NextItem`, and the `## Next` heading a session reads first. Everything a person touched called it **ready** — `specline ready`, `specline_ready`, and the nav label.

**The decision.** It is called *next* everywhere. The page becomes "What's next", the CLI verb becomes `specline next` with `ready` kept as an alias, and the MCP tool becomes `specline_next`.

**Why next rather than ready.**

*Ready reads like a status.* Every tracker has a "Ready for dev" column, so the word arrives already meaning something else, and it invites "how do I move a task to Ready?" — which has no answer, because it is computed. This project already refuses a `blocked` status for that reason: being blocked is a fact about the graph, and holding it twice meant two facts that had to agree and did not. Ready is derived in exactly the same way.

*Ready names the filter; next names the question.* The nav's other entry is "What changed", which says what a reader will learn. "Ready" says only that the rows share a property. Since the grouping change the page's own first section, "Next up", described the page better than its title did.

**The tool rename is the part that needed agreement**, since the MCP surface is KB's to approve. It is a contract change, and tool names steer which tool a model reaches for. `specline_next` answers "what should I work on" more directly than `specline_ready`, which reads like a filter on a list. KB agreed on 2026-08-18.

**What was rejected.** Renaming the internals to `ready` instead, closing the split the other way. Cheaper, since nothing external moves, but it keeps the weaker of the two names and leaves the digest heading saying Next.

Renaming only the interface was rejected too: it widens the split rather than closing it, which is the thing worth fixing.

