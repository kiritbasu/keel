<!-- keel:generated spec spc_01KZMVA363QPEVBT2TJK7CF827
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Step 6 and Runs B and C — the Stop hook, and 18 of 20

*2026-08-10. Step 6 built and measured twice. KB confirmed that s2, s7 and s9 in Run A were genuine misses rather than sessions with nothing worth recording, which is what made this the right next thing to build.*

---

## Result

| Run | Condition | Wrote | Offers |
|---|---|---|---|
| 1–3 | skill only (never loaded) | 0–1 of 10 | 3 each |
| 4 | SessionStart hook, single-turn harness | 5 of 10 | 8 |
| A | repaired instrument, no new treatment | 7 of 10 | 1 |
| **B** | **+ Stop hook** | **9 of 10** | 1 |
| **C** | **confirmatory, unchanged** | **9 of 10** | 1 |

**Pooled: 18 of 20. Point estimate 90%, 95% CI [69.9%, 97.2%].**

The criterion — *"across 10 unprompted sessions, Claude writes to Keel in ≥9"* — **was met twice in succession, on independent draws.**

## The hook did exactly and only what it was built to do

Run B fired the Stop hook in **sessions 2, 7 and 9 — precisely the three that missed in Run A**, and in no others. It converted two of the three: s2 and s9 each made five or six Keel calls after the nudge and wrote. It stayed silent for the seven sessions that had already recorded, which is the constraint that keeps it from becoming noise a user disables.

Run C fired it in four sessions and converted three.

That specificity is the result worth trusting more than the score. The hook is not raising the number by nagging everyone; it is speaking to the exact population that was failing and staying out of the way otherwise.

**s7 is the one it could not reach.** It received the nudge and answered the user's follow-up question instead. That is a real remaining miss, not an instrument artefact.

## Precision — because a recall metric is least trustworthy when recall is high

Every artifact Run B created, from the archived transcripts:

```
decision 2 · question 5 · task 11 · feedback 1     19 create calls
```

Roughly fifteen distinct artifacts across nine sessions — about 1.7 each. Not shredding, and the typing is right: the harbourmaster complaint became `feedback`, the resolution choice became a `decision`, the unresolved safety concern became a `question`.

Two things worth naming rather than glossing:

- **Four sessions called `create` twice with an identical title.** Idempotency deduplicated them, so the store is clean — REQ-7 doing precisely its job on organic traffic for the first time. But the model retrying suggests the success response is not clearly acknowledging the write.
- **s6 produced three tasks from a prompt about one validation** — guard `step ≤ 0`, validate amplitude, validate phases. Two of those were not asked for. That is mild write-amplification (R-6), the first organic instance of it, and it is the kind of thing that compounds.

## What this does not establish

**The criterion is met. Phase 2 is not closed, and I am not closing it.**

- The panel is explicit: *"≥9 — Phase 2 closes, but only under a sequential stopping rule and a precision floor. Not on a single 9/10 draw."* Two draws is better than one, and the pooled lower bound is still **69.9%** — well under 90%. Twenty sessions cannot establish a 90% rate.
- **The precision floor does not exist yet.** Step 10 requires hand-judging ~20 writes for keep-rate before anything that raises write frequency ships, and says not to delegate that to an LLM judge until ~30 sessions are hand-labelled with measured agreement. The review above is mine, not an independent one, and the metric is pure recall — exactly the condition under which a recall-maximiser scores well while making the store worse.
- **Twenty sessions, two projects, ten prompts.** The effective sample is much smaller than twenty, and the prompts are fixed. Nothing here tests a different codebase, a longer conversation, or a surface other than Claude Code — and chat and Cowork have neither hook.

## What the numbers say about the mechanisms

Each intervention moved a different stage, and the decomposition is now clean across six runs:

- **The SessionStart hook fixed noticing and intent.** Ceiling from ~30% to 80%; sessions never touching Keel from 5/4/4 down to 2–3.
- **The continuation turn fixed execution.** Offers collapsed from 8 to 1; run 4's 80% ceiling had converted to only 5 writes, Run A's 70% converted to all 7.
- **The Stop hook fixed the residual.** It reaches the sessions that never noticed — heads-down implementation work, where the session-start digest is thousands of tokens back with no salience at the moment the work ends.

Three different failures, three different fixes, and none of them was the wording change five evenings went into.

## Next

1. **KB's call on closing Phase 2.** The criterion is met twice; the precision floor is not built. Those are separable decisions.
2. **The Step 10 hand-judge**, properly — 20 writes, someone other than the agent that produced them.
3. **s7's failure mode** is now the only known one: the nudge arrives, and the user's follow-up wins. Worth one look at whether the Stop reason should survive a subsequent user turn.
4. **Neither hook exists in chat or Cowork.** Everything above measures Claude Code only. That is the largest untested surface, and TQ-21's surviving item — rewriting the tool description — is the only proposal that reaches it.
