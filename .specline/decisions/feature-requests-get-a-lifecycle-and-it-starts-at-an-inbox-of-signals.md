<!-- specline:generated decision dec_01M0CNH5V9B58M5J50E8ZM76Y3 v1 2026-08-19T09:30:08Z
     source of truth is Specline — edits here are not saved -->
# B-90 — Feature requests get a lifecycle, and it starts at an Inbox of signals

**Status:** `accepted`  
**Id:** `dec_01M0CNH5V9B58M5J50E8ZM76Y3`

## Decision

Feature requests get the full lifecycle — capture, develop, decide, shape, build, close the loop — rather than a fifth `tasks.kind` and a filter. KB's call, 2026-08-19: *"Yes lifecycle."*

The design is in the spec [How feature requests should work, end to end](spc_01M0CMDKDPWZ0CS317SEPXTDVS). Three things from it are settled here because everything downstream depends on them.

**Four artifacts, all of which already exist.** A raw arrival is `feedback`. The developed idea is a `spec` with `kind = 'feature'`. A rejection is a `decision`. The work is a `task` with `kind = 'feature'` and children by `parent_id`. No `features` table, so the thirteen-type ceiling holds; two enum values are added and nothing else in the schema moves.

**The thinking is separate from the container.** The feature spec holds the why and exists whether or not the thing is ever built. The epic task is the unit of work and is created at the moment of the decision to build, not before. This is what keeps unbuilt ideas off the board entirely, keeps hard constraint 7 intact — the app creates the epic, Claude writes the spec — and makes a milestone able to hold epics and loose bugs at once, which is what KB asked for.

**The human judges twice.** In or out, and is this the right decomposition. Clustering, dedupe, checking a new arrival against every decision ever made, and drafting the breakdown are all proposed rather than performed. That is the whole difference between this and a ticketing system, and it is the reason the lifecycle is affordable at all — five of the six stages are reading and writing at volume.

## The naming

**The surface is the Inbox.** KB delegated the name; this is the reasoning, so it can be overruled cheaply.

It was tempting to coin something — Signals, Wants, Requests, Wishlist all read better on a nav item. Every one of them is wrong for the same reason: **they name a collection you would be pleased to grow.** Nobody has ever felt bad about having two hundred signals. Everybody feels bad about two hundred unread. KEEL-303 is precisely the complaint that a pile grows until it is too expensive to read and nothing points that out; a name that implies the thing should be *emptied* does product work that no feature can. Inbox is also thirty years of muscle memory, which is not nothing for a surface meant to be opened daily and cleared.

**An item in it is a signal.** Every alternative breaks on at least one source: a competitor sighting is not a "want", KB's own 5pm idea is not a "request", a recurring theme in support is neither. "Signal" carries the right hedge — something noticed that might mean something, and might not — which is exactly the epistemic state of a thing that has not been triaged.

**A signal is picked up or set down.** Not accepted/rejected. *Set down* is the honest word: the thing is not destroyed, the reasoning for putting it down is written and retrievable, and it can be picked up again when the same idea arrives in four months. That is the durable-rejection property the design turns on, and naming it "rejected" would make it sound like the tombstone it deliberately is not.

## What this rejects

The cheap version — `feature-request` as a fifth task kind, filtered out of `specline_next`, nothing else. Roughly half a day against several days. It buys the board-clutter fix and leaves every other gap open: no record of who asked, no surviving a no, no dedupe, no closing the loop, and every idea still phrased as a solution because a task asks you what to do. Recorded here because it is a reasonable answer and because it is the version that gets built by accident if nobody decides otherwise.

Also rejected: a `features` table (nothing needs it), any scoring formula — RICE, votes, weights (one user; `specline_next` already ranks), and a public request portal (no customer stream yet).

## Consequence for TQ-32

TQ-32 declined a `triage` task status on 2026-08-11 because *"with app filing declined, nothing files in a hurry, so the holding pen has nothing to hold."* App filing shipped afterwards, so that reason has expired — but the answer survives anyway, for a better reason: **an untriaged signal is not a task at all**, so it needs no status on the task enum to hide behind. The task status enum stays at five. TQ-32 should be superseded rather than reopened, so the record shows the answer standing on reasoning that is still true.

