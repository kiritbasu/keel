<!-- keel:generated spec spc_01KZR4882HZTJ4HHGZ5Y6HQDPM
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — Phase 10
## Release, distribution and install

**Goal:** a new machine reaches a working Keel from inside Claude Code, nothing compiles on it, and before every release we know exactly what we are about to break.

Rewritten 2026-08-14. The first draft was written 2026-08-11 and carried a banner saying KB had more thoughts and wanted to discuss it. That conversation happened; this is the result, and the banner is gone.

---

## What changed since the first draft

Three of the draft's load-bearing assumptions have moved.

**Phase 9 shipped.** Half the original document was about DuckDB refusing to cross-compile and what to do about it. SQLite's bundled build is one C file that cross-compiles routinely and links statically, so the release artifact is one binary and there is nothing left to decide. That section is deleted rather than updated.

**The Claude Code plugin system is the front door.** The draft's install path was `curl … | sh` followed by `claude mcp add`. A plugin registers the MCP server and wires the hooks itself, which is exactly the half of the job `plugin/install.sh` deliberately refuses to do. What a plugin cannot do is ship 37 MB binaries — so the release pipeline below is a prerequisite for the plugin, not an alternative to it.

**Keel is meant to have users.** Decided 2026-08-14, and it is the reason section 5 exists. Everything else in this document is a weekend's work. Section 5 is the phase.

Three decisions taken alongside it, since everything below rests on them:

| | |
|---|---|
| **Licence** | Apache-2.0. Permissive, with the patent grant MIT lacks. |
| **Version line** | 0.x, declared unstable. Breaking changes are allowed on a minor bump — but every one is still detected, classified and migrated. The promise is softer; the machinery is identical. |
| **Updates** | Automatic when compatible. Never automatic across a schema change. |

And one thing removed from scope entirely: **no desktop app and no mobile app.** The read surface is the local site, served by the daemon. Section 3 says what that means for the Tauri shell.

---

## 1. What the user does

Inside Claude Code:

```
/plugin marketplace add <owner>/keel
/plugin install keel@keel
/keel:setup
```

`/keel:setup` is where the weight is. It downloads the binaries for the platform, verifies them, creates the store, installs the service so nothing has to be babysat, starts the daemon, and prints each step as it goes. After a restart of Claude Code the MCP server connects and the digest appears at session start.

Between step two and step three there is a gap that has to be handled rather than hoped away: the plugin is installed, the skills and hooks are live, and the MCP server cannot connect because nothing is listening yet. Today both hooks fail open and say nothing, which is right when the daemon is merely down and wrong here — the session looks fine and Keel is silently absent. So the session-start hook gains one case: no binary and no daemon means one line saying so and naming `/keel:setup`.

For anyone not using Claude Code, the same installer stands on its own:

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/<owner>/keel/releases/latest/download/keel-installer.sh | sh
```

`--proto '=https' --tlsv1.2` are not decoration — they stop a redirect from downgrading the transfer to plaintext. `--no-setup` stops after the binaries.

The original scruple against editing `settings.json` from a shell script still holds and still applies. It never applied to `claude mcp add`, which is a supported and reversible command, and it does not apply to a plugin at all, which is the supported way to add hooks without touching anyone's settings file.

---

## 2. Zero compile, and Gatekeeper

Nothing compiles on the user's machine. Two findings from the first draft survive intact and are the reason this is cheap.

**`curl` does not quarantine.** The `com.apple.quarantine` attribute is applied by the downloading application through a key in its Info.plist, not by the kernel and not by Gatekeeper. Browsers set it. `curl`, `wget` and `git` have no Info.plist and do not. Apple treats this as intended.

So a CLI installed this way never meets Gatekeeper: no prompt, no "unidentified developer", **no Apple Developer Program, no notarization, no $99 a year**. This is why rustup, uv, bun and deno all work this way.

**One hard requirement comes with it.** On Apple Silicon every executable must carry at least an ad-hoc signature or the process is killed at exec. Native macOS toolchains add one at link time; a binary cross-compiled from Linux does not have one and dies on every M-series Mac. So macOS builds happen on macOS runners, which are free for public repositories. This is a real trap and it fails in a way that looks like a corrupt download.

---

## 3. The read surface is the local site

The React app already exists, already talks to `/api`, and already builds to `apps/desktop/dist`. Phase 10 compiles that build into `keel-daemon` with `rust-embed` and serves it from the axum server that is already there. `keel ui` opens `http://127.0.0.1:7654` in the user's browser.

