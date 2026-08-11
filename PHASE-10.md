<!-- keel:generated spec spc_01KZR4882HZTJ4HHGZ5Y6HQDPM
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — Phase 10
## Release, distribution and install

## Where this sits

Phases 0–5 are the original plan. Phase 6 (*Make the tracker real*) shipped on 2026-08-11; Phase 7 (*Clean up the footprint*) closed alongside it. This document is one of three that follow:

| | |
|---|---|
| **Phase 8 — The working loop** | verbs for doing work, filing issues from the app, and the app made legible |
| **Phase 9 — One database** | DuckDB + Lance → SQLite |
| **Phase 10 — Release, distribution and install** | one pasted line, nothing compiles |

Written 2026-08-11 for KB, in plain English. Where a term from the codebase appears it is explained the first time.

**Goal:** a new machine reaches a working Keel from one pasted line, and nothing compiles on it — not even out of sight.

> **KB has more thoughts on this phase and wants to discuss it.** Treat what follows as the current state of the argument rather than a settled plan. The two findings most worth carrying into that conversation are that curl-delivered binaries never meet Gatekeeper, and that the Tauri app is the only thing on the install path that would need notarization.

---

Two requirements: a single copy-paste statement, and no compilation on the user's machine — not even hidden. Both are achievable. The second one has a consequence for Phase 9 that I did not expect and will state plainly.

### What the user types

```
curl --proto '=https' --tlsv1.2 -LsSf https://keel.sh/install.sh | sh
```

That is the whole thing. It installs the binaries, creates the store, registers the MCP server with Claude Code, installs the launch agent, starts the daemon, and prints each step as it does it. `--no-setup` stops after the binaries.

The `--proto '=https' --tlsv1.2` flags are not decoration: they stop a redirect from downgrading the transfer to plaintext.

The existing installer deliberately refuses to touch Claude's settings, on the grounds that rewriting somebody's config from a shell script is the kind of helpfulness that is indistinguishable from damage the one time it gets it wrong. That scruple was right about *hand-editing `settings.json`* and it does not apply to `claude mcp add`, which is a supported, reversible command. So the one-liner can finish the job, and the objection resolves rather than being overridden.

### Zero compile: what it actually takes

**The good news is bigger than expected. `curl` does not quarantine.** The `com.apple.quarantine` attribute is not applied by the kernel or by Gatekeeper — it is applied by the *downloading application*, opt-in, through a key in its Info.plist. Browsers set it. `curl`, `wget` and `git` have no Info.plist at all and do not. Apple treats this as intended rather than as a gap.

So a CLI installed by `curl | sh` never meets Gatekeeper: no prompt, no "unidentified developer", **no Apple Developer Program, no notarization, no $99 a year**. This is why rustup, uv, bun and deno all work this way.

**One hard requirement comes with it.** On Apple Silicon every executable must carry at least an *ad-hoc* signature or the process is killed at exec. Native macOS toolchains add one automatically at link time; a binary cross-compiled from Linux does not have one and would be killed on every M-series Mac. So macOS builds happen on macOS runners, which are free for public repositories. This is a real trap and it fails in a way that looks like a corrupt download.

### The decision that removes the most: drop the Tauri app

A `.dmg` downloaded through a browser **is** quarantined, and then it needs Developer ID signing plus notarization — $99 a year, forever, with a signing pipeline to maintain. macOS Sequoia removed the Control-click bypass, so there is no free workaround any more. Tauri also needs Node, Xcode command line tools, WebView2 on Windows and webkit2gtk on Linux, and it cannot meaningfully cross-compile, so it is a second full release pipeline with a runner per platform.

**Instead: embed the built interface into the daemon and open a browser.** The React build is compiled into `keel-daemon` at our build time with `rust-embed` and served from the axum server that already exists; `keel ui` opens `http://127.0.0.1:7654`.

What that removes: the Apple Developer Program, notarization, Gatekeeper entirely, WebView2, webkit2gtk and its distro-version minefield, Node from the user's machine, and one of the two release pipelines.

What it costs: no dock icon, no native menus, no OS file dialogs, and the interface renders in the user's browser rather than a webview. For an app that is read-only apart from filing an issue, that is a good trade — and it is a thoroughly ordinary shape. Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server and pgAdmin all work exactly this way.

One thing it introduces that must not be skipped: anything bound to localhost is reachable by every other process on the machine, and by a web page through DNS rebinding. So the daemon binds loopback only and `keel ui` opens the interface with a per-session token in the URL. Today the API is read-only and unauthenticated, which is defensible; the moment §8B adds an intake endpoint, it stops being.

Tauri stays possible later as an optional wrapper. It just stops being on the install path.

### The compile problem, and what it means for Phase 9

Nothing compiles on the user's machine either way. The question is what *our* build has to do, and this is where DuckDB bites.

`duckdb-rs`'s own README says cross-compiling with `bundled` is "best-effort (not covered by CI)" because it compiles DuckDB from C++ source and needs a working C++ cross-compiler for the target. There are two ways out:

