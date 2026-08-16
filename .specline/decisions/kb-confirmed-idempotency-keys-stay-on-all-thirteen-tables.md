<!-- specline:generated decision dec_01KZN5H4EJ905TXJA2RTS0MNKY v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-32 — KB confirmed: idempotency keys stay on all thirteen tables

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZN5H4EJ905TXJA2RTS0MNKY`

TQ-9 answered 2026-08-10. B-10 stands and is no longer provisional.

The spec disagreed with itself - REQ-7 and section 7.2 say every create is idempotent, section 3.2 gave the column only to tasks. Implemented on all thirteen because the alternative silently drops idempotency for twelve types including projects, the one type where duplicates are called out as ruining the cross-project view.

It has since earned it on organic traffic: across the gate runs, sessions called create twice with an identical title on nine occasions and the key deduplicated every one.