**The Tauri shell comes off the install path.** A `.dmg` downloaded through a browser *is* quarantined, and then it needs Developer ID signing plus notarization — $99 a year forever, with a signing pipeline to maintain, and macOS Sequoia removed the Control-click bypass so there is no free workaround. Tauri also wants Node, Xcode command line tools, WebView2 on Windows and webkit2gtk on Linux, and it does not meaningfully cross-compile, so it is a second full release pipeline with a runner per platform.

What dropping it removes: the Apple Developer Program, notarization, Gatekeeper entirely, WebView2, webkit2gtk and its distro-version minefield, Node on the user's machine, and one of two release pipelines.

What it costs: no dock icon, no native menus, no OS file dialogs. For a read-only surface that is a good trade, and it is an ordinary shape — Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server and pgAdmin all work exactly this way.

`apps/desktop/src-tauri` is already excluded from the workspace and its build script already refuses to run without `KEEL_DESKTOP=1`. Phase 10 does not delete it; it stops pretending it is on the release path. Mobile is not in scope and has never been.

One thing this introduces that must not be skipped: anything on localhost is reachable by every other process on the machine. Section 7 covers it.

---

## 4. Two version numbers, and what each one means

This is the foundation the rest of section 5 stands on, and most of it already exists.

**Schema version** — `max(id)` over the `_keel_migrations` ledger. It moves only when the shape of the data moves. `Store::schema_version()` reads a store's, `shipped_schema_version()` reads the binary's, and `pending_migrations_at()` answers the question without opening the store properly. The guard that refuses to open a store newer than the binary is already there, written after KEEL-95, where a daemon 84 seconds behind wrote NULLs into a column it did not know about.

**Package version** — semver on 0.x. Moves for any release.

They are deliberately not the same number. The package version moves for reasons that have nothing to do with the tables, and conflating them is how a documentation fix comes to look like a data migration.

A third number already exists and is not ours: `PROTOCOL_VERSION`, the MCP wire revision. It tracks the specification and changes when the specification does.

---

## 5. Knowing what breaks, before it breaks

The problem in one sentence: today a change to a table, a tool schema or an endpoint is caught by a human reading a diff, and that stops working the moment somebody else's data is on the other end.

The answer is not more discipline. It is to make every surface emit a description of itself, store that description per release, and fail the build when the current one differs from the last without the version bump to match.

### 5.1 The contract corpus

A directory at the repository root, one subdirectory per released version:

```
contracts/
  0.5.0/
    schema.sql        normalised dump of sqlite_master, sorted
    schema_version    the integer
    tools.json        tools/list in full — names, descriptions, JSON Schemas
    api/*.json        one response per HTTP endpoint, against the fixture store
    cli.txt           --help for every subcommand and flag
    generated/        keel generate output for the fixture corpus
    store.sqlite      a fixture store built BY this version
  0.6.0/
    …
```

Everything but the last file is text and diffs cleanly. Everything is produced by one command, and nothing is written by hand.

The emitter is an integration test that writes the corpus when `UPDATE_CONTRACTS=1` and otherwise asserts against it — the same shape as `insta`, which the project already uses and already has seventeen snapshots in. No new machinery, no xtask crate, and nothing added to the shipped CLI surface.

The MCP half of this is largely done: `snapshots__tools_list.snap` already pins the tool surface, and the Phase 9 engine swap produced zero diffs across both snapshot suites, which is the whole return on the Phase 0 trait boundary. What is missing is that the snapshots compare *current against current*. A released version is never kept, so there is nothing to compare a release against.

### 5.2 What counts as breaking

The classifier compares the current emission against the newest stored version and sorts every difference into one of three buckets. It is a table rather than a judgement call, because a judgement call at release time is made by whoever is tired.

| Surface | Additive — minor bump | Breaking — needs a plan |
|---|---|---|
| **Store schema** | new table; new nullable column; new index | dropped or renamed table or column; narrowed type; `NOT NULL` without a default; changed primary or foreign key |
| **MCP tools** | new tool; new optional argument; new field in a response | removed or renamed tool; new required argument; removed response field; narrowed enum; changed argument type |
| **HTTP API** | new endpoint; new optional query parameter; new response field | removed endpoint; removed field; changed status code; changed envelope |
| **CLI** | new subcommand; new optional flag | removed or renamed subcommand or flag; changed default; changed exit code |
| **Generated markdown** | — | any layout change at all |
| **Backup bundle** | new file in the bundle | changed layout; changed manifest fields |

