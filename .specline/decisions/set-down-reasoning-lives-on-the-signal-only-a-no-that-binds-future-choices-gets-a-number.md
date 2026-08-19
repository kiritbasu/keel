<!-- specline:generated decision dec_01M0CNTD0B3TA4SJF259QA2YM5 v1 2026-08-19T09:32:57Z
     source of truth is Specline — edits here are not saved -->
# B-91 — Set-down reasoning lives on the signal; only a no that binds future choices gets a number

**Status:** `accepted`  
**Id:** `dec_01M0CNTD0B3TA4SJF259QA2YM5`

## Decision

When a signal is set down, the reasoning is written **on the signal itself**. It becomes a numbered decision only when it is the kind of no that constrains what gets built next. KB's call, 2026-08-19: *"reasoning lives on the signal, promote only when it binds future choices."*

This refines one clause of [B-90](dec_01M0CNH5V9B58M5J50E8ZM76Y3), which said flatly that a rejection is a `decision`. B-90 otherwise stands — this narrows where the reasoning lands, and changes nothing about the lifecycle, the four artifacts or the naming. A new decision rather than an edit, because B-90 is accepted and accepted decisions are superseded rather than amended; and `references` rather than `supersedes`, because one sentence is being sharpened, not the argument replaced.

## Why this is safe, and it is worth being specific

The worry that made this worth asking was that reasoning parked on a `feedback` row would be second-class — written once, filed somewhere nobody looks, and functionally lost. That worry does not survive contact with the schema.

**A signal's body is a document, and documents are indexed.** `feedback` carries `current_doc_version` like every other prose-bearing type, so a set-down reason is a revision in `documents` with an embedding, reached by both halves of hybrid search on equal footing with a spec or a decision. It is not a comment field. Somebody asking "did we ever consider X" four months from now finds it by the same search that finds everything else, and the durable-no property B-90 turns on is fully intact at the default tier.

So the two tiers are not "findable" and "not findable". They are both findable; the number is about **standing**, not retrievability.

## The test for promotion

A numbered decision is for a no that binds. "We are not building a public request portal" constrains the next twenty choices and belongs in `product/DECISIONS.md` where somebody reads it before proposing one. "Not this, it is a bad idea" constrains nothing and belongs on the signal.

Claude proposes which tier at the moment of triage and KB overrules, like every other judgement in this phase. Getting it wrong in the cheap direction costs a search away; getting it wrong in the expensive direction costs the decision log its property that everything in it matters — which is the asymmetry that decides the default.

## What this protects

There are 90 decisions and every one is load-bearing. A rule minting one per rejected idea would put "no thanks" entries next to the storage-engine replacement inside a month. The decision log is valuable precisely because its entries all matter; a log nobody trusts to be dense is a log nobody reads, and then the binding nos stop binding too.

