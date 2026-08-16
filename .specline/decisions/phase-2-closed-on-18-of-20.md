<!-- keel:generated decision dec_01KZN24NH42AW7XQB9GNNZ0NFY v1 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-29 — Phase 2 closed on 18 of 20

**Status:** `accepted`  
**Decided:** 2026-08-10  
**Id:** `dec_01KZN24NH42AW7XQB9GNNZ0NFY`

KB's call, 2026-08-10.

The exit criterion - across ten unprompted sessions Claude writes to Keel in at least nine, every write attributed, zero duplicate projects - was met on two consecutive independent draws. Runs B and C each scored 9 of 10. Pooled 18 of 20, point estimate 90%, 95% CI [69.9%, 97.2%].

What closes it is not the score alone but the mechanism being understood. Six runs decompose into three distinct failures with three distinct fixes: the SessionStart hook fixed noticing and intent (ceiling ~30% to 80%), the continuation turn fixed execution (offers 8 to 1, and it was an instrument artefact rather than a product fault), and the deterministic Stop hook fixed the residual by reaching sessions that never noticed Keel at all during heads-down implementation work. The Stop hook fired in exactly the three sessions that had missed in Run A and stayed silent for the seven that had not, which is stronger evidence than the aggregate.

What is knowingly carried rather than resolved:
- The pooled lower bound is 69.9%. Twenty sessions cannot establish a 90% rate, and 9-of-10-at-n=10 was retired as a statistical instrument for exactly that reason. Closing the phase is a judgement that the mechanism works, not a claim that the rate is 90%.
- The precision floor does not exist. Step 10's independent hand-judge of ~20 writes remains open as a p0. The criterion is pure recall, which is least trustworthy when recall is high.
- Chat and Cowork have neither hook and are entirely untested. Everything measured is Claude Code.
- One known failure mode survives: a session that receives the Stop nudge and answers the user's next question instead.

These are recorded as open work, not as reasons the phase stays open.