Two rows deserve their reasoning.

**Generated markdown has no additive column.** Users commit these files. A change to how a heading is rendered is a diff in every repository that has ever run `keel generate`, which is not a breaking change in any technical sense and is exactly as annoying as one. It gets announced.

**A tool description change is not breaking, and is not nothing.** The descriptions are the only documentation a model gets — they are the product. They do not break a caller, so they are additive; the classifier still prints them, because a silent rewrite of the thing that decides tool selection is worth a human reading.

### 5.3 The test that actually protects data

Everything above catches a change. One test catches the consequence.

`contracts/<version>/store.sqlite` is a small store built by that version from `keel fixture`. The test opens every one of them with the current binary, runs `keel migrate`, runs `fsck`, and asserts that row counts per table and content hashes per document survive. A dozen versions of a fixture corpus is a few megabytes.

That is the test that stands between a migration and somebody's year of notes. It is also the only one here that cannot be reconstructed later: a vintage store has to be captured at the time, because once the code that wrote it is gone there is no way to make another.

Two more, both cheap:

- **The forward guard.** The current binary must refuse a store whose schema version is higher than its own, with a message naming the upgrade. The behaviour exists; the assertion does not.
- **Round-trip across versions.** Back up with version N, restore with N+1, diff. The backup round-trip test already exists within one version; this points it across two.

### 5.4 The release gate

A release is refused unless all of these hold:

1. The contract diff is empty, or every difference is classified and the version bump matches the highest severity found.
2. Every vintage store in `contracts/` migrates green and passes `fsck`.
3. `keel generate keel --check` is clean.
4. Every breaking change names its migration and the sentence the user will see.
5. Release notes carry a **Breaking** section generated from the classification rather than written from memory.

Point 5 is the one that pays for the rest. Release notes assembled by hand from a week of commits are how a breaking change reaches users unannounced.

### 5.5 Deprecation, while on 0.x

Breaking changes are allowed. Unannounced ones are not.

- A removed tool, subcommand or flag keeps working as an alias for **two minor releases**, warning each time, then goes.
- A column is never dropped in the same release that stops writing it. Stop writing, ship, drop next time — so a rollback in between still has its data.
- Every migration is reversible or takes a backup first. `/keel:setup` and the updater both back up before migrating regardless.

### 5.6 CI has never run

Worth stating plainly, because everything above assumes a working pipeline and there is not one. `.github/workflows/ci.yml` has been in the tree since it was written and has never been executed by anything: `git remote -v` is empty, so there is no remote and no repository for Actions to run in. The trigger also named `main` while trunk was `master`, which is fixed in f65be15.

So the Linux matrix entry has never tested anything on Linux, and every check in this project's history was run by hand on one Mac. **Creating the remote is step zero of Phase 10**, and it comes before the contract corpus rather than after, because a corpus that only one machine can generate is a corpus with one platform's opinion baked into it.

---

## 6. Version drift, and updating without asking

The plugin updates over git through `claude plugin update`. The binaries update from a release. Those are two channels and they will fall out of step — which is TQ-26 again, the hand-copied hooks that drifted while nothing said so, except now across a network and in someone else's install.

### 6.1 The handshake

`/api/health` already exists and reports a version. It gains `schema_version` and `min_plugin_version`, and the plugin's manifest gains `min_daemon_version`. The session-start hook compares them and has three outcomes.

**They match.** Silence, as today.

**The binary is behind, and no schema change lies between them.** The update is applied without asking. The user sees one line at the top of a session: `Keel updated to 0.6.1.` Nothing else changes, nothing is confirmed, and the next session says nothing at all.

**A schema change or a breaking classification lies between them.** Nothing is applied. The hook injects one line naming what changes and pointing at `/keel:update`, which backs up first, prints what will happen, and waits. The user's data is about to be rewritten in a way a rollback will not undo, and that is a decision with an owner.

### 6.2 Where the update actually runs

Not in the hook. The hook runs before the user's first word and must never block a session — that is constraint 1 on `session-start.sh` and it does not bend for this.

