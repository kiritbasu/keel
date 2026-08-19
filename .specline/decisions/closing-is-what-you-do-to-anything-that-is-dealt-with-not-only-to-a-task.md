<!-- specline:generated decision dec_01M0D6TFDNXGY9W38NABX4VJ9X v1 2026-08-19T14:52:58Z
     source of truth is Specline — edits here are not saved -->
# B-94 — Closing is what you do to anything that is dealt with, not only to a task

**Status:** `accepted`  
**Id:** `dec_01M0D6TFDNXGY9W38NABX4VJ9X`

## Decision

`specline_close` accepts a signal as well as a task. KB's call, 2026-08-19: *"widen close to accept a signal."* No fourteenth tool; the thirteen-tool ceiling holds.

This answers the open question "How does triage reach MCP without a fourteenth tool?" and takes its option 2.

## Why this rather than a fourteenth tool

Triage has to reach MCP or the phase does not do what it was built for. B-90's argument is that a session reads the Inbox, clusters it, checks each item against every decision ever made, and proposes outcomes for a person to accept or refuse — five of the six lifecycle stages are reading and writing at volume, which is what a model is good at and a person is bad at. A verb reachable only from a terminal leaves the human doing the reading, which is the half the design exists to move.

A fourteenth tool would have bought that at the cost of the thing the cap protects: more tools means worse selection, not more capability, and this would be a rarely-reached tool sitting among twelve well-worn ones.

**And the semantics genuinely match, which is what makes this a widening rather than a workaround.** "This is dealt with, here is why, and here is the proof" is the same sentence for a task and for a signal. `close` already enforces exactly what triage needs, in the storage layer where the CLI and MCP cannot disagree: a reason, a message on every reason, and evidence on `done`.

## How the five reasons map

Three apply to a signal and two do not:

- **`done` — picked up.** The signal became a feature. Evidence names the feature spec, which is the same demand `done` already makes of a task: show the thing that proves it.
- **`wont_do` — set down.** The message is the argument, and it is appended to the signal's body rather than replacing it, because the body is the verbatim and overwriting it would destroy what somebody said in the act of saying why we are not doing it.
- **`duplicate`** — the same want, already recorded. `other` names the signal that keeps the history.
- **`superseded` and `no_change`** are refused for a signal. Neither means anything about a want: a signal is not replaced by a later signal the way a decision is by a later decision, and "nothing changed" describes work rather than an idea.

## What this does not fix

The vocabulary is still named for work. A person setting a signal down reads `wont_do`, which sounds like a rejection of the idea rather than of doing it now — and that is the same mismatch KEEL-338 reports from the other direction, where a task that turns out to be a signal has no honest reason to close with. Widening `close` makes the mismatch reach further rather than resolving it. Worth doing anyway, because the alternative was a tool nobody would reach for, and because the fix for the vocabulary is the same fix either way and can come later.

`work::triage` stays the enforcing path underneath. `close` translates and delegates; it does not reimplement, so there is one place a signal can leave the Inbox and one set of invariants guarding it.

