<!-- keel:generated decision dec_01KZNHQ0SMBXVKYF3SA85W9VZ7 v1 2026-08-10T18:53:23Z
     source of truth is Keel — edits here are not saved -->
# B-34 — The desktop app routes on the hash, and the router is hand-written

**Status:** `accepted`  
**Id:** `dec_01KZNHQ0SMBXVKYF3SA85W9VZ7`

## Context

Phase 6 needs every screen, project, document, search and task to have an address. The app had no router at all.

## Decision

**Route on `location.hash`, with a hand-written route table in `apps/desktop/src/lib/router.ts`.** No routing dependency.

## Reasoning

Two separate calls, both pointing the same way.

**The hash rather than the path.** A path-based router needs the server to fall back to `index.html` for any deep URL. Vite's dev server does that; Tauri's asset protocol does not. So `/projects/keel/board` would have 404'd on reload in the built app — precisely the failure routing was added to fix, and one that would only have shown up in the packaged build. The hash never reaches a server, so one bundle behaves identically in dev, in the Tauri webview, and in the future static web build SPEC §10 asks for.

**Hand-written rather than a library.** Eleven route patterns and a query string come to about 200 lines including the comments. A router library would need roughly as much configuration and would add a dependency to the surface. Same reasoning as B-14 for components.

Two properties are held by tests rather than by care: `parseHash(toHash(r)) === r` for every route the app can build, and an address matching no route falls back to Home while keeping its query. `App` then canonicalises — an address that means nothing, or names a project-scoped screen with no project, is rewritten with `replace` so it never becomes a Back destination.

## Reversible?

Yes, cheaply. Every call site goes through `href`, `navigate` and `useRoute`; swapping to paths or to a library means rewriting one module.

