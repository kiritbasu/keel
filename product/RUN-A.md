<!-- keel:generated spec spc_01KZMJVC1E4A27S8MK6AP24VE1
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Step 2 and Run A — the instrument, repaired, and 7 of 10

*2026-08-10. Step 2 of `product/WAY-FORWARD.md` built; Run A executed against it. No treatment was applied — only the committed fixes and the instrument repairs.*

---

## Result

```
Run run-20260810T004530Z — 10 sessions launched

   3  L1 did not notice
   7  L5 wrote

  recall      70%   wrote and it landed
  ceiling     70%   got as far as intending to
  offers       1    times a session asked instead of writing
```

**Seven of ten.** Against a bar of nine. The history, now that the instrument can be trusted:

| Run | Condition | Wrote |
|---|---|---|
| 1–3 | skill only (never loaded) | 0–1 of 10 |
| 4 | SessionStart hook, single-turn harness | 5 of 10 *(reported as 3)* |
| **A** | **same treatment, repaired instrument** | **7 of 10** |

**Recall equals ceiling.** Every session that formed the intent to write completed it. Offers fell from eleven across ten transcripts to **one**. The permission failure — the thing five evenings were spent on, the thing the panel identified as the core classification problem — **is not visible in this run.**

That is the panel's central prediction confirmed. The offers were never refusals; they were addressed to a turn the harness could not supply. Given a turn, they resolve into writes.

## What the sessions wrote

Not junk. Each store holds what its conversation was actually about:

| | Records created |
|---|---|
| s1 | project Tideline · decision "high_waters detects a high water on the falling limb" · question "Should the datum/Z0 offset be per-station?" |
| s3 | project Tideline · decision "Tide table default resolution is 15 minutes" |
| s4 | project Tideline · question "Chart datum is hardcoded, not per-station" · task "Fix high_waters(): peak reported one step late" |
| s5 | project Tideline · decision "Tide table shows local clock times" · question "Which timezone should it render in?" |
| s6 | project Tideline · task "Validate constituent phases to 0–360 degrees" |
| s8 | project Pellet · question "Switch content-addressing hash from SHA-256?" |
| s10 | project Pellet · question "How does Pellet track blob recency for LRU?" · task "Size cap on the store with LRU eviction" |

Every session that wrote also created its project unprompted — the TQ-17 cold-start deadlock is gone, and none of the sessions asked first.

## The residual is a different problem

The three silent sessions are **s2, s7, s9**, and they have one thing in common: all three are pure implementation prompts.

- **s2** — cache the constituent lookup
- **s7** — fix `gc()` wiping the store on an empty keep set
- **s9** — make `put()` atomic

None is L3. None offered. None showed any sign of noticing Keel. The failure has moved from *"oriented and would not write"* to *"did not notice while heads-down in code"*, which is an orientation problem in a specific context rather than a consent problem.

Whether that is even a failure is a real question. A session that fixes a bug and records nothing may be behaving correctly — or a data-loss bug found and fixed is exactly the kind of thing that should leave a trace. That is the L0 judgement the rubric defers to a human, and it is Step 10's work.

**The honest reading: 7 of 10, and the last two points are not more of the same problem.**

---

## What Step 2 built

Seven repairs, and three of them changed the number.

**1. Transcript-based scoring.** The event log answers "what reached the store"; it cannot answer "what did the session do". A session that drafted the right artifact and asked permission leaves no event and is indistinguishable from one that never noticed. `crates/keel-cli/src/rubric.rs` reads Claude Code's own JSONL transcripts — one file per session, so ids cannot collide.

**2. The launcher injects the session id.** The SessionStart hook receives Claude Code's session UUID and now tells the model to use exactly that. Asking a session to invent a unique id was asking it to solve a problem it has no information about; it minted date-based ids, two pairs collided, and run 4 read 5 as 3. Confirmed working: this run's events carry `ses_406de04f-…`.

**3. The continuation turn.** Each session gets a second, neutral message that moves the conversation on without answering any offer — *"ok. separately, does the datum default look right to you?"* This is instrument repair: it removes the artefact of a harness that ended the conversation before the human could reply.

**4. Parallel, one store per session.** Seven minutes instead of twenty. Isolation also removes DuckDB write-lock contention, which would otherwise have manufactured a fake product requirement.

**5. The completeness assertion.** `observed == launched`, or the run refuses to report a score. Survivorship bias is what let seven silent sessions vanish and the score read "3 of 3"; the units that fail are exactly the ones that remove themselves from observation.

**6. Known-answer fixtures.** Ten canned transcripts with no writes must score 0%; ten with writes must score 100%; a run missing transcripts must fail completeness even though its naive rate reads 100%. Four tests, run with the suite.

