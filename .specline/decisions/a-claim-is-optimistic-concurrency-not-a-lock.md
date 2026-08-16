<!-- keel:generated decision dec_01KZS0SZ6V2YCKKGX3ANTW777B v1 2026-08-11T18:19:39Z
     source of truth is Keel — edits here are not saved -->
# B-48 — A claim is optimistic concurrency, not a lock

**Status:** `accepted`  
**Id:** `dec_01KZS0SZ6V2YCKKGX3ANTW777B`

`keel claim` had to be atomic, and the obvious reading of that is a lock. It is not one.

Claiming reads the task, checks whether anybody is holding it, and writes through the ordinary update with the version it read. Two sessions racing for the same task both read version 7; the first writes 8 and the second is rejected with a stale-version error carrying the current state and the events in between. That is already how every other write in Keel behaves, and it is exactly the property a lock would have been added to provide.

A lock would also have needed a release path of its own, and something to release it when the holder dies. Both of those already exist here and neither is new machinery: closing clears the claim in the store's update path, and a claim goes stale after three days.

## The three days are fsck's number, not a second one

`fsck` already warns about a task parked in `in_progress` for three days. Choosing a different threshold here would have meant two answers to "this session is probably gone", and the disagreement would surface as work `fsck` calls abandoned and `keel claim` refuses to take.

## The one write refused for want of a session

A claim with no `session_id` is refused outright. Everywhere else in Keel an anonymous write is merely less traceable — SPEC §6.5 says to fall back to the transport's identity rather than decline. Here the session is the content: a claim naming nobody excludes the task from `keel ready --unclaimed` while telling no one who to ask about it, which is worse than leaving it unclaimed.

## Releasing lives in the store, not in the close path

Any transition into a terminal status clears `claimed_by` and `claimed_at`, wherever it came from. Putting that only in `keel_close` would have let a plain `keel_update(status: done)` leave a claim standing, and `ready --unclaimed` would have gone on skipping work nobody was doing.

