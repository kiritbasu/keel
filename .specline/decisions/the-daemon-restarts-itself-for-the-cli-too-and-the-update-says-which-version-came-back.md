<!-- specline:generated decision dec_01M03KQE9V0G9VSZMPKTWHB171 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-77 — The daemon restarts itself for the CLI too, and the update says which version came back

**Status:** `accepted`  
**Id:** `dec_01M03KQE9V0G9VSZMPKTWHB171`

## Decision

`POST /api/update/restart` — a second write endpoint on the daemon. It restarts into whatever binary is at this process's own path, and does nothing else. `keel update` and `keel update --rollback` call it, then poll `/api/health` and report the version that came back. Both take `--daemon`, like every other command that talks to one.

## Why

KB, after taking 0.1.3:

> do i need to restart the daemon manually? isn't that a part of the update? also where's the instruction for it. that message needs to be way more user friendly

All three were fair. The old line was:

```
Updated Keel 0.1.2 → 0.1.3. Restart the daemon to run it; `keel update --rollback` undoes this.
```

There is no `keel restart`. Nothing supervises the daemon — no launchd job on the machine this was found on, and the running one had been started by hand from a shell. The message did not name the command. So it handed over a chore without the means to do it, and the daemon quietly went on serving the old version.

The odd part is that the capability existed. B-75 gave the daemon `/api/update/apply`, which re-execs into an update it staged itself, and the interface has a button for it. But `keel update` installs directly and stages nothing, so that endpoint answers "nothing is staged, so there is nothing to apply". Two halves, never joined.

## Why an endpoint rather than the CLI killing the process

The CLI does not own the daemon and has no business signalling a process it did not start — it does not know which one is serving the store it just updated, or whether that daemon is mid-write. Asking is what the daemon can answer safely: it replies, flushes, and `exec`s itself, keeping its pid so anything watching it does not count a restart as a crash.

## Why it waits and re-reads the version

"Asked it to restart" is a claim about a request. "It is now serving 0.1.3" is a claim about the thing somebody cares about, and this project keeps meeting the gap between those two. So the CLI polls health until it gets an answer and reports the version it finds.

That turned up a case worth its own sentence. `keel update` writes into the directory holding the `keel` being run; a daemon started from somewhere else has a different binary at its own path and is untouched. The restart then succeeds and changes nothing. It now says so, and says how to find the other copy, rather than reporting a successful update that did not take.

## What is not covered by a test, and why

The endpoint ends in `exec`. A test that reached it would replace the test binary with itself, so there is no unit test of the endpoint — the caller's half is tested against a stub daemon, including the too-old and came-back-unchanged failures, and the endpoint was exercised against a real daemon by hand: one pid, two "listening" lines in the log, serving again in under a second.

