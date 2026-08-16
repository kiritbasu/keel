<!-- specline:generated decision dec_01KZPNN6TBH77592TR7VN6DD4K v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-43 — An accepted decision can be corrected; the revision chain is the guard

**Status:** `proposed`  
**Id:** `dec_01KZPNN6TBH77592TR7VN6DD4K`

## Context

`keel_update` refused any content change to a decision whose status was `accepted` — SPEC §3.2, enforced in `keel-core` because the schema cannot express it. The remedy it named was to create a new decision linked with `supersedes`.

`keel_write_doc` was never subject to it. It replaced an accepted decision's entire body without complaint, and did so twenty-five times on 2026-08-10 while the reasoning was migrated out of the prose table into the rows.

## Decision

The guard is removed. An accepted decision can be edited, and the revision chain is what makes the edit safe.

## Reasoning

It sat on the wrong door. A title is a label; the body is the argument, and the argument *is* the decision. Guarding the label while leaving the argument writable stopped the harmless edit and permitted the harmful one.

The concrete cost was visible: seven decision titles had been truncated at roughly eighty characters by whatever imported them — B-8 read `Surface carries five values, not four: chat \` — and could not be corrected. They were invisible while the prose table carried the real titles, and became the headings of the generated log the moment the register was unified. Correcting a transcription defect is not amending a decision, but a write guard cannot tell the two apart. The revision chain can, after the fact, which is when the question is actually asked.

What replaces it was already there: every change is an attributed revision with a diff and an event naming the field. A reworded decision is visible rather than prevented, and *visible* is the property that was wanted — "supersede instead of editing" is advice about how to think, and it survives as advice.

The old test asserted only that the error named the remedy. It never asserted that the body it was protecting was protected, which is part of how the gap lasted.

## Consequences

Retitling a decision changes its mirror slug, and `generate` never deletes, so seven orphaned files under `.keel/decisions/` had to be removed by hand. A generated file that survives a rename reads as current, which is its own small instance of the disease this register unification was fixing. Recorded as TQ-28.

## Reversible?

Yes — the guard was six lines. Re-adding it would re-break title correction, so anything that reinstates it should guard the body too or not at all.