- **`DUCKDB_DOWNLOAD_LIB=1`** — the build script downloads DuckDB's official prebuilt library and links that instead. The 22-minute build disappears. But it links *dynamically*, so every release tarball carries a 40–60 MB `libduckdb` beside the binaries with an rpath pointing at it, and the single-file property is gone. It also happens to restore the ICU extension that `bundled` omits.
- **A native runner per platform**, paying the 22 minutes four times per release.

With SQLite there is no problem to solve. `rusqlite`'s bundled build is one C amalgamation file, compiles in well under a minute, cross-compiles routinely, and links statically. The release artifact is **one binary**.

**So the zero-compile requirement changes my recommendation about Phase 9.** Yesterday I called it optional and last, and that was right when the argument was only build time. It is not right now. Shipping binaries for four targets, forever, with a vendored C++ database in the tree is a maintenance cost that recurs at every release; with SQLite it is a solved problem that nobody thinks about again. Phase 9 is what makes 8D cheap to keep rather than merely possible to do once.

**My recommendation: do Phase 9 before the release pipeline**, and use `DUCKDB_DOWNLOAD_LIB=1` in the interim so that no build anywhere takes 22 minutes while you decide.

### Embeddings

`fastembed` is fine on the compile question and awkward on the download one. The ONNX Runtime it needs is fetched **prebuilt** and statically linked at our build time — nothing compiles, on our machines or anyone's. But the embedding model is a **133 MB download on first use**, and fastembed's default cache directory is relative to the working directory rather than the home directory, which for a long-running daemon is a bug waiting to be found.

So: set the cache path explicitly, make embeddings opt-in behind a visible prompt rather than a silent pull, and use the 66 MB quantized variant. Keyword search works without any of it, which is what makes opt-in honest rather than a downgrade. One target to note: there is no prebuilt ONNX Runtime for Linux ARM, so that target either drops embeddings or drops out.

### The pipeline

`dist` (formerly cargo-dist) at 0.32, released May 2026. Worth a sentence on its health since it is load-bearing: its sponsoring company wound down its commercial product and there was a six-month gap in 2025, during which Astral forked it for `uv`; those changes were merged back upstream and releases have shipped every two to three months since. It is maintained by the original authors plus Astral. The fallback, if it ever stalls, is that the workflow it generates is readable and can be vendored.

- Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.
- Shell and PowerShell installers, checksums embedded inline in the installer itself, GitHub artifact attestations on so the build provenance can be verified.
- macOS builds on macOS runners, for the ad-hoc signature.

**One bug to patch on day one.** The installer `dist` generates verifies downloads by calling `sha256sum` — and stock macOS does not have `sha256sum`, it has `shasum`. The generated script prints "skipping sha256 checksum verification" and returns success. On our primary platform, integrity checking silently does nothing. Fall back to `shasum -a 256`, and send the fix upstream.

**And one thing to diary.** GitHub's macOS Intel runners retire in August 2027. Before then, x86_64 macOS either moves to cross-compilation with an explicit signing step or is dropped.

### The rest of the setup story

- `keel-daemon install-service` — a launchd plist or systemd unit, so nobody babysits a terminal. Run by the installer.
- The two remaining hooks become `keel hook session-start` and `keel hook stop`, which removes `jq` and `python3` from the requirements entirely and puts the logic somewhere testable. `python3` is needed by both hooks today and appears in none of the installer's warnings.
- Geist and Geist Mono ship inside the binary with the interface, so there is no font fetch either.

### Phase 10 exit criteria

- **A fresh machine reaches a working Keel from one pasted line, and nothing compiles on it** — verified on a clean container and on a Mac that has never had Rust installed.
- No artifact on the install path requires an Apple Developer ID.
- `jq` and `python3` are no longer required by anything.
- The installer verifies its own downloads on macOS — checked by deliberately corrupting an archive and confirming it refuses.
- A release is one CI run and produces every target.

**You said you have more thoughts on this — so treat the above as the current state of the argument rather than a settled plan.** The two findings worth carrying into that conversation are that curl-delivered binaries never meet Gatekeeper, and that the Tauri app is the only thing on the install path that would need notarization.

---


---

## Appendix — what this rests on

| Claim | Source |
|---|---|
| `curl` does not set `com.apple.quarantine` | quarantine is applied by the downloading app via its Info.plist; curl has none. Apple treats this as intended |
| Apple Silicon kills unsigned executables | an ad-hoc signature is required at exec; native macOS toolchains add one at link time, cross-compiles do not |
| A browser-downloaded `.dmg` needs notarization | $99/yr, and macOS Sequoia removed the Control-click bypass |
| DuckDB cross-compilation | `duckdb-rs` README: bundled cross-compilation is "best-effort (not covered by CI)" |
| `dist` installer checksum bug | its generated script calls `sha256sum`, which stock macOS does not have; it then skips verification and returns success |
| `dist` is maintained | 0.32.0, May 2026; Astral forked it during a 2025 gap and the changes were merged back upstream |
| ONNX Runtime does not compile | `ort` downloads a prebuilt static library at build time |
| fastembed model download | 133 MB on first use, 66 MB quantized; default cache is relative to the working directory |
| GitHub macOS Intel runners | retire August 2027 |
| Serving a UI from a local daemon is ordinary | Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server, pgAdmin |
