<!-- keel:generated decision dec_01KZYASB3PX1BXA4Y37VP0D4XD v1 2026-08-13T19:50:55Z
     source of truth is Keel — edits here are not saved -->
# B-61 — Explicit writes only, and adopting an existing project is a two-layer backfill

**Status:** `accepted`  
**Id:** `dec_01KZYASB3PX1BXA4Y37VP0D4XD`

Resolves Q-6, 2026-08-13. KB chose the hybrid, for people adopting Keel on a project that already exists.

**The question's own exception has gone.** Q-6 recorded the working assumption as "explicit writes only, except the GitHub webhooks in SPEC §9". Those webhooks were dropped on 2026-08-11 (KEEL-45, `wont_do`) because there is no git remote — the integration was specified for a world this project does not live in. So Keel has no automatic ingestion at all, and the only path ever planned was deliberately removed. Explicit-only is not a choice being made here so much as a fact being written down.

**Backfill does not reopen it.** What Q-6 is about is *unattended* ingestion: something watching files, receiving webhooks, scraping commits, writing without anyone asking. A backfill is the opposite — a person runs it, once, on purpose. The property that matters is who initiates, not how many rows land. Three hundred artifacts written because somebody typed a command are more explicit than one row written by a webhook nobody remembered configuring.

**Two layers, because the two halves have different failure modes.**

*Mechanical.* `keel import` already does this: markdown files become versioned documents, idempotent, recording `mirror_path` so `keel generate` puts them back where they came from. Nothing is inferred — a file *is* a document. This is most of the volume and none of the risk. Its one gap is that it cannot be previewed, which matters more than it sounds: soft-delete-only means a bad backfill leaves permanent sediment, archived but present forever.

*Judged.* Tasks, decisions, glossary terms and milestones are not sitting in files waiting to be parsed. Deciding that an ADR is a decision, that a heading is a spec, or that a `TODO` is a task is a judgement, and a parser making it at scale gets it wrong at scale. That half is Claude reading the repository and writing through the MCP surface that already exists — no new code, and better at the only part that is hard.

**The hybrid's provenance is honest by construction, which is the strongest argument for it.** A parser writing hundreds of rows would need a way to mark them as derived rather than asserted — `Actor::System` exists for that and is currently unused — because Keel's whole value is that "we decided X" comes with who and when, and unattributed rows that look attributed dilute it by exactly their volume. The hybrid needs none of that machinery: imported documents carry `Actor::Human` truthfully, because a person wrote the file, and judged rows carry a real `session_id` because a real session made the call. Nothing is fabricated, so nothing needs flagging as fabricated.

**The risk that remains is volume, and it is the one Q-6 named.** `write-amplification` in its own body. A naive backfill turns every `TODO` into a task and every heading into a spec, and the digest — budgeted at 3–4k tokens and already over budget on this project — is the first thing every session reads. A backfilled project can be worse than an empty one. The standing instructions already say a project with forty trivial tasks that should be eight is worse than useless; the backfill workflow has to say consolidate, not transcribe, and be judged on what it left out.

Not doing: a `keel backfill <repo>` command that infers everything. It is the version that sounds most finished and is worst at the part that matters, and it would need the provenance machinery the hybrid avoids.

