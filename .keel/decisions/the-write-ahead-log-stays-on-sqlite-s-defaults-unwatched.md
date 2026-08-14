<!-- keel:generated decision dec_01KZZGS6S1DC5T05KFQY6KQCFT v1 2026-08-14T06:54:20Z
     source of truth is Keel — edits here are not saved -->
# B-64 — The write-ahead log stays on SQLite's defaults, unwatched

**Status:** `accepted`  
**Id:** `dec_01KZZGS6S1DC5T05KFQY6KQCFT`

# The write-ahead log stays on SQLite's defaults, unwatched

Nothing is added to monitor or manage the WAL. No check in `doctor`, no `journal_size_limit`, no background checkpoint loop. `wal_autocheckpoint = 1000` and the TRUNCATE on daemon shutdown are the whole of it, as before.

## What prompted the question

`Store::wal_pages()` is documented as the number that says whether checkpointing is keeping up, and only tests call it. The failure it describes is real and genuinely nasty: SQLite cannot checkpoint past the oldest open read snapshot, the daemon runs for days holding a server-sent-events connection, and a reader that never releases would pin the log so it grows without bound while every query keeps answering correctly out of it. No error, no failed request.

## Why nothing is being built

The hazard is real but has not been observed, and three things already stand between it and us. `wal_autocheckpoint = 1000` handles the ordinary case. `await_holding_lock = "deny"` in the workspace lints forbids the coding mistake most likely to cause it — a guard held across an await. The daemon checkpoints with TRUNCATE on clean shutdown.

The evidence says it is working: at the time of asking, the live store carried a 3 MB log against a 7.2 MB database. That is the mechanism doing its job, not a symptom.

Adding a background timer to watch for something that has never happened is exactly what the scale-discipline rule exists to stop. One user, a few thousand rows, and a measurement that says the current arrangement is fine.

## What was learned along the way, and where it lives

Four measurements sit on KEEL-191, and they are the reason closing this costs nothing:

- `PRAGMA wal_checkpoint(PASSIVE)` is not a read. It cost 7 ms on a 606-page log against 1.32 µs for a `stat` of the `-wal`, so anything calling it on a cadence puts real work on the write path and quietly replaces the checkpoint policy it meant to observe.
- A checkpoint never truncates the file. The next write after one does, and only when `journal_size_limit` is set. So file size is a high-water mark and not a current reading.
- A pinned snapshot shows as `checkpointed == 0` while `busy` stays `0`. The obvious column to check is the wrong one, and a monitor written against `busy` would never fire while reading as success.
- Retrying PASSIVE does not help. Three consecutive attempts moved zero pages, then all 304 the moment the reader released.

If a `-wal` is ever found larger than the store beside it, the diagnosis is already written down and reopening this is cheap.

## Reversible

Nothing was built, so there is nothing to unwind. The argument for revisiting would be an actual observation — a log that does not come back down — rather than the theoretical possibility, which is what was on the table this time.