So the daemon does it. Once a day it checks the release feed, and when it finds a compatible update it downloads it, verifies the checksum and the GitHub build attestation, and stages it beside the current binary. It applies the staged binary at its next start, which the service supervisor provides for free — launchd `KeepAlive` and systemd `Restart=always` both restart a daemon that exits. Swapping a binary that is not running removes a whole family of problems.

The hook only reports. `keel update` forces it now. `KEEL_AUTO_UPDATE=0` turns it off, and `/keel:setup` asks once which the user wants.

Three limits on this, none negotiable:

- **Never across a schema version.** The check is `shipped_schema_version()` on the candidate against the store's, and unequal means stop and ask.
- **Never without verification.** Checksum and attestation both, and a failure means staying put and saying so — never falling back to unverified.
- **Never silent about what happened.** One line naming the new version, once. An update the user cannot see is one they cannot report a bug against.

### 6.3 Why not fully automatic

Because a bad migration reaching every install before anyone can stop it is a class of failure this project has no way to undo, and because a self-updating binary is a supply-chain target — one compromised release becomes remote execution on every machine that has it. Splitting on schema version puts the automatic path where the blast radius is a restart, and a human in front of the path where the blast radius is their data.

---

## 7. The port, and the network

**Programmatic, not a question the user answers.** The daemon tries 7654. If something is already there it asks `/api/health`, and:

- another `keel-daemon` on the same store — this is the single-writer design working. Exit cleanly saying so; that is not an error.
- anything else — walk 7655, 7656, up to 7664 for a free port.

Whatever it binds, it writes `~/.keel/daemon.json` atomically with the URL, and removes it on shutdown. The CLI reads that file first, then `$KEEL_DAEMON_URL`, then the default — which also fixes `keel` today assuming 7654 with no way to learn otherwise.

The plugin's `.mcp.json` becomes `"url": "${KEEL_DAEMON_URL}/mcp"` with 7654 as the declared default, and `/keel:setup` reads `daemon.json` and records a non-default URL through the plugin's own config. The user never types a port and never edits a file inside an installed plugin, which the next update would overwrite anyway.

**Binding.** `KEEL_BIND` accepts any address today, so a user who wants to reach the site from a laptop can expose an unauthenticated write API to their network in one environment variable. It starts refusing anything that is not loopback unless an explicit flag says otherwise, and the flag's help says what it is agreeing to.

The attack that actually matters is already handled: the Origin and Host checks in `keel-daemon::http` stop a web page reaching the daemon by DNS rebinding, with a test for the rebinding case. The remaining gap is local processes, which is why `keel ui` opens the site with a per-session token in the URL. Today the API is read-only and unauthenticated and that is defensible; it stops being defensible the moment a write or intake endpoint exists, which is what que_01KZSQHK2C0CTKN36WJ9G4ZHQC is about.

---

## 8. The hooks move into the binary

`keel hook session-start` and `keel hook stop`, replacing the two shell scripts.

This is not tidying. The scripts need `python3` and `curl`, neither of which is declared anywhere; `install.sh` warns about a missing `jq`, which neither script uses, so the one dependency check that exists is checking the wrong thing. A clean macOS has no `python3` until the Xcode command line tools are installed, so on a fresh machine the hooks silently do nothing — which looks exactly like Keel not working. And bash means Windows is out, while section 10 lists a Windows target.

The fourth reason is the one that matters most here. **Nothing in the workspace or in CI executes either script.** KEEL-192 — the Stop hook nagging in projects Keel had never heard of — was found by reading and fixed by reading, and the fix is guarded by nothing. Every rule in section 5 is about surfaces that describe themselves and are tested; the hooks are the one surface that does neither, and moving them into the binary is what fixes that rather than another checklist item.

The plugin still ships `hooks/hooks.json`. It calls `${CLAUDE_PLUGIN_ROOT}`-relative wrappers that exec the binary, so the hooks keep working when the binary is missing — by saying so, which is section 1's gap.

---

## 9. Embeddings

`fastembed` is fine on the compile question and awkward on the download one. The ONNX Runtime is fetched prebuilt and statically linked at our build time — `ort-sys` pulls a 77 MB archive on every build already, so turning embeddings on adds no build step. The model is the cost: `bge-small-en-v1.5` is 133 MB on first use, or 66 MB quantized.

So: cache path set explicitly under `~/.keel/models` rather than fastembed's default, which is relative to the working directory and is a bug waiting to be found in a long-running daemon. Opt-in behind a visible prompt in `/keel:setup`, never a silent pull. Keyword search works without any of it, which is what makes opt-in honest rather than a downgrade.