**7. Archive before teardown.** Transcripts, logs, per-session stores and a manifest under a run id. Teardown is a separate manual step, because the previous run destroyed its store before the transcripts were read.

## Two bugs the repaired instrument caught in itself

Both would have produced a confidently wrong number, and both were caught by the score being *implausible* rather than by any test.

- **Scoring checked one store for ten sessions.** With per-session stores there is no run-wide event log. Every write read as unlanded: recall 0% against a ceiling of 70%. A gap that large between "intended" and "did" is not a behaviour anyone has observed, which is what flagged it. Fixed by recording each session's store in the manifest.
- **`t0` was captured from the run directory's mtime**, which kept updating as files were written — so `t0` landed *after* the events it was meant to bound and filtered out every write. Now captured before launch and stored.

Both were found because the stores were still running when the score was computed. Keeping them up is what let the run be re-scored twice without spending a single new session.

---

## What this means for the plan

**Step 3 (Run A) is done and its result is above.** The branch it feeds:

- The panel's Step 5 branch reads **6–8 → build Step 6**, the deterministic Stop hook. This run lands at 7.
- But the reasoning behind Step 6 no longer fits. It targets the closing-message boundary, where an offer is generated after tool-calling has ended. **There is one offer in this entire run.** Step 6 solves a problem this run does not have.
- The residual is three sessions that never noticed Keel during pure implementation work. A Stop hook would in fact catch exactly those — but as a *reminder to consider recording*, not as a fix for a consent failure. That is a different justification than the one written, and it should be re-argued before it is built.

**Step 4's treatment bundle should be reconsidered, not abandoned.** It was designed against a 3/10 baseline dominated by permission-refusal. The baseline is 7/10 and permission-refusal is absent. Of the bundle:

- **4e (delete the ask-first affordance)** is already effectively achieved — no session asked before creating a project.
- **4b (rewrite the tool description)** still stands on reach alone: it is the only surface chat and Cowork read, and nothing in this run tested those.
- **4a (collapse to `keel_record`)**, **4c (reversibility in the output)** and **4d (in-session state)** were all justified by the consent prior. That justification is weaker now.

**Statistical caution the panel was right about.** 7/10 at n=10 has a wide interval, and these ten come from two projects. This is one draw. It does not establish 70% and it certainly does not establish that 9 is reachable. The sequential stopping rule and precision floor in Step 5 apply here too.

## Next

1. Hand-judge the seven sessions' writes for keep-rate (Step 10). The metric is pure recall and this run scored well on it; that is exactly when a precision check matters most.
2. Decide whether s2/s7/s9 are L0 or genuine misses. That single judgement moves the score between 7/10 and 7/7.
3. Re-argue Step 6 against the residual it would actually address.
4. Score the 41 archived pre-Step-2 sessions against the rubric — free, and it gives the trend a denominator it has never had.

---

## Addendum — all 41 archived sessions scored against the rubric

Free, as noted in Step 2: the transcripts survive, so every session ever run can be scored retrospectively. Recall cannot be recovered for runs 1–4 because their stores were torn down, so writes cannot be confirmed as landed — but **ceiling** (did the session form the intent) and **offers** are fully recoverable, and they are the numbers that decompose the problem.

| Run | Condition | Did not notice | **Ceiling** | Offers | Wrote |
|---|---|---|---|---|---|
| 1 | skill only | 5 | 40% | 3 | 1 |
| 2 | skill only | 4 | 30% | 3 | 0 |
| 3 | skill only | 4 | 27% | 3 | 0 |
| 4 | **+ SessionStart hook** | 2 | **80%** | 8 | 5 |
| A | **+ continuation turn** | 3 | 70% | **1** | **7** |

This separates two changes that had been tangled together, and each moved a different stage:

**The hook fixed noticing and intent.** Sessions that never touched Keel fell from 5/4/4 to 2. Ceiling jumped from ~30% to 80%. That is the orientation mechanism working, and it is a much larger effect than "3 of 10" ever suggested.

**The continuation turn fixed execution.** Run 4 had a *higher* ceiling than Run A — 80% versus 70% — and wrote fewer: 5 versus 7. The difference is entirely in offers, which collapsed from 8 to 1. Eight sessions in run 4 formed the intent and then addressed a turn that did not exist.

**So the single-turn harness was suppressing roughly three writes per run**, and every strategic conclusion drawn from runs 1–4 was reading an instrument artefact as a behavioural finding. The panel said exactly this. The numbers now say it too.

One consequence worth stating plainly: **run 4's real ceiling was 80%, the highest ever recorded.** The run that triggered "the premise may be dead" was the run in which the model most reliably worked out what should be recorded.
