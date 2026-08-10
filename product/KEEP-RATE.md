<!-- keel:generated spec spc_01KZN3VBADSJ3Z7ZTVVCTVPCJ7
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Step 10 — hand-judge of 30 writes for keep-rate

*2026-08-10. Every artifact created in Runs B and C, judged one at a time.*

**The bias, stated first.** KB asked me to do this. The writes came from sibling sessions of the same model, and I built the harness and have a stake in the number. That is narrower than marking my own homework but it is not zero, and the panel wanted someone else for exactly this reason. Read the per-row judgements rather than the headline; they are checkable.

---

## Result

**39 create calls → 30 distinct artifacts → 26 worth keeping. Keep-rate 87%.**

Nine calls were exact-duplicate retries within a session, deduplicated by the idempotency key. REQ-7 working on organic traffic.

| Verdict | n | |
|---|---|---|
| Keep | 26 | records something that became true, with enough context to act on later |
| Drop | 4 | see below — two kinds, both instructive |

## The four drops

**1 & 2 — speculative validation tasks (B6, C6).** Both sessions were asked *"constituent phases should be validated to 0–360, nothing checks that today"* and each produced **three** tasks: the phase validation asked for, a guard against `step ≤ 0`, and validation of amplitude and speed.

The `step ≤ 0` guard I keep — it is a genuine infinite loop, and finding a hang while you are in the file is what you want. The amplitude/speed task I drop: nobody asked, nothing prompted it, and it is the third item generated from one request. That is R-6 write amplification, and this is what it looks like in practice — not forty junk tasks, but a third plausible one that nobody will ever prioritise.

**3 — C2, `"Which 'constituent lookup' caching did the ask refer to?"`** A clarifying question addressed to the user, recorded as project knowledge. The session was right that the premise did not match the code — `CONSTITUENTS` is a static dict and `height()` does not rebuild it — and saying so in the reply was correct. Storing it as an open question is not: six months on it records a moment of confusion about a prompt, not anything true about the project. **Mis-typed rather than wrong.**

**4 — C10, and this is the serious one.** Title: *"Size cap with LRU eviction — how to reconcile with append-only (D-9) and no access-time tracking?"*

**There is no D-9 in Pellet.** D-9 is a *Keel* decision — soft delete only. The Pellet store was empty when that session started; it created the project itself. The body goes on to reference `created_at` columns that a 30-line JavaScript blob store does not have.

So the artifact **fabricates a cross-reference to a decision that does not exist in the project it was filed under.** The underlying question is real and good — LRU needs access recency and the store records creation time only — but a reader who follows that citation finds nothing, and a store whose references cannot be trusted is worse than one with fewer rows. This is precisely the failure a recall metric cannot see: the write happened, it looked substantial, and it is subtly poisoned.

## What the recall metric would never have shown

**Near-duplicate titles defeat the idempotency key.** Run B produced *"Validate constituent phases to 0–360 degrees"*; Run C produced *"Validate constituent phases to 0–360"*. The key is a hash of the normalised title, and normalisation lowercases and collapses whitespace — it does not know those are the same task. Separate stores per session hid this completely.

**In a real shared store, ten sessions across one project would produce near-duplicate rows that idempotency does not catch.** That is UC-8's failure — the one the PRD calls most damaging — arriving at the task level rather than the project level, where nobody was watching for it. The gate scores it as a success both times.

## What is genuinely good, because precision cuts both ways

- **Typing is right.** The harbourmaster call became `feedback`, the resolution choice became a `decision`, unresolved safety concerns became `question`s, and the blake3 question stayed a question rather than being recorded as a decision nobody made.
- **Several rows are things nobody asked for and everybody would want.** C7's `get(key)` path-traversal finding, B4's *"a wrong chart datum is a silent safety failure"*, C9's *"get() never verifies the digest"*. These are the sessions noticing something real while in the code, which is the entire premise of the product.
- **Bodies carry evidence, not restatement.** B2 quantifies the hot path (672 `height()` calls per window); C9 names the file and line and the exact crash window. These are rows a person could act on cold.

## Recommendations

1. **Do not treat 87% as the number.** It is one judge, an interested one, on 30 rows from two projects and ten fixed prompts.
2. **The near-duplicate title problem is the highest-value fix here**, and it is invisible to the current gate. Either widen normalisation, or have `keel_create` search for a similar title in the project and return the existing artifact with `created: false`. Filed separately.
3. **Fabricated cross-references need a check.** The lexical orphan-ID check the panel proposes for composed documents (Step 9) would catch exactly this if applied to artifact bodies: any `B-n`/`D-n`/`TQ-n` mention must resolve to a live artifact in the same project.
4. **An independent judge still matters.** If the keep-rate is going to gate anything, someone who did not build the harness should re-score these thirty. The rows are archived and the judgements above are per-row, so it is an hour of work and it is checkable against mine.
