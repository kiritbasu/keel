<!-- specline:generated spec spc_01KZKMPVP6MN27N10M3PPD6EVK v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# Phase gates that cannot be verified without a human

**Status:** `draft`  
**Kind:** `note`  
**Id:** `spc_01KZKMPVP6MN27N10M3PPD6EVK`

Phase 2's criterion — nine of ten *unprompted* sessions write to Keel — cannot be automated. "Unprompted" is the whole claim, and a test that calls the tool has prompted it.

Phase 1's UC-1→UC-4 gate passes mechanically: 21 tests drive a real daemon over real HTTP. What that does not prove is the part only a model can demonstrate — that the tool descriptions lead an agent to the right tool unprompted. A scripted client is told which tool to call.

`plugin/README.md` has the protocol for running the ten sessions, and what each failure mode means in terms of which part of `SKILL.md` to change.

