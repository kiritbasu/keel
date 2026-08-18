<!-- specline:generated decision dec_01M0B3HDB61W4VFBWJDX2PADCR v1 2026-08-18T18:58:19Z
     source of truth is Specline — edits here are not saved -->
# B-89 — Writes are same-origin only, and that stops being an accident

**Status:** `accepted`  
**Id:** `dec_01M0B3HDB61W4VFBWJDX2PADCR`

#### Decision

The daemon's CORS layer covers the read routes and not the mutating ones. That stays, and `tests/cors.rs` now asserts both halves, so it is the intent rather than a consequence of where `.merge(guarded)` happens to sit.

#### What was actually wrong

Nothing, in behaviour. The comment.

The layer says it was added so the Tauri webview could reach the API, and its allow-list carries `POST` with a note about `/api/generate` having been unreachable without it. Read together that says the list governs the write endpoints. It does not: `guarded` is merged into the router *after* the layer is applied, so no mutating route carries CORS at all, and adding a verb to the list changes nothing.

This is a trap of a specific kind — a comment describing an intent the structure no longer carries out. It cost a session exactly what it was shaped to cost: adding `PATCH` for B-87 looked like a one-line fix, and only the test written to prove it showed `POST` was not reaching the list either.

#### Why the shape is kept rather than corrected

The task allowed either: make the layer cover the guarded routes, or say plainly that it does not and why. The second, for three reasons.

Nothing needs the first. The only interface is the one the daemon serves, which is same-origin and never preflights. `apps/desktop/src-tauri` is off the release path — `dist-workspace.toml` says so in as many words.

The predicate is `is_local_origin`, which accepts any port on localhost. So covering the writes would let a page on `http://localhost:3000` — any dev server the user happens to be running — *attempt* one. The per-daemon token is what actually stops it, and that defence is sound. But there is no argument for removing a second obstacle while nothing is behind it.

And a change to what a hostile page can reach should be a decision, not a side effect of tidying. Making the current shape deliberate costs a test; making the other shape deliberate costs an argument nobody has needed to make yet.

#### What was left alone, and is worth knowing

Cross-origin **reads** are open to any local origin: a page on `http://localhost:3000` can read the whole store — entities, documents, search. That is what the layer was built to do and the comment has always said so, so it is not this task's to change. It is recorded here because it is the more surprising half of the arrangement and nothing else states it plainly.

`POST` stays in the allow-list although it reaches nothing. Removing it would leave a GET-only list that reads as though writes were considered and excluded on some other grounds; leaving it, with the comment saying it is inert, points at where the exclusion actually happens.

#### The general shape

A comment describing an intent is not evidence the structure still carries it out, and the cheapest way to find out is the test you were about to skip because the fix looked obvious.

