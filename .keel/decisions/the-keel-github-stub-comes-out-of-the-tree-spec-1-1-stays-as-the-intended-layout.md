<!-- keel:generated decision dec_01KZYFS0PJY5RPXCMN15GC2AS7 v1 2026-08-13T21:19:00Z
     source of truth is Keel — edits here are not saved -->
# B-63 — The keel-github stub comes out of the tree; SPEC §1.1 stays as the intended layout

**Status:** `accepted`  
**Id:** `dec_01KZYFS0PJY5RPXCMN15GC2AS7`

# The keel-github stub comes out of the tree

`crates/keel-github` was 24 lines: a `main.rs` that printed `keel-github: not yet implemented — Phase 4`, a Cargo.toml, no lib target, no dependents, and a `tempfile` dev-dependency for tests that were never written. Its own doc comment said it existed "so §1.1's layout is real and so nothing drifts into the daemon that belongs here".

It is removed, along with its workspace member entry.

## Why the stated reason did not hold

The first half — making §1.1's layout real — is the part that looked like it should block this. It does not, because the layout in §1.1 is not a description of the tree. That same diagram lists `apps/web/`, which has never existed as a directory in this repository. So the diagram was already the intended shape rather than the current one, and `keel-github` was the odd member: the only planned component with an empty crate standing in for it. Removing it makes the two consistent, and §1.1 goes on meaning what it already meant.

The second half — stopping webhook logic drifting into the daemon — was doing nothing a compiler enforces. An empty crate does not repel code; §9 saying the receiver is a separate binary that calls the daemon's API is what does that, and it still says it.

## What it cost to keep

It compiled on every `cargo build --workspace` and `cargo test --workspace`, and produced a test binary that ran zero tests. Small, but it was pure overhead against a component nobody has started.

## When Phase 4 arrives

`cargo new` costs nothing. The spec still names the crate, its job and its deployment shape in §1.1 and §9, which is the whole of what the placeholder was preserving.

