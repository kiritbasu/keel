<!-- keel:generated spec spc_01KZMGZ01R1JCB09WKKJXFSGRC
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Step 1 — validity audit of run 4

*Zero build, as specified. Completed 2026-08-10. Evidence is the archived Claude Code transcripts under `~/.claude/projects/*keel-gate*/`, which survived teardown — 55 in total, 10 inside the run-4 window (2026-08-09T21:53:44Z–22:12Z).*

---

## The headline: run 4 was 5 of 10, not 3 of 10

`keel gate` counts **distinct `session_id` values**. Five sessions called `mcp__keel__keel_create`; they minted only three distinct ids, because the ids were date-based.

| Time (UTC) | Project | `keel_create` calls | `session_id` |
|---|---|---|---|
| 21:57:03 | tideline | 1 | `tideline-2026-08-09` |
| 22:00:15 | tideline | 2 | `tideline-2026-08-09` ← collision |
| 22:01:59 | tideline | 2 | `tideline-2026-08-09-a` |
| 22:02:36 | pellet | 2 | `pellet-2026-08-09` |
| 22:03:31 | pellet | 1 | `pellet-2026-08-09` ← collision |

Two collisions, five sessions, three rows. **The reported 3/10 was an artefact of the id scheme, not a measurement of behaviour.** The id-collision defect was already recorded as a known undercount; what was not known is that it was undercounting *the headline number the strategy was built on*.

Decomposed against the panel's rubric:

| | Sessions | |
|---|---|---|
| Wrote (L5) | **5** | reached `keel_create`, writes confirmed |
| Oriented, did not write (L2/L3) | 2 | called `keel_context`, no write |
| Never touched Keel (L1) | 3 | no Keel tool of any kind |

---

## Check 1 — the permission-allowlist confound is dead

**No `keel_*` tool call in run 4 was denied at the permission layer.** Not one, across all ten transcripts. The allowlist in `scripts/gate-run.sh` named the tools correctly and they were granted.

This was the check that could have killed a confound without spending a run, and it did. It also retires the risk flagged against the future `keel_record` rename — the mechanism is confirmed working, so the rename's danger is a *new* omission rather than an existing fault.

**But two writes failed for a different reason**, and this is a finding in its own right:

```
keel_create(type=task, project=pellet, title="Guard store.gc() against empty keep set…")
  → invalid task field `priority`: unknown variant `high`,
    expected one of `p0`, `p1`, `p2`, `p3`

keel_create(type=task, project=tideline, title="Validate constituent phases to 0–360")
  → invalid task field `priority`: unknown variant `medium`, expected one of `p0`…`p3`
```

The model's natural vocabulary is *high/medium/low*; the schema demands *p0–p3*. **Both sessions retried and succeeded**, so the error message did its job and no write was lost — but this is direct evidence for the panel's open disagreement (b): every field is a decision point at the write boundary, and at least one of them is actively wrong about how a model thinks. It is the only organic evidence that exists on that question, since 119 of the store's first 134 events were bootstrap.

## Check 2 — permission configuration is clean

No `--permission-mode` flag anywhere in the harness. No `.claude/` directory in either scratch project, so no project-local settings and no local permission grants. Nothing was silently permitting or denying.

## Check 3 — the harness is single-turn. Confirmed.

```bash
( cd "$work/$project" && claude -p "$prompt" \
    --mcp-config "$mcp_config" --strict-mcp-config --allowedTools "$tools" \
    </dev/null 2>&1 ) | tee "$work/session-$n.log"
```

`</dev/null`, no `--continue`, no `--resume`, no continuation prompt. One turn, then exit. The panel's central claim is correct: a session ending *"I'll hold off until you say go"* was addressing a turn the harness could not supply.

## Check 4 — the post-run-4 fixes, with one correction to the panel

Run 4's baseline was 21:53:44Z. Local clock is UTC+1.

| Fix | Committed | Relative to run 4 |
|---|---|---|
| SessionStart hook (the treatment) | `ebd9d74` 21:52Z | **1 minute before** — correctly measured |
| `--sessions` denominator | `32a6337` 22:11Z | 18 min after |
| Path normalisation | `4fccfc7` 22:17Z | 24 min after |
| Session-id uniqueness instruction | `4fccfc7` 22:17Z | 24 min after |
| Expanded anti-asking section | `4fccfc7` 22:17Z | 24 min after |

**Correction.** The panel states the anti-asking preamble "has never been measured at all". That is not quite right. The hook as it ran in run 4 already contained:

> *"Use the keel_* tools and pass a stable session_id. **Do not ask permission to record something that plainly happened.**"*

