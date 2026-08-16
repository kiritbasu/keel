<!-- specline:generated decision dec_01KZNW724SBG1NFAWDZ9CR66DN v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-40 — Readable identifiers are composed, never stored

**Status:** `accepted`  
**Id:** `dec_01KZNW724SBG1NFAWDZ9CR66DN`

## Context

Phase 6 adds `KEEL-42` alongside `tsk_01KZKW28CS4Q1WSB0D95B2A01G`. The obvious implementation is a `ref` column on `tasks` holding the composed string, written once at creation.

## Decision

**Store the two halves — `projects.key` and `tasks.number` — and compose the label at every point of use. Nothing anywhere stores `KEEL-42`.**

## Reasoning

A stored composite is a denormalisation whose invalidation nobody owns. Re-keying a project — which the key being editable makes a legitimate operation — would require rewriting every task row, and any that were missed would go on displaying the old prefix while resolving under the new one. That is the failure this project keeps meeting in other clothes: something that looks right and is quietly wrong.

Composing costs a project lookup at the surfaces that do not already hold the project, and every surface that renders more than one task already holds it: `ProjectLine` carries the key, so the digest, the board, the detail view and the tracker all have it to hand. The one place it is genuinely per-row is `keel_get`'s summary, where it is a point lookup on a table with a handful of rows.

Two consequences that took some deciding:

**A number is never reused, even after an archive.** `MAX(number) + 1` counts archived rows. If a number were handed on, `KEEL-1` would keep resolving and silently start meaning a different task — and every note, commit message and conversation that used it would be wrong with nothing to say so.

**The uniqueness index is on `upper(key)`.** References resolve case-insensitively, so `KEEL` and `keel` must be one identifier. A plain unique index would have permitted both as separate projects, leaving the lookup to pick one arbitrarily.

## Reversible?

Adding a cached column later is easy. Removing one that has drifted is not, which is the asymmetry the decision turns on.

