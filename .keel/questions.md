# Open questions and risks

<!-- keel:generated questions prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from Keel — edits here are not saved.

## I maintained the tracker as prose all session and never touched a task row

`que_01KZKW33RGTJY86XYDTHSPMF0C` · risk · severity high

KB noticed it before I did: "I am not seeing the actual project task items getting updated from this chat."

He was right. Of 134 events in the store, 119 were `keel bootstrap`, 13 were `keel import`, and 2 were one-off `curl` calls. Zero task rows moved status in a session that shipped four features. I kept the tracker by hand in `product/STATUS.md`, imported it, and generated it back out — so the *prose* was accurate and the *data* was frozen at what bootstrap wrote at 16:11.

This is R-2 ("the agent doesn't write to it") happening live, to the agent that built the thing. Worth recording because it is evidence about the product, not about one session:

- The MCP surface was connected and working the whole time. Wiring was never the problem.
- The pull toward editing a markdown file is strong enough to beat a tool surface designed specifically to replace it, even with the tool in front of me and the project's own contract telling me to use it.
- It went unnoticed until a human looked at the app. Nothing in the system complains that no task row has changed while the commit log fills up.

Two candidate mitigations, neither built:
1. `keel generate --check` already fails on stale prose. A sibling check could fail when the event log shows no task mutation in a session that produced commits — cheap, and it converts a silent drift into a build failure.
2. The Phase 2 skill and hooks are the real answer, and they are exactly what the ten-session gate is meant to measure. This session is a data point that the gate matters and is currently failing when the agent is running without the plugin.

## The agent might simply not write to it

`que_01KZKMPW1RDD4SJ8MZXW8ZMKCK` · risk · severity high

If Claude has to be reminded every session, the whole thing fails. This is what Phase 2's gate measures, and it has not been run.

## How long should the 2025-11-25 handshake be carried?

`que_01KZKMPW0CDTNEYCYDJDT5N8PT` · question

TQ-11. Needed today, because that is what Claude Code sends. Worth revisiting once clients move on.

## Should BM25 live in DuckDB rather than Lance?

`que_01KZKMPVZRBGWRH4Z7Y381B21F` · question

TQ-10. Implemented in DuckDB because lance_hybrid_search's keyword half could not be characterised. The swap back is one module.

## Should idempotency_key be on all thirteen tables or only tasks?

`que_01KZKMPVZ3AQ0BNVV5BQKV3QY8` · question

TQ-9. Implemented on all thirteen. The one storage-format change made without KB, because the alternative silently breaks a v1 must-have for twelve types.

## How does a design image get into Keel from a Claude chat session?

`que_01KZKMPVYBQVZG4MSWFSKHCNTB` · question

There is no filesystem in chat. Cowork can send files and Claude Code can read them. Unsolved; blocks part of Phase 4.

## Should Keel ingest anything automatically, or only explicit writes?

`que_01KZKMPVXPKW1KXV9KMG36C6F8` · question

Working assumption: explicit writes only, except the GitHub webhooks in SPEC §9. Governs push and deployment_status behaviour, and the write-amplification risk.

## What is the retention policy on the event log?

`que_01KZKMPVX3SNJ82BQ4GA3DF9S5` · question

It grows forever. Keep everything, which is probably fine for a decade at this write volume, or roll up events older than a year into daily summaries.

## Where does the store live, and does ~/.keel get a git remote?

`que_01KZKMPVWG2SWPNH1RPD0P9569` · question

Working assumption: ~/.keel, local git, no remote. Low cost to get wrong — moving it is a config change.

