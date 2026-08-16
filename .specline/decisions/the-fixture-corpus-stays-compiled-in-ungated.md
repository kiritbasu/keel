<!-- specline:generated decision dec_01KZTFE58YF4AZATMPAHDQ8R87 v1 2026-08-16T14:48:42Z
     source of truth is Specline — edits here are not saved -->
# B-54 — The fixture corpus stays compiled in, ungated

**Status:** `accepted`  
**Id:** `dec_01KZTFE58YF4AZATMPAHDQ8R87`

## Decision

`keel-core::fixture` — about 2,200 lines of demo corpus — stays in every build. No `#[cfg(feature = "fixture")]`.

## Why

KEEL-162 asked for it to be gated. Working through what that costs against what it buys, it does not pay.

**What it buys.** Roughly 60 KB of a binary, and only in a build that excludes it. Cargo unifies features across a workspace build, so `cargo build --release` at the repo root would compile it into the daemon anyway. The saving only lands if the daemon is built on its own, which is not how it is built.

**What it costs.** Three things, and the third is the one that matters:

1. `keel fixture` is a shipped CLI command, so keel-cli has to enable the feature. Gating it therefore does not remove it from the product, only from one crate's dependency graph.
2. `crates/keel-core/tests/fixture_backup.rs` uses the corpus. A crate cannot enable its own optional feature for its dev-dependencies, so plain `cargo test -p keel-core` would silently skip that file unless every invocation grows `--features fixture`.
3. DECISIONS B-11 dropped `--all-features` from the definition of done because feature combinations were a hazard rather than a help here, and the note added in Phase 9 records that no workspace crate declares a feature at all. Adding one puts back the machinery that was deliberately removed, in exchange for bytes nobody has measured a problem with.

## What would change this

A measurement. If the daemon binary size or its compile time ever becomes a real complaint, the corpus is the obvious first thing to move — and the cleaner move is out of the binary entirely, into a data file `keel fixture` reads, rather than behind a feature flag.

