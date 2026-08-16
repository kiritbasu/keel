<!-- specline:generated decision dec_01KZN2W5BPHM5DH3PRSHW5A600 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-30 — The p0 wedge was SIGKILL mid-write, not the FTS index

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZN2W5BPHM5DH3PRSHW5A600`

Fixed 2026-08-10. I diagnosed this wrong twice before getting it, and the wrong diagnosis is recorded in STATUS as of the previous commit.

What it actually was. An UPDATE raised a DuckDB FATAL: 'Failed to delete all rows from index. Only deleted 0 out of 1 rows.' That is an ART index disagreeing with its table. A FATAL poisons the DuckDB connection, so every subsequent query fails with whatever operation happened to be running - 'count matching rows' on a create, 'run a question lookup' on an update. Reads on a freshly started process worked because they never touched the damaged index, and fsck reported clean because it checks referential integrity, not index consistency. Every observation was true and the conclusion was still wrong.

My earlier claim that the FTS index was broken came from searching after a write had already poisoned the connection. Search on a genuinely fresh daemon returns hits. The FTS index was never involved.

The cause. The daemon's graceful shutdown waits for in-flight connections, and /api/events is a Server-Sent Events stream that by design never ends. So SIGTERM never completed, and every restart this session ended in SIGKILL - repeatedly, mid-write. That is how the index and its table stopped agreeing.

Three fixes:
1. Shutdown now runs on a five-second deadline and CHECKPOINTs before exiting. Verified: with an SSE stream open, SIGTERM stops the daemon in 5s and logs the checkpoint. It used to hang indefinitely.
2. Error chains are surfaced. Error::chain() walks to the root cause and the MCP boundary reports it. The source was attached the whole time; nothing printed it, which is why two hours went into guessing instead of reading.
3. Regression test: create, checkpoint, reopen, update - the exact cycle, asserting the connection is not poisoned afterwards.

Recovery used backup and restore, which rebuilt every table and index from Parquet. 536 rows, verified per table. The damaged store is kept at ~/.keel.corrupt-20260810T053513Z and the Parquet backup at /tmp/keel-backup-repair. No data was lost.

Worth noting for the panel's Step 8: their hypothesis was two write paths into one file where only the daemon maintains derived state. That was reasonable and it was not the cause. The cause was cruder - the daemon could not be stopped politely, so it was stopped rudely, many times.

