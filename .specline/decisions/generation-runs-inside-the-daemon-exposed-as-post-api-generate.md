<!-- keel:generated decision dec_01KZKWMTBYP6P7B8DQTB3586G9 v2 2026-08-10T18:53:24Z
     source of truth is Keel — edits here are not saved -->
# B-21 — Generation runs inside the daemon, exposed as POST /api/generate.

**Status:** `accepted`  
**Decided:** 2026-08-09  
**Id:** `dec_01KZKWMTBYP6P7B8DQTB3586G9`

## Decision

Generation runs inside the daemon, exposed as POST /api/generate. The CLI is a client.

## Reasoning

Not a preference — a discovery. D-5 says non-daemon processes "connect read-only or go through the daemon's API", and **the read-only half does not exist**: DuckDB refuses a read-only connection while any process holds the write lock, so no second process can read the store while the daemon runs. Verified by implementing `open_read_only` and watching it fail with the same conflicting-lock error a writer gets; the code was reverted. Since the daemon is always running, "go through the API" is the only path, which resolves TQ-12 for every read-shaped command. `keel generate` falls back to opening the store directly only when no daemon answers, which is safe precisely because nothing else holds the lock then. **SPEC D-5's wording is now wrong** and is flagged as TQ-15.

## Reversible?

Yes.