There is no prebuilt ONNX Runtime for Linux ARM, so that target either ships without embeddings or is not shipped.

---

## 10. The pipeline

`dist` at 0.32, released May 2026. Its sponsoring company wound down its commercial product and there was a six-month gap in 2025, during which Astral forked it for `uv`; those changes were merged back and releases have shipped every two to three months since. It is maintained by the original authors plus Astral. If it ever stalls, the workflow it generates is readable and can be vendored.

- Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc`.
- Shell and PowerShell installers, checksums embedded in the installer, GitHub artifact attestations on so build provenance can be verified.
- macOS builds on macOS runners, for the ad-hoc signature.

**One bug to patch on day one.** The installer `dist` generates verifies downloads by calling `sha256sum`, and stock macOS does not have it — it has `shasum`. The generated script prints "skipping sha256 checksum verification" and returns success. On our primary platform, integrity checking silently does nothing. Fall back to `shasum -a 256`, and send the fix upstream.

**One thing to diary.** GitHub's macOS Intel runners retire in August 2027. Before then, `x86_64` macOS either moves to cross-compilation with an explicit signing step or is dropped.

---

## 11. Where it lives

One public repository, HTTPS remote — SSH authentication is broken on this machine and anything cloning over `git@github.com` fails.

```
github.com/<owner>/keel
├── .claude-plugin/marketplace.json    "source": "./plugin"
├── plugin/                             the plugin, already valid
├── contracts/                          section 5
├── crates/
└── apps/desktop/                       the local site
```

The plugin stays in this repository rather than getting its own. Its skills describe behaviour the binary implements, so one commit changes both and there is nothing to keep in step — which is the TQ-26 drift class removed by construction rather than by discipline. The cost is that installing the plugin clones the Rust source to get four text files. A few megabytes, and worth it.

Binaries go to GitHub Releases. `keel.sh` is a redirect that can be added at any time and is not a prerequisite; the release URL works today and costs nothing.

Apache-2.0 replaces the deliberately-absent licence, which means `publish = false` and `deny.toml`'s `private.ignore` both come out.

---

## 12. Exit criteria

- **A fresh machine reaches a working Keel from three lines inside Claude Code, and nothing compiles on it** — verified on a clean container and on a Mac that has never had Rust installed.
- No artifact on the install path requires an Apple Developer ID.
- `jq` and `python3` are required by nothing, and the hooks run on Windows.
- The installer verifies its own downloads on macOS — checked by corrupting an archive on purpose and confirming it refuses.
- A release is one CI run and produces every target.
- **The contract corpus exists, CI fails on an unclassified difference, and every stored vintage store migrates green.**
- A compatible update applies itself and announces itself in one line; an update crossing a schema version stops and asks.
- The daemon picks a working port without the user being asked, and refuses a non-loopback bind unless told.

The last three are new and are the ones that make this a product rather than a distribution.

---

## Appendix — what this rests on

| Claim | Source |
|---|---|
| `curl` does not set `com.apple.quarantine` | quarantine is applied by the downloading app via its Info.plist; curl has none. Apple treats this as intended |
| Apple Silicon kills unsigned executables | an ad-hoc signature is required at exec; native macOS toolchains add one at link time, cross-compiles do not |
| A browser-downloaded `.dmg` needs notarization | $99/yr, and macOS Sequoia removed the Control-click bypass |
| Plugins may declare an HTTP-transport MCP server | the official marketplace ships `linear`, `github` and `vercel` as `"type": "http"`; `./.mcp.json` is the default path and needs no manifest entry |
| `dist` installer checksum bug | its generated script calls `sha256sum`, which stock macOS does not have; it then skips verification and returns success |
| `dist` is maintained | 0.32.0, May 2026; Astral forked it during a 2025 gap and the changes were merged back upstream |
| ONNX Runtime does not compile | `ort` downloads a prebuilt static library at build time |
| fastembed model download | 133 MB on first use, 66 MB quantized; default cache is relative to the working directory |
| GitHub macOS Intel runners | retire August 2027 |
| Serving a UI from a local daemon is ordinary | Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server, pgAdmin |
| CI has never run | `git remote -v` is empty and no keel repository exists under the account; the workflow has never been executed by anything |
| The MCP surface already survives an engine swap | Phase 9 changed the storage engine and both insta suites produced zero diffs |
