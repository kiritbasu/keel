<!-- keel:generated decision dec_01KZNQR16ZJKQ5MGTSF8H0VW9C v1 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-39 — The Tauri shell is suspended; the web build is the surface

**Status:** `accepted`  
**Id:** `dec_01KZNQR16ZJKQ5MGTSF8H0VW9C`

The desktop shell is not built for now. Work on the read surface happens in the browser: `npm run dev` in `apps/desktop` serves the React bundle on :1420 and proxies `/api` to the daemon on :7654.

**Why.** Nothing in `apps/desktop/src` imports a Tauri API — the frontend is already a plain web app, and SPEC §10's "same bundle, different base URL" holds today. The shell therefore buys nothing at this stage while costing a webview dependency tree and ~1.2 GB of build output. That cost was invisible until the disk filled: `target/` had reached 11 GB alongside `src-tauri/target/` at 1.2 GB, on a volume with 10 GB free.

**How it is enforced.** `apps/desktop/src-tauri/build.rs` exits with a message unless `KEEL_DESKTOP=1` is set. A loud refusal rather than deleting the crate, because the shell is coming back — this is a pause, not a reversal of the Phase 3 plan. The workspace already excluded `src-tauri`, so no ordinary `cargo` command was building it; the guard is what stops the next session from doing it by hand without noticing the cost.

**To un-suspend:** delete the guard block in `build.rs` and the note on the workspace `exclude`.

