<!-- specline:generated spec spc_01KZPDVA3THNZG533KZZ6772JX
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# The gate — what it measured, and why it is frozen

*2026-08-10. One page in place of six. Frozen by KB's decision, with the code kept and the runs stopped.*

---

## The question

Phase 2's gate asked one thing: **will a coding agent write to Specline without being told to?**

Everything else is downstream of it. The desktop app is a read surface by decision, so if the agent does not write, nothing does — a store that requires prompting is a filing cabinet with extra steps. The criterion KB set was deliberately hard: **nine of ten unprompted sessions record at least one artifact worth keeping.**

## What happened, in order

| Run | Score | What it was actually measuring |
|---|---|---|
| 1–3 | 1, 0, 0 | A harness that never loaded the skill. |
| 4 | reported 3 of 10 | Ids collided; two sessions were read as one. **The true figure was 5 of 10.** |
| — | *validity audit* | Zero build. Re-scored run 4 from the archived transcripts and found the instrument, not the agent, was the variable. |
| A | 7 of 10 | The first run against a repaired instrument. No treatment applied. |
| B, C | 9 and 9 | **Pooled 18 of 20. Point estimate 90%, 95% CI [69.9%, 97.2%].** |

The trend everyone was reading as "the agent will not write" — 1, 0, 0, 3 — was in fact 1, 0, 0, 5 against an instrument that could not see half of what happened.

## The finding that outlasts the number

**The harness was single-turn.** `claude -p … </dev/null`: one prompt, one response, process exits. Sessions formed the intent to write, said they would, and addressed a next turn the harness could architecturally not supply. The write was not refused. It was scheduled for a turn that never came.

Offers fell from eleven across ten transcripts in run 4 to **one** in Run A. So the single-turn harness was suppressing roughly three writes per run, and every strategic conclusion drawn from runs 1–4 was reading an instrument artefact as a behavioural finding. Five evenings went into fixing a permission-refusal problem that the repaired instrument shows was never there.

Two more, both of which changed how anything here gets measured:

**Survivorship bias is the default failure of a run harness.** Seven silent sessions vanished from a run that then reported "3 of 3" — the units that fail are exactly the ones that remove themselves from observation. The instrument now asserts `observed == launched` and refuses to report a score otherwise.

**Recall was never the risk it looked like.** Once sessions could reach a second turn, recall equalled ceiling: every session that formed the intent completed it. What remained was precision, which the gate could not see at all.

## What the hand-judge found that the score could not

Thirty-nine `create` calls, thirty distinct artifacts, twenty-six worth keeping — **87%**, by one interested judge, which is why the per-row judgements were written down and archived rather than the headline.

Two of the four drops are failures the recall metric scores as *successes*, and both turned out to be real:

- **Near-duplicate titles defeat the idempotency key.** Run B wrote *"Validate constituent phases to 0–360 degrees"*; Run C wrote *"Validate constituent phases to 0–360"*. The key hashes a normalised title, and normalisation lowercases and collapses whitespace — it does not know those are the same task. One store per session hid it completely; a real shared store would have accumulated the pair. Fixed: `create` now falls back to a similarity check on overlap **plus containment**, so only added words merge and substituted ones never do (KEEL-65).
- **One artifact fabricated a cross-reference.** A row filed under a different project cited "D-9", which is a *Specline* decision, and referenced columns that project's storage does not have. A reader following that citation finds nothing, and a store whose references cannot be trusted is worse than one with fewer rows. Partly addressed: `fsck` now resolves `B-n`/`D-n`/`Q-n`-style citations in artifact bodies against live entities in the same project, and found six genuine dangling references on the first scoped run (KEEL-66). **It does not catch the case that motivated it** — no project titles artifacts `D-n`, because Specline's own decisions live as a numbered table inside a prose document rather than as rows, so a lexical check cannot tell "cites a convention documented in prose" from "cites nothing". That is the two-decision-registers problem, and it is why unifying them is worth doing.

The gate scored both of those writes as successes. That is the argument for hand-judging a sample at least once: a recall metric cannot see a row that is confidently wrong.

## Why it is frozen

KB's call, 2026-08-10: keep the code, stop running it. Three reasons, in the order that matters.

1. **The criterion is met**, on two consecutive independent draws, with the mechanism understood rather than merely observed. Phase 2 closed on 18 of 20.
2. **Phase 7 changed the surface the gate measured.** The `SessionStart` hook, the `Stop` hook, the skill's wording and the tool descriptions have all moved since Run C. A seventh run would measure a different instrument against the same bar and could only muddy the record.
3. **The cost was wrong for the return.** Seven runs and roughly 11,700 words of documentation — more than the PRD and the SPEC together — for one number whose 95% interval is twenty-seven points wide. This is a project with one user and a few thousand rows.

## What survives

The instrument still works and is still tested: `scripts/gate-run.sh`, the transcript scorer in `crates/keel-cli/src/rubric.rs`, and four known-answer fixtures that run with the suite (ten canned transcripts with no writes must score 0%, ten with writes must score 100%, and a run missing transcripts must fail completeness even though its naive rate reads 100%). Nothing was deleted. Nobody is running it.

## What would restart it

Not a schedule, and not a number to improve on. One of two things:

- a change to how the agent is oriented — the hooks or the skill's wording — that is not obviously safe; or
- evidence from real use that writes have stopped, or that the store is filling with rows nobody keeps.

In either case a run measures the *changed* thing against Run C, not against the original bar. 18 of 20 is the baseline now.

## Where the six documents went

`PROBLEMS.md`, `WAY-FORWARD.md`, `VALIDITY-AUDIT.md`, `RUN-A.md`, `RUN-B-C.md` and `KEEP-RATE.md` are archived in Specline with their revisions intact and searchable. Their files are gone from the repository.

Two of them were wrong by the time anyone read them. `PROBLEMS.md` was written for outside review and its central claim — *"the agent will not write to it"* — was refuted by the review it was commissioned for, but it stayed in `product/` reading as current for a week afterwards. `WAY-FORWARD.md` then specified a five-part treatment bundle designed against a 3-of-10 baseline that turned out not to exist; most of it was never built, and the run that met the criterion applied one part of it.

That is the argument for one page rather than six. A document that describes a moment either gets dated or gets retired, or it goes on quietly asserting things that stopped being true.
