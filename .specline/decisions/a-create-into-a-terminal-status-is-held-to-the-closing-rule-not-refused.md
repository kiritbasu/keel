<!-- specline:generated decision dec_01M04J2FKE9S4F3H7HDFRKM1NB v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-79 — A create into a terminal status is held to the closing rule, not refused

**Status:** `accepted`  
**Id:** `dec_01M04J2FKE9S4F3H7HDFRKM1NB`

A task that arrives already closed — `keel_create(status: "done")` — now has to carry what a close carries: a reason, a message, and evidence when the reason is `done`. The alternative was to refuse a terminal create outright, which is what KEEL-217 recommended when it was filed.

Refusing was rejected for a plain reason: it would have made things illegal that this repository already does and has nowhere else to do. `keel bootstrap` transcribes Phases 0–3 as rows that were finished before Keel existed; `keel fixture` seeds a demo corpus with `done` and `wont_do` rows; adopting a finished backlog is the same shape and is the whole of the `keel-adopt` flow. The argument on the row was that back-filling is what `keel import` is for — but `keel import` writes document revisions, not task rows, so it cannot back-fill a closed task at all. A rule whose escape hatch does not exist is a rule that gets `--force`d, or worked around.

Two things follow, and both are in the code:

- `closed_at` is stamped on the way in **unless the caller supplied one**. A backfill knows the real date and the store does not; overwriting it with `now` would date the whole of Phases 0–3 to the afternoon someone ran the import.
- A claim is released on a terminal create, the same as on the transition, so the two doors cannot disagree about what a closed row looks like.

The cost, accepted: a legacy-shaped row — terminal, no reason — can no longer be constructed through any door. Two tests needed one and now build it by closing properly and stripping the field afterwards. That is the same shape `lint.rs` already used for a row with no summary, and the lint that reports the hundred and ten real ones is unaffected.

