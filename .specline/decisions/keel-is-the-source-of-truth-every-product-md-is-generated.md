<!-- specline:generated decision dec_01KZKWMT9M8EJQM7TJDZH8KX22 v2 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-20 — Keel is the source of truth; every product/*.md is generated.

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMT9M8EJQM7TJDZH8KX22`

## Decision

Keel is the source of truth; every product/*.md is generated. A prose artifact records the repository file it *is*, as mirror_path, and generation writes its body there verbatim.

## Reasoning

KB's call, made directly. The alternative that was running — repo files authoritative, `keel import` keeping Keel in step — worked, but it left two copies that agree only as long as someone remembers to run the import, and the failure mode is silent. Verbatim rather than re-rendered: these documents carry their own heading and front matter, and injecting a generated preamble would corrupt a file written to be read whole. The banner is an HTML comment for the same reason — invisible in every renderer, and harmless at the top of `product/CLAUDE.md`, which Claude Code loads verbatim on every session. Adopted files are excluded from the `.keel/` mirror, so no document has two homes.

## Reversible?

Yes — the files are on disk and in git; deleting the `mirror_path` values reverts to the mirror's slugged layout.

