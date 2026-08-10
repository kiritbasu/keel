<!-- keel:generated decision dec_01KZMTF8PVC0AWYFPQVGXM69BB v1 2026-08-10T03:27:56Z
     source of truth is Keel — edits here are not saved -->
# s2, s7 and s9 are genuine misses, not L0

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZMTF8PVC0AWYFPQVGXM69BB`

KB's judgement, 2026-08-10. The three silent sessions in Run A were all pure implementation prompts - cache a lookup, fix gc() wiping the store on an empty keep set, make put() atomic. Each is a genuine miss rather than a session with nothing worth recording.\n\nThat is the right call. A bug that would wipe a content-addressed store on an accidental empty keep set is exactly the kind of thing a project memory should hold, and 'we found and fixed a data-loss bug' is more valuable six months on than most of what did get recorded.\n\nConsequence: Run A is 7 of 10 against a bar of 9, and the residual is real. It also settles the open judgement in TQ-21 and re-frames Step 6.

