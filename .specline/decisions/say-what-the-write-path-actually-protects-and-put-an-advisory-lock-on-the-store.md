<!-- specline:generated decision dec_01KZXMVTV13AA2M09Y84NMG9HT v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-60 — Say what the write path actually protects, and put an advisory lock on the store

**Status:** `accepted`  
**Id:** `dec_01KZXMVTV13AA2M09Y84NMG9HT`

Resolves TQ-36 and the untitled duplicate beside it, 2026-08-13. Both asked the same thing: hard constraint 1 says the daemon owns the single write path, DuckDB used to enforce that and SQLite does not, so is it a rule or a convention now.

**Both halves, and TQ-36 was right about the first.** The constraint's value was never the exclusivity — six of the seven steps in a Keel write have nothing to do with locking, and they are the reason one place has to know how to write. So the constraint is reworded to say what is actually protected: everything that writes goes through `keel-core`'s write path. A contract claiming an enforcement the engine no longer provides is worse than one describing what is true.

**But rewording alone would have permitted what happened today.** I started a second daemon against the live store by accident — `--bind` and `--embeddings` passed, `--home` forgotten — and it applied a schema migration while the first daemon was serving. That process was not a rogue writer skipping the five steps. It went through `keel-core` correctly, and the reworded constraint would have blessed it. It was a legitimate writer that should not have been a second one, and nothing in the system had anything to say about it. The migration guard could not: it refuses a binary *older* than the store, and this was newer.

So the second half: **opening the store for writing takes an advisory lock on it.** The daemon holds it for its lifetime; the CLI takes it for the length of a direct write. A second acquirer fails immediately with a message naming what holds it, instead of succeeding and being discovered later by a health field that happened to disagree.

**TQ-36's objection to a lock file no longer stands, and it is worth being precise about why.** It said "a stale lock after a crash is a store nobody can open, which is worse than the problem". That is exactly right for a PID file or a claimed row in a table, and exactly wrong for an OS advisory lock, because the kernel releases it when the file descriptor closes — including on `SIGKILL`, panic and power loss. Measured rather than assumed:

```
--- while the holder is alive ---
REFUSED — still held: "WouldBlock"
--- after SIGKILL of the holder ---
ACQUIRED — the lock was free
```

There is no stale-lock failure mode to weigh, so the option TQ-36 rejected on that ground comes back on the table having lost its only real cost.

`std::fs::File::try_lock` does this with no new dependency, which settles the "lock file or health probe" half of the duplicate: neither a hand-rolled file nor a probe. The probe stays where it is useful — it is what the CLI consults to decide whether to *ask the daemon instead of writing*, which is a different question from whether writing is safe, and it is advisory in the sense that a second daemon never thinks to ask.

Two costs, both accepted. `rust-version` moves from 1.85 to 1.89, which is when that API stabilised; this is a personal project on current stable. And advisory locks are unreliable on network filesystems — `doctor`'s location check already warns when the store sits in a synced or network folder, which is the same population.

Not attempting the third option TQ-36 listed, enforcement in the type system. The CLI legitimately writes when no daemon is running, so the capability cannot be daemon-only, and a runtime lock expresses "one at a time" directly where a type would have to express it by proxy.

