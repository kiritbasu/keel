<!-- specline:generated decision dec_01M05D3X5QVJ0S6B4R9BY54MAK v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-81 — Keel becomes Specline: the store migrates itself, the task key does not change, and everything else is a clean break

**Status:** `accepted`  
**Id:** `dec_01M05D3X5QVJ0S6B4R9BY54MAK`

KB asked for the product to be renamed from Keel to Specline, and for a phase that finds every surface the old name is load-bearing on rather than a find-and-replace. Four choices shape the work, and they are not all the same choice.

## The task key stays KEEL

`KEEL-42` is composed from the project row's `key` column, not stored on the task. Changing it to `SPCL` or `SPEC` would strand or require rewriting 763 references in tracked files and 145 inside stored document bodies — and rewriting stored bodies means full-document writes, which is the one editing operation this project has already identified as able to go wrong with nothing downstream noticing.

The contract says task ids are stable and never reused. That rule was written about not recycling numbers, but it answers this too. A project named Specline whose tasks read `KEEL-42` is mildly odd and completely honest: it says the work started under a different name, which is what happened.

`SPEC` was also rejected on its own merits — it collides with the `spec` artifact type, so `SPEC-4` would read as a spec rather than as a task.

## The store migrates itself; the interfaces do not

These look like the same decision and they are opposites, so the reason is worth stating.

**The store moves itself.** `~/.keel` becomes `~/.specline`, taken once on first run when the new directory does not exist and the old one does, with a marker and a line on stdout saying what happened. The store is the only irreplaceable thing in the rename, and a missing store fails *quietly* — the code sees a fresh home and creates an empty one, which is this project's defining failure shape.

**Everything else breaks loudly and is therefore left to break.** The 27 environment variables, the thirteen tool names, the MCP server name, the plugin, the skills and the binaries all get no compatibility shim. A renamed environment variable falls back to a default or exits; a renamed tool is absent from the namespace; a renamed binary is not on `PATH`. Every one of those is visible in the first second. Carrying two names for each of them indefinitely would cost more than it protects.

KB confirmed this Mac is the only install, which is what makes the clean break available at all. The store still migrates itself, because the argument for that is about how the failure presents rather than about who is affected.

## The repository is renamed in place

`kiritbasu/keel` becomes `kiritbasu/specline`. GitHub redirects clones, remotes, API calls and release asset downloads, so the five published releases and the installers that point at them keep resolving. A new repository would lose that on the day the old one was archived, and lose the history and the issues with it.

The redirect does not cover the self-hosted runner, which is registered against the repository name and runs the macOS release builds on this Mac. It needs re-registering, and a rename that skips it produces a release leg that queues forever without an error.

## What is deliberately not in scope

- `product/` — not named after the product, does not move.
- The `embeddings` feature — not the product name.
- `_keel_migrations` — an internal table name nobody outside the schema module reads. Renaming it is a migration whose failure mode is a store that looks unmigrated and runs every migration again from the top. It buys nothing and it can destroy data, so it stays.
- Historical prose. Changelog entries, closed rows, past decisions, quoted error messages and the journal keep the old name, because rewriting them produces a record of decisions nobody made about a product that did not exist yet.

