# Phase 2's gate, by hand

For when `scripts/gate-run.sh` cannot run — the headless `claude -p` needs its
own login, separate from the desktop app's.

This is also the *better* version of the test. `claude -p` gives single-turn
sessions; the gate is about whether Claude reaches for Specline during ordinary
back-and-forth work, and a real conversation is what the PRD means by a session.

## Before you start

```bash
curl -s http://127.0.0.1:7654/api/health          # daemon up
ls ~/.claude/skills/specline/SKILL.md                 # skill installed
date -u +%Y-%m-%dT%H:%M:%SZ                       # write this down — it is t0
```

## The rules

- **A fresh session per row.** Ten sessions, not one session with ten turns.
- **Start each one in the project's directory**, listed below. Not in the Specline
  repo — `CLAUDE.md` there is four hundred lines telling Claude to use Specline,
  which is the opposite of unprompted.
- **Do not mention Specline.** Not once, in any session. No "remember that", no
  "record this", no "update the tracker".
- Work normally otherwise. Argue, follow up, change your mind. A session that
  ends after one exchange is not much of a test.

## The ten

Scratch projects live under `$TMPDIR/keel-gate`. Recreate them with
`scripts/gate-run.sh` if they are gone — it builds them before it runs anything.

| # | Start in | Say something like |
|---|---|---|
| 1 | `tideline` | `high_waters misses the first peak if the window starts right on one — have a look at src/harmonics.py` |
| 2 | `tideline` | `we should cache the constituent lookup, it gets recomputed on every height() call` |
| 3 | `tideline` | `let's go with 15-minute resolution as the default for the tide table rather than the current step` |
| 4 | `tideline` | `what's the risk if a station's chart datum is wrong? walk me through what breaks` |
| 5 | `tideline` | `a harbourmaster rang to say the 7-day table is unreadable on a phone — the times need to be local, not hours since epoch` |
| 6 | `tideline` | `before I forget: constituent phases should be validated to 0-360, nothing checks that today` |
| 7 | `pellet` | `gc() in src/store.js deletes anything not in keep. if keep comes back empty by accident that wipes the store — fix it` |
| 8 | `pellet` | `we picked sha256 early on but blake3 is a lot faster. worth switching?` |
| 9 | `pellet` | `what's stopping put() from being atomic? a crash mid-write leaves a truncated blob under a valid-looking digest` |
| 10 | `pellet` | `I want a size cap on the store with LRU eviction. roughly what's involved?` |

Reword them freely — they are examples of ordinary developer talk, not a script.
Several are things `SKILL.md` claims to trigger on ("we should", "let's go
with", "what's blocking", a customer complaint), which is the point: those are
what people actually say.

## Scoring

```bash
specline gate --since <t0>
```

Six sessions in `tideline` and four in `pellet`, so if the
`keel_projects`-first instruction is not firing, duplicates show up as
"Tideline / tideline app" or similar. That is the failure the PRD calls the most
damaging, because it quietly ruins the cross-project view.

`plugin/README.md` maps each failure mode to the part of `SKILL.md` at fault.
Every one of them is a wording change, not a code change.
