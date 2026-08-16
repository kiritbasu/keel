<!-- keel:generated decision dec_01M010PFT9N71ZDB2EZ1BWV5Z4 v1 2026-08-14T20:52:32Z
     source of truth is Keel — edits here are not saved -->
# B-70 — One package owns both binaries, because dist builds one installer per package

**Status:** `accepted`  
**Id:** `dec_01M010PFT9N71ZDB2EZ1BWV5Z4`

## Decision

`keel-cli` is renamed `keel` (this is B-69's half of it) and now declares **both** shipped binaries. `crates/keel/src/bin/keel-daemon.rs` is a three-line shim over `keel_daemon::run`, and `keel-daemon` sets `[package.metadata.dist] dist = false`.

## Why

`dist` names an installer after the package that owns the binaries, and treats every package with binaries as a separate app. Run against the workspace as it was, `dist plan` announced two apps and two installers — `keel-cli-installer.sh` and `keel-daemon-installer.sh`.

Two things in the tree say that is wrong. PHASE-10 §1 advertises one URL ending `keel-installer.sh`, and `scripts/verify-release-tier1.sh` checks that running **one** installer leaves both `keel` and `keel-daemon` on disk. Two installers satisfies neither, and a user who ran only the advertised one would get a CLI with no daemon behind it.

There is no setting for this. `binaries` in the dist config is a per-platform override, not a way to pull another package's binaries into an archive. So the package boundary had to move.

## What moved, and what did not

Only the entry point. `crates/keel-daemon/src/main.rs` became `crates/keel-daemon/src/run.rs` with `pub fn run()`, so the argument parsing, the bind refusal and its three unit tests all stay in the crate they are about. What crossed the boundary is a `fn main` calling one function.

The two integration tests that drive the real process — `end_to_end.rs` and `wont_restart_loop.rs` — did have to move to `crates/keel/tests/`, because `CARGO_BIN_EXE_keel-daemon` only resolves in the package that declares the binary. Nothing in them changed.

## The cost

`cargo build -p keel` now builds axum and tokio. A workspace build was doing that anyway, and the `keel` binary references none of it so the linker drops it, but a single-package build of the CLI is slower than it was.

## Rejected

Publishing under the names `dist` picks and correcting §1's URL. It reads as the smaller change and is not: it leaves the user running two installers to get one product, and it would have meant rewriting the tier-1 check to expect that rather than fixing what the check was right about.

