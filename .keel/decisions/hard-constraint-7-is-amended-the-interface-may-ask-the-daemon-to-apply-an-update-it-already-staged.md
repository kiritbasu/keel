<!-- keel:generated decision dec_01M03EVSQZBVB93NR94MYNTKWB v1 2026-08-15T19:48:10Z
     source of truth is Keel — edits here are not saved -->
# B-75 — Hard constraint 7 is amended: the interface may ask the daemon to apply an update it already staged

**Status:** `accepted`  
**Id:** `dec_01M03EVSQZBVB93NR94MYNTKWB`

## Decision

Hard constraint 7 — "the desktop app is read-only. Claude and Keel are the only writers. No write endpoints on the daemon for it, no forms in it" — gains exactly one exception: **the interface may ask the daemon to apply an update the daemon has already fetched, verified and staged.**

Everything else in constraint 7 stands. The app still writes nothing to the store, still has no forms, and still gets no other write endpoint. KB agreed to this explicitly on 2026-08-15.

## Why the exception is narrow enough to be safe

The endpoint takes **no arguments**. Not a version, not a URL, not a path. It can only apply what is already staged, and staging is something only the daemon does, only after fetching a release over TLS and checking it against the SHA-256 in the published manifest (B-73).

So the complete set of things a caller can cause is: *restart into the version Keel had already decided was safe to install*. It cannot install a chosen version, cannot point Keel at a different source, and cannot cause an unverified binary to run.

That matters because KEEL-168 is still open: the API has no token, so any page on localhost can reach it. Under this design the worst that reaches is an unexpected restart. Compare it with the thing constraint 7 exists to prevent — a browser page silently writing to the project's history — and the difference is the whole argument.

## Why the constraint had to move rather than be reinterpreted

It would have been easy to say an update endpoint "is not really a write" because it touches no rows, and thereby keep the letter of the rule while breaking it. That is the reasoning that makes constraints stop meaning anything. It is a write endpoint on the daemon, for the app, which constraint 7 forbids in those words — so the rule changes in the open, with the reasoning attached, and the next person meets an amendment rather than a contradiction.

## What forced it

The updater shipped applying compatible releases at the next daemon start without telling anyone (KEEL-203). On first real use that was wrong twice over: releases land every few hours during active development so a daily check is too slow, and a restart under someone's feet is something they should agree to. Both need the interface to show state and take a decision, and the second needs it to act. KEEL-225 is the work.

## What this does not license

Applying an update the app selects, downgrades, installs from a URL, or anything that writes to the store. A future request to relax any of those is a new decision, not an extension of this one.

