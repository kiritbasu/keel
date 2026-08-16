<!-- specline:generated decision dec_01KZX83RKRDV71XF3ZWMBYY50N v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-56 — Superseded decisions stay in search results and carry a label saying what replaced them

**Status:** `accepted`  
**Id:** `dec_01KZX83RKRDV71XF3ZWMBYY50N`

KB decided, 2026-08-13.

A decision whose thinking has been replaced — `decisions.status = 'superseded'`, or an inbound `supersedes` edge — stays in the index and stays returnable. `SearchHit` gains `superseded_by`, and the hit says which decision replaced it. Ranking is untouched.

The reason is the reason Keel exists. "Why did we stop passing `--all-features`" is answered by the old decision and the new one together; returning only the new one answers a different question. Hiding superseded rows would make the store good at describing the present and useless at explaining it.

Ranking them down was the obvious middle path and was rejected: the multiplier would be arbitrary, and the adjustment would be invisible to whoever read the results — the silent-correction shape this codebase keeps having to undo. Telling the caller what is true and letting them decide is what the close reasons and the digest already do everywhere else. It also composes: demotion can be added later on top of a label, and a label cannot be recovered from a demotion.

Not to be confused with a superseded *revision*, which is `documents.status = 'superseded'` and is already settled — search reads current revisions only, older ones stay readable by version through `keel_get`, and passages are never built for them.

Decided before chunking lands because the label has to be carried from the query through to the hit, and retrofitting means touching the same three layers twice.

Resolves QUE "Should superseded decisions still be findable by search".