So a one-sentence form of the instruction **was** in play, and the true result under it was 5/10. What is unmeasured is the *expanded* version — the one carrying the measurement as evidence, the "record it, then say in one line that you did" phrasing, and the `SKILL.md` section. The distinction matters: the weak form has a number, and it is 5, not 3.

## Check 5 — transcripts survived teardown

Yes. `~/.claude/projects/` retains full JSONL per session, independent of the Keel store that was torn down. Tool calls, inputs, results and errors are all recoverable. This is what made the audit possible, and it should be treated as the archive of record rather than the `tee`'d logs, which contain only final assistant text.

---

## What is still trustworthy

| Claim | Verdict |
|---|---|
| "3 of 10 wrote" | **False.** 5 of 10. Artefact of id collision |
| "0 duplicate projects" | **Trustworthy** |
| "every write attributed" | **Trustworthy**, but weaker than it sounds — ids were not unique, so attribution was ambiguous |
| "7 sessions wrote nothing" | **False.** 5 did |
| "orientation solved, writing is not" | **Half true.** Orientation is solved. Writing was 5/10, not 3/10 |
| Any reading of run 4 as a measure of *judgement* | **Invalid.** Single-turn harness; the offers had no turn to be answered in |
| "the premise may be dead" | **Unsupported.** It rested on 3/10, which was wrong, from an instrument that cannot resolve 55% from 100% |

## What this changes

1. **The gap to the bar is half what was thought.** 5→9, not 3→9.
2. **The trend across runs is steeper.** With the same id-collision defect present in earlier runs, those numbers are also likely undercounts — direction unknown without re-auditing their transcripts, which is cheap and should be done before any run.
3. **Nothing in the Step 4 treatment bundle is invalidated**, but its expected effect is smaller, because the baseline is higher and the weak anti-asking form is already priced in.
4. **The `--sessions` denominator fix does not solve the counting problem.** It corrects the denominator; the numerator is still distinct session ids. **A session that writes under a colliding id is still invisible.** This must be fixed before Run A: count sessions by launcher identity, not by what the model minted.

## Recommended addition to Step 2

The panel's instrument repairs stand. Add one, promoted by this audit:

> **Inject the session id from the launcher.** The harness knows how many sessions it started and can pass a unique id per session — via the SessionStart hook, which already receives the payload and can stamp it. Asking the model to mint a unique id is asking it to solve a problem it has no information about, and it demonstrably gets it wrong in the direction that corrupts the measurement.

And re-audit runs 1–3 from their transcripts before Run A. Zero build, and it is the only way to know whether the trend is real.

---

## Addendum — runs 1–3 re-audited from transcripts

Done as recommended above, before any new run. Zero build.

| Run | Condition | Sessions | **Wrote** | Oriented, no write | Never touched Keel | Distinct ids |
|---|---|---|---|---|---|---|
| 1 | live store, cold start | 10 | **1** | 2 | 7 | 1 |
| 2 | live store, Tideline archived | 10 | **0** | 4 | 6 | 0 |
| 3 | empty scratch store | 11* | **0** | 5 | 6 | 0 |
| 4 | SessionStart hook | 10 | **5** | 2 | 3 | 3 |

\* Run 3's window caught one ad-hoc diagnostic session of mine. Excluded from any rate.

**Runs 1–3 were reported accurately.** The id-collision undercount could only bite when more than one session wrote under the same id, and in runs 1–3 at most one session wrote at all. So only run 4 was wrong — the one the strategy was built on.

**The trend is real, and steeper than reported: 1 → 0 → 0 → 5.**

Two further things fall out of the table, neither previously visible:

- **Orientation moved as much as writing did.** Sessions never touching Keel: 7, 6, 6 → **3**. Sessions engaging at all: 3, 4, 5 → **7**. The SessionStart hook did its job on the stage it targeted, and the evidence is stronger than "3 of 10" ever suggested.
- **Runs 2 and 3 scoring zero is now more interesting, not less.** Nine sessions across them called `keel_context` and none wrote. Run 2 had the archived-Tideline near-match; run 3 had a genuinely empty store. Both are cold-start conditions, and both produced total write failure with orientation working. That is a cleaner statement of the cold-start problem than TQ-17 made, and it argues for TQ-17(a) — auto-create the first project — on evidence rather than analogy.

### One consequence for Step 2

The rubric in the plan (L0–L5) can be scored **retrospectively from the archived transcripts**, for all 41 sessions already run, without spending a single new session. Tool calls, inputs, results and final text are all present. That is a free labelled corpus for the hand-judging in Step 10, and it should be built before Run A rather than after.
