<!-- specline:generated decision dec_01M0CVA1KSB8DWS6DDE2DYMCNB v1 2026-08-19T11:08:29Z
     source of truth is Specline — edits here are not saved -->
# B-92 — The roadmap shows derived progress, not a target date, and releases are rows

**Status:** `accepted`  
**Id:** `dec_01M0CVA1KSB8DWS6DDE2DYMCNB`

A phase's roadmap row says how many of its tasks are closed and when one of them last moved. It does not say when the phase is due.

## Why

`target_date` existed and nothing set it. It is reachable only through the `fields` bag on create and update, where it appears as one word in a list of examples, so across fifteen phases four had a date and all four said 2026-08-09 — the day `bootstrap` seeded them. The other eleven rendered "no target".

The obvious fix was to ask for a date the way `Milestone::new` asks for a summary, so the compiler finds anyone who forgets. That was rejected. A date on a one-developer project with no external commitment is a guess, and a guessed date is worse than a blank one: it makes the roadmap look planned, and it goes stale silently. Requiring the field would have hidden the gap rather than closed it.

Progress and last activity cannot go stale, because nobody maintains them. They also answer the question the column was there for — is this moving — which a date never did.

## What this cost

`milestone_states` returned the derived state and threw away the counts it was derived from, so every caller that wanted numbers found its own. `render_status` filtered the task list itself; the digest printed a target date; the API sent neither, which is why the browser had nothing. It now returns a `MilestoneProgress` — state, tally, last activity — and all three read the same numbers.

`target_date` stays in the schema and stays unadvertised. A date somebody does set still shows and still orders the roadmap. The day there is a real external commitment, the field is there.

One thing to delete or build: SPEC §7 says the digest's attention block carries "overdue milestones". Nothing computes it — the only occurrence of the word in the workspace is a doc comment — and with no dates it could never have fired.

## Releases

Ten versions had shipped without one of them being a row, so "what shipped, and when" was answerable only from `git tag`. `MilestoneKind::Release` had existed since Phase 0 and had never been used. All ten are backfilled.

They get a strand of their own on the roadmap and their own table in `STATUS.md`, rather than being sorted in with the phases. Two reasons. A release carries no tasks, so beside the phases it is ten rows of `planned  0 / 0`. And interleaving by date reads badly here: the first ten phases finished inside three days and every release landed the week after, so one chronological list buries the plan in the middle of a changelog.

## What was deliberately not done

A stored pointer from a phase to the release that will carry it. It is the same guess as a date, one field along — a version nobody has committed to, going stale the same way. For a phase that has already shipped it is derivable from the dates and needs no field at all.

