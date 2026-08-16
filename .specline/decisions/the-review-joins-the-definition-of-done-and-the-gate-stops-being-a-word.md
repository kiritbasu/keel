<!-- specline:generated decision dec_01M05B9KHR4EV9KQDT60YW929Q v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-80 — The review joins the definition of done, and "the gate" stops being a word

**Status:** `accepted`  
**Id:** `dec_01M05B9KHR4EV9KQDT60YW929Q`

A task is not done until it has been read against the five axes — correctness, readability, architecture, security, performance — with every Critical and Required finding fixed or filed as a row. KB's call, 2026-08-16.

**The evidence is one day.** A session ran the review over its own thirty-five commits and found three real defects: a callback that reloaded the whole page when a button had only looked for an update, a progress line that cleared solely because the parent happened to destroy the component, and two writers able to race on one staging file. `fmt`, `clippy`, the full suite and CI were green for every one of those commits, and had been all day. Two of the three were written in the last hour, when the work was fastest and each change looked small.

That is the argument in full. The automated checks establish that the code compiles, is formatted, and does what its tests say. They are silent on whether the tests ask the right question, and all three of those defects had passing tests sitting next to them.

**Reviewing your own work counts.** There is one developer, so insisting on a second reader would make the rule unfollowable, and an unfollowable rule is worse than none — it gets skipped and the skipping becomes normal. What does not count is treating a green suite as the review, which is the exact condition under which these three shipped.

**Not enforced, and that is honest.** Three items on that list are enforced in the storage layer: a terminal status needs a reason, a message and evidence. This one cannot be, because nothing can tell whether a person read something. It sits on the list as an instruction, the way most of the list does.

**The vocabulary changed with it.** "The gate" had come to mean the automated checks in most sentences and the review in others, and the ambiguity produced exactly the failure it describes — a session reporting that something was verified when only one half had run. Two words now: **the checks** and **the review**. The contract says so, so that the distinction survives the session that noticed it.

