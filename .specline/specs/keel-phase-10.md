<!-- specline:generated spec spc_01KZR4882HZTJ4HHGZ5Y6HQDPM v5 2026-08-19T08:16:32Z
     source of truth is Specline — edits here are not saved -->
# Keel — Phase 10

**Status:** `draft`  
**Kind:** `note`  
**Id:** `spc_01KZR4882HZTJ4HHGZ5Y6HQDPM`

# Keel — Phase 10
## Release, distribution and install

**Goal:** a stranger reaches a working Keel without compiling anything, and before every release we know exactly what we are about to break.

Revision history, because the shape of this document has changed three times and the reasons matter more than the diff. Drafted 2026-08-11 against DuckDB and a Tauri app. Rewritten 2026-08-14 when KB decided Keel is meant to have users. Rewritten again the same day after a stress test found six things in that rewrite that did not hold — two of them safety mechanisms that would have passed CI while doing nothing. Amended again after §5.6's determinism check was actually run, which answered it and corrected two claims this document had been making about `keel fixture` and about what a hundred runs would show.

---

## Settled

| | |
|---|---|
| **Licence** | Apache-2.0. Permissive, with the patent grant MIT lacks. |
| **Version line** | 0.x, declared unstable. Breaking changes allowed on a minor bump; every one still detected, acknowledged and migrated. |
| **Updates** | Automatic when compatible. Never automatic across a schema change. |
| **Order** | Phase 11 finishes first. Then Phase 10 in full. |
| **Targets** | macOS arm64, macOS x86_64, Linux x86_64. **No Windows.** |
| **Cadence** | Every few days at first, then weekly, then fortnightly. |
| **Surfaces** | The local site, served by the daemon. No desktop app, no mobile app. |

Cadence is not scheduling trivia — it is the constraint that decided section 5. At a release every few days, anything stored per release accumulates thirty copies in six months, and a gate with a manual review step is the step that gets skipped.

---

## 1. What the user does

Three commands and a restart:

```
/plugin marketplace add <owner>/keel
/plugin install keel@keel
/keel:setup
```

then restart Claude Code.

The restart is not incidental and is not hidden here the way it was in an earlier draft. Claude Code connects MCP servers when it starts, and at step two nothing is listening yet — the binary arrives at step three. So the honest count is three commands and a restart, and the exit criteria in section 13 say that rather than something tidier.

`/keel:setup` is where the weight is. It downloads the binaries for the platform, verifies them, creates the store, resolves the port, installs the service, starts the daemon, asks once about embeddings and once about automatic updates, and prints each step as it goes.

**The command runs one script, not a sequence of steps a model improvises.** A slash command is a prompt, so anything expressed as "then run this, then run that" is executed non-deterministically and behind a permission prompt each time. `/keel:setup` therefore does one thing: run `${CLAUDE_PLUGIN_ROOT}/scripts/setup.sh`. The determinism lives in the script, where it can be tested.

Between step two and step three the plugin is installed and the MCP server cannot connect. Section 8 says how the session says so rather than looking fine.

For anyone not using Claude Code the same installer stands alone:

```
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/<owner>/keel/releases/latest/download/keel-installer.sh | sh
```

`--proto '=https' --tlsv1.2` stop a redirect from downgrading the transfer to plaintext. `--no-setup` stops after the binaries.

The original scruple against editing `settings.json` from a shell script still holds. It never applied to `claude mcp add`, which is supported and reversible, and it does not apply to a plugin at all.

---

## 2. Zero compile, and Gatekeeper

Nothing compiles on the user's machine. Two findings from the first draft survive intact.

**`curl` does not quarantine.** The `com.apple.quarantine` attribute is applied by the downloading application through a key in its Info.plist, not by the kernel and not by Gatekeeper. Browsers set it. `curl`, `wget` and `git` have no Info.plist and do not. Apple treats this as intended.

So a CLI installed this way never meets Gatekeeper: no prompt, no "unidentified developer", **no Apple Developer Program, no notarization, no $99 a year**. This is why rustup, uv, bun and deno all work this way. It is also directly checkable — `xattr -l` on a fresh download, which section 12 does.

**One hard requirement comes with it.** On Apple Silicon every executable must carry at least an ad-hoc signature or the process is killed at exec. Native macOS toolchains add one at link time; a binary cross-compiled from Linux does not and dies on every M-series Mac. So macOS builds happen on macOS runners, free for public repositories. This fails in a way that looks like a corrupt download.

---

## 3. The read surface is the local site

**This section has a prerequisite and it is open.** `que_01KZSQHK2C0CTKN36WJ9G4ZHQC` — does a browser-served write or intake endpoint require amending hard constraint 7 — is unanswered, and there is a task blocked behind it. An earlier draft shipped a browser-served UI without mentioning either. Because the order is now "Phase 10 in full", that question is a Phase 10 prerequisite: **it gets answered before section 3 starts**, not discovered halfway through it.

The React app already exists, already talks to `/api`, and already builds to `apps/desktop/dist`. Phase 10 compiles that build into `keel-daemon` with `rust-embed` and serves it from the axum server already there. `keel ui` opens `http://127.0.0.1:7654` in the user's browser.

**The Tauri shell comes off the install path.** A `.dmg` downloaded through a browser *is* quarantined, and then it needs Developer ID signing plus notarization — $99 a year forever with a signing pipeline to maintain, and macOS Sequoia removed the Control-click bypass. Tauri also wants Node, Xcode command line tools, WebView2 and webkit2gtk, and does not meaningfully cross-compile, so it is a second full release pipeline with a runner per platform.

Dropping it removes the Apple Developer Program, notarization, Gatekeeper entirely, WebView2, webkit2gtk and its distro-version minefield, Node on the user's machine, and one of two pipelines. It costs a dock icon, native menus and OS file dialogs. For a read-only surface that is a good trade, and it is an ordinary shape — Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server and pgAdmin all work this way.

`apps/desktop/src-tauri` is already excluded from the workspace and already refuses to build without `KEEL_DESKTOP=1`. Phase 10 does not delete it; it stops pretending it is on the release path. Mobile has never been in scope.

One consequence for the pipeline: embedding the site means **Node is needed in the release job**, which the workflow `dist` generates will not do by itself.

---

## 4. Two version numbers

**Schema version** — `max(id)` over the `_keel_migrations` ledger, four today. It moves only when the shape of the data moves. `Store::schema_version()` reads a store's, `shipped_schema_version()` reads the binary's, `pending_migrations_at()` answers without opening the store properly, and the guard refusing a store newer than the binary already exists — written after KEEL-95, where a daemon 84 seconds behind wrote NULLs into a column it did not know about.

**Package version** — semver on 0.x, moves for any release.

Deliberately not the same number. The package version moves for reasons that have nothing to do with the tables, and conflating them makes a documentation fix look like a data migration.

`PROTOCOL_VERSION`, the MCP wire revision, is a third and is not ours.

---

## 5. Knowing what breaks, before it breaks

Today a change to a table, a tool schema or an endpoint is caught by a human reading a diff. That stops working when somebody else's data is on the other end.

### 5.1 One contracts directory, and git keeps the history

Every surface emits a description of itself into one always-current directory, checked in:

```
contracts/
  schema.json        PRAGMA table_info / index_list / foreign_key_list, per table
  schema_version     the integer
  tools.json         tools/list in full — names, descriptions, JSON Schemas
  api/*.json         one response per endpoint, against the fixture store
  cli.txt            --help for every subcommand and flag
  generated/         keel generate output for the fixture corpus, banner stripped
  stores/v<n>.sqlite one fixture store per schema version — see 5.3
```

CI regenerates it and fails if the tree is dirty. That is the same pattern as `keel generate keel --check`, which this project already lives by and already trusts, and reusing it means one idea rather than two.

**There are no per-release copies, because git already keeps history.** The release diff is `git diff <last-tag>..HEAD -- contracts/`. An earlier draft stored a directory per release; at this cadence that is thirty directories in six months, storing something version control already stores.

**Schema is emitted through `PRAGMA`, not by dumping `sqlite_master`.** SQLite keeps the original `CREATE TABLE` text verbatim, so a comment or a line break becomes a diff. `table_info`, `index_list` and `foreign_key_list` give structured, sortable, semantically comparable output — and 5.6 confirms it emits identically across a hundred runs.

**`generated/` is emitted with the `keel:generated` banner line stripped and the manifest excluded.** Both carry a wall-clock timestamp, so without that step every file changes every second and the gate is useless. This is not a new normalisation to design — `keel generate --check` already does exactly it, which 5.6 established by measurement rather than by reading the code.

The emitter is an integration test that writes when `UPDATE_CONTRACTS=1` and asserts otherwise — the shape `insta` already gives this project seventeen snapshots of. `snapshots__tools_list.snap` already pins the tool surface; what was missing was ever comparing it to a *released* one.

**The emitter refuses to overwrite anything under `stores/`.** `UPDATE_CONTRACTS=1` is the insta footgun — the thing someone runs to make a failing test go away — and the text files are safe to regenerate because git shows what changed. A vintage store is not regenerable and must only ever be created.

### 5.2 What counts as breaking

A table, not a judgement call, because a judgement call at release time is made by whoever is tired. **Anything the classifier cannot place is breaking.** Failing closed is the whole point; a classifier that guesses "additive" when unsure is worse than none.

| Surface | Additive | Breaking |
|---|---|---|
| **Store schema** | new table; new nullable column; new index | dropped or renamed table or column; narrowed type; `NOT NULL` without a default; changed primary or foreign key |
| **MCP tools** | new tool; new optional argument; new field in a response | removed or renamed tool; new required argument; removed response field; narrowed enum; changed argument type |
| **HTTP API** | new endpoint; new optional query parameter; new response field | removed endpoint; removed field; changed status code; changed envelope |
| **CLI** | new subcommand; new optional flag | removed or renamed subcommand or flag; changed default; changed exit code |
| **Generated markdown** | — | any layout change at all |
| **Backup bundle** | new file in the bundle | changed layout; changed manifest fields |

Two rows deserve their reasoning.

**Generated markdown has no additive column.** Users commit these files. A change to how a heading renders is a diff in every repository that has run `keel generate` — not breaking in any technical sense and exactly as annoying as one. It gets announced.

**A tool description change is not breaking and is not nothing.** The descriptions are the only documentation a model gets; they are the product. They break no caller, so they are additive — and the classifier prints them anyway, because a silent rewrite of the thing that decides tool selection deserves a human reading.

### 5.3 Vintage stores, and why the fixture is not enough

`contracts/stores/v<n>.sqlite` is one small store per **schema version** — four today, not one per release, because most releases do not touch the tables. The test opens every one with the current binary, runs `keel migrate`, runs `fsck`, and asserts row counts per table and content hashes per document survive.

That is the test standing between a migration and somebody's year of notes, and it is the only artifact here that cannot be reconstructed later: a store can only be written by the code that wrote it.

Size is settled and small. `keel fixture --home <dir> --force` produces a 700 KB store in about a second, so four schema versions of vintage corpus is under 3 MB. An earlier draft of this section said a fixture run "produced none" and asked whether the command was broken. It is not: `--home` is a subcommand flag rather than a global one, so `keel --home X fixture` does not parse as intended, and a second attempt was refused because a daemon was running — which is its own defect, filed as KEEL-194, since that daemon holds a different store entirely.

**Still seed the corpus from a scrubbed copy of the real store rather than from the fixture.** A synthetic corpus has the shapes we thought to generate; the real one is 7.2 MB and has archived rows, retracted notes, superseded decisions, blobs, a DuckDB-era history and whatever else five months produced. A migration that breaks on a null the fixture never leaves null passes green against the fixture and eats data in the field. The remaining thing to do before relying on any of this is to diff the fixture's table-shape coverage against the real store's, and treat every gap as a case the fixture cannot speak for.

Two more, both cheap:

- **The forward guard.** The current binary must refuse a store whose schema version is higher than its own, naming the upgrade. The behaviour exists; the assertion does not.
- **Round-trip across versions.** Back up with N, restore with N+1, diff. The within-version test exists; point it across two.

### 5.4 The release gate is about acknowledgement, not version numbers

An earlier draft refused a release unless "the version bump matches the highest severity found". **On 0.x that condition is satisfied by every release**, because additive and breaking both mean a minor bump. It would have passed forever while appearing to guard something.

What has teeth:

1. `contracts/` is clean after regeneration.
2. Every difference classified breaking appears in a checked-in entry for this release, **naming its migration and the sentence the user will see**. CI fails otherwise. This is the mechanism; the version number is decoration.
3. Every vintage store migrates green and passes `fsck`.
4. `keel generate keel --check` is clean.
5. Release notes carry a **Breaking** section generated from that entry rather than written from memory.

Point 2 is what point 5 is made of. Release notes assembled by hand from a week of commits are how a breaking change reaches users unannounced.

### 5.5 Deprecation, while on 0.x

Breaking changes are allowed. Unannounced ones are not.

- A removed tool, subcommand or flag keeps working as an alias for **two minor releases**, warning each time, then goes.
- A column is never dropped in the release that stops writing it. Stop writing, ship, drop next time, so a rollback in between still has its data.
- Every migration is reversible or takes a backup first. `/keel:setup` and the updater back up before migrating regardless.

### 5.6 Determinism — asked, and answered

Everything above rests on the emitter producing identical bytes for identical state. ULIDs, timestamps and map iteration order are what make snapshot emitters flap, and a flapping gate is disabled inside a month, after which the project believes it is guarded and is not. So this was run before anything was built on it, against the surfaces the emitter will wrap, on a fixed fixture store with its own daemon so that nothing observed was the live store moving underneath.

**Nine of ten surfaces emit one hash across 100 runs**: CLI help, the PRAGMA schema dump, `tools/list`, and `/api/health`, `/api/context` and `/api/activity`.

**The tenth, `keel generate`, produced 100 distinct hashes in 100 runs — and for exactly one reason.** Every `.keel/` file opens with a banner carrying the generation time, and `manifest.json` carries `generated_at`. Two generates in the same second are identical; two a second apart differ in all 67 files. Strip the banner line, drop the manifest, and it is **one hash across 100 runs spanning minutes**.

That normalisation is already the project's own: the committed banner in `.keel/decisions/chrono-for-time-not-jiff.md` reads `2026-08-10T18:53:24Z`, four days stale, and `keel generate --check` still counts that file among "83 current". So §5.1's emitter inherits a rule that exists rather than inventing one, and the risk this section was written to guard against is measured and closed rather than assumed away.

The harness is `scratchpad/determinism.sh`: `N` is an environment variable, a flapping surface leaves two differing runs on disk so the cause can be read instead of guessed, and a full pass at N=100 takes 60 seconds. It is worth keeping and pointing at the real emitter once that exists.

### 5.7 CI has never run

Everything above assumes a working pipeline and there is not one. `.github/workflows/ci.yml` has been in the tree since it was written and has never been executed by anything: `git remote -v` is empty, so there is no repository for Actions to run in. The trigger also named `main` while trunk was `master`, fixed in f65be15.

The Linux matrix entry has never tested anything on Linux, and every check in this project's history was run by hand on one Mac — including, for now, 5.6's. **Creating the remote is step zero**, before the contracts work rather than after, because a corpus generated on one machine has that machine's opinion baked into it.

---

## 6. Version drift, and updating without asking

The plugin updates over git through `claude plugin update`; the binaries update from a release. Two channels that will fall out of step — TQ-26 again, across a network and in someone else's install.

### 6.1 The release manifest is what makes the decision possible

Each release publishes a manifest beside its artifacts:

```json
{ "version": "0.6.1", "schema_version": 5,
  "min_plugin_version": "0.5.0", "artifacts": { "…": "sha256:…" } }
```

An earlier draft said to compare `shipped_schema_version()` on the candidate against the store's. **That cannot be done without executing the candidate**, which is the thing being decided. The schema version has to be readable before anything is downloaded, or "never automatic across a schema change" is a sentence with no mechanism behind it.

`/api/health` already reports `version`, `schema` and `protocol`. It gains `min_plugin_version` — and, for KEEL-194, the store it is actually serving.

### 6.2 Three outcomes

**Versions match.** Silence.

**Binary behind, manifest reports the same schema version.** Applied without asking. One line at the top of a session: `Keel updated to 0.6.1.` Nothing is confirmed and the next session says nothing.

**Manifest reports a different schema version, or the release entry carries a breaking change.** Nothing is applied. One line names what changes and points at `/keel:update`, which backs up, prints what will happen, and waits. The data is about to be rewritten in a way a rollback will not undo, and that decision has an owner.

### 6.3 Where it runs, and how it lands

Not in the hook. The hook runs before the user's first word and must never block a session — constraint 1 on the session-start hook, which does not bend for this.

The daemon checks the manifest daily, and on a compatible update downloads it, verifies checksum and GitHub build attestation, and stages it beside the current binary. **At its next startup the daemon finds the staged binary, renames it over itself and re-execs.** Renaming over a path is atomic and safe while the old image is still mapped; the restart comes free from the supervisor, since launchd `KeepAlive` and systemd `Restart=always` both restart a daemon that exits. Nothing ever swaps a binary that is running.

The hook only reports. `keel update` forces it now.

### 6.4 Rollback

The previous binary is kept as `keel.previous`, and `keel update --rollback` puts it back. This is the difference between one bad release being an inconvenience and being the end of somebody's trust.

**A migrated store cannot be un-migrated**, which is the reason schema changes never auto-apply. Rollback covers the case automatic updates create and does not pretend to cover the other one.

### 6.5 Disclosure

A daily check is a network request from a tool whose whole identity is local-first, and users will find it. So it is disclosed rather than discovered: `/keel:setup` asks once and takes no for an answer, `KEEL_AUTO_UPDATE=0` turns it off, and `keel doctor` reports whether it is on and when it last ran.

### 6.6 Why not fully automatic

A bad migration reaching every install before anyone can stop it is the one failure here with no undo. And a self-updating binary is a supply-chain target — one compromised release is remote execution everywhere it lands. Splitting on schema version keeps the automatic path where the worst outcome is a restart, and puts a human in front of the path where the worst outcome is their data.

---

## 7. The port, and the network

**The daemon does not wander.** It tries its configured port, default 7654, and:

- **Another `keel-daemon` on the same store** — the single-writer design working. Exit 0 saying so. Not an error.
- **Anything else** — fail, naming `--bind` and `KEEL_BIND`.

An earlier draft had it walk up the range to find a free port. That is the seductive wrong answer: `.mcp.json` expands its environment when Claude Code starts and plugin config is written at install time, so a daemon that quietly moves to 7655 leaves both stale and MCP fails with no explanation. **A wandering port and a static configuration file cannot both be right.**

Collisions are handled at the only moment configuration can still be written. `/keel:setup` probes, and on a collision picks a free port and writes it into the service unit *and* the plugin config together. `.mcp.json` becomes `"url": "${KEEL_DAEMON_URL}/mcp"` with 7654 as the declared default. The user never types a port and never edits a file inside an installed plugin, which the next update would overwrite.

`~/.keel/daemon.json` records the live URL for the CLI, which reads it first, then `$KEEL_DAEMON_URL`, then the default. **Presence of that file is not liveness** — a crash leaves it behind — so the CLI confirms with a health probe before trusting it.

**Binding.** `KEEL_BIND` accepts any address today, so one environment variable exposes an unauthenticated write API to the network. It starts refusing anything that is not loopback unless an explicit flag says otherwise, and the flag's help says what is being agreed to.

The attack that matters is already handled: the Origin and Host checks in `keel-daemon::http` stop a web page reaching the daemon by DNS rebinding, with a test for the rebinding case. The remaining gap is other local processes, which is why `keel ui` opens the site with a per-session token. A read-only unauthenticated API is defensible; it stops being defensible the moment an intake endpoint exists, which is section 3's prerequisite.

---

## 8. The hooks move into the binary — and one shim stays

`keel hook session-start` and `keel hook stop` replace the shell scripts' logic.

This is not tidying. The scripts need `python3` and `curl`, neither declared anywhere; `install.sh` warns about a missing `jq`, which neither script uses, so the one dependency check that exists checks the wrong thing. A clean macOS has no `python3` until the Xcode command line tools arrive, so on a fresh machine the hooks silently do nothing — which looks exactly like Keel not working.

The strongest reason is the fourth. **Nothing in the workspace or in CI executes either script.** KEEL-192 — the Stop hook nagging in projects Keel had never heard of — was found by reading and fixed by reading, and the fix is guarded by nothing. Section 5 is entirely about surfaces that describe themselves and are tested; the hooks are the one surface that does neither.

**The shim that has to stay.** Section 1 wants the hook to say "the binary is missing, run `/keel:setup`" — and a hook that *is* the binary cannot report its own absence. So `hooks/hooks.json` keeps calling a `${CLAUDE_PLUGIN_ROOT}` script, and that script does exactly one thing:

```sh
[ -x "$KEEL_BIN" ] && exec "$KEEL_BIN" hook session-start
printf '%s' '{"hookSpecificOutput":{…"Keel is installed but not set up — run /keel:setup."}}'
```

Ten lines that never change, holding no logic worth testing, with everything that can change on the other side of the `exec`. That is the honest resolution of the circularity rather than a claim it does not exist.

---

## 9. Embeddings

`fastembed` is fine on the compile question and awkward on the download one. The ONNX Runtime is fetched prebuilt and statically linked at our build time — `ort-sys` already pulls a 77 MB archive on every build — so turning embeddings on adds no build step. The model is the cost: `bge-small-en-v1.5` is 133 MB on first use, 66 MB quantized.

Cache path set explicitly under `~/.keel/models` rather than fastembed's default, which is relative to the working directory and is a bug waiting to be found in a long-running daemon. Opt-in behind a visible prompt in `/keel:setup`, never a silent pull. Keyword search works without any of it, which is what makes opt-in honest rather than a downgrade.

There is no prebuilt ONNX Runtime for Linux ARM. Not a target, so not a problem today.

---

## 10. The pipeline

`dist` at 0.32, released May 2026. Its sponsoring company wound down its commercial product and there was a six-month gap in 2025, during which Astral forked it for `uv`; those changes were merged back and releases have shipped every two to three months since. If it ever stalls, the workflow it generates is readable and can be vendored.

- Targets: `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-unknown-linux-gnu`.
- Shell installer, checksums embedded in it, GitHub artifact attestations on.
- macOS builds on macOS runners, for the ad-hoc signature.
- **Node in the release job**, for the embedded site. Not in the generated workflow by default.

**Windows is not a target.** It had no CI coverage and the hooks cannot run there, so shipping it would have meant either a binary nobody had run or a product that looks whole and is half-wired — this project's recurring failure shape. It comes back when someone asks, with test coverage from the first commit.

**One bug to patch on day one.** The installer `dist` generates verifies downloads with `sha256sum`, which stock macOS does not have — it has `shasum`. The generated script prints "skipping sha256 checksum verification" and returns success, so on our primary platform integrity checking silently does nothing. Fall back to `shasum -a 256` and send the fix upstream.

**One thing to diary.** GitHub's macOS Intel runners retire in August 2027. Before then `x86_64` macOS either moves to cross-compilation with an explicit signing step, or is dropped.

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

The plugin stays in this repository. Its skills describe behaviour the binary implements, so one commit changes both and there is nothing to keep in step — the TQ-26 drift class removed by construction rather than by discipline. The cost is cloning the Rust source to get four text files. Worth it.

Binaries go to GitHub Releases. `keel.sh` is a redirect that can be added any time and is not a prerequisite.

Apache-2.0 replaces the deliberately-absent licence, so `publish = false` and `deny.toml`'s `private.ignore` come out.

---

## 12. How a release is verified

Three tiers. **Tier 2 is the standing gate**; tier 3 runs before the first public release and before major ones after that.

### Tier 1 — the build machine, clean environment

Run the installer under `env -i HOME=/tmp/keel-clean PATH=/usr/bin:/bin` so there is no cargo on the path and no existing store.

Covers the installer end to end; the checksum refusal path, by corrupting an archive on purpose and confirming it refuses; a binary running with no toolchain; store creation; daemon start; MCP answering; the plugin installed into a virgin `~/.claude`; and the quarantine claim proved directly with `xattr -l` on a fresh download.

Blind to Linux entirely, to a Mac without the Xcode command line tools, and to the cross-compile signature trap. About fifteen minutes, scriptable, every release.

### Tier 2 — tier 1 plus a Linux VM in UTM, snapshot-restored, run by hand

Adds the Linux binary actually running, glibc compatibility, the shell installer on a non-Mac, the systemd unit installing and restarting after a kill, and "nothing compiles" on a machine that has genuinely never had Rust.

**This is the highest value per unit of effort in the whole phase**, because it covers the platform CI has never tested once. An hour of setup, then about ten minutes a release with a snapshot restore.

### Tier 3 — tier 2 plus a clean Mac

Adds a machine that has never had Rust or the Xcode command line tools, ad-hoc signature behaviour in the wild, and the multi-machine store story. A macOS VM in UTM on Apple Silicon may collapse most of this into tier 2's infrastructure and is worth trying before buying anything.

---

## 13. Exit criteria

- **Three commands and a restart, inside Claude Code, reach a working Keel with nothing compiling** — verified at tier 1 and tier 2.
- No artifact on the install path requires an Apple Developer ID.
- `jq` and `python3` are required by nothing.
- The installer refuses a corrupted archive — checked by corrupting one.
- A release is one CI run and produces every target.
- **`contracts/` emits one hash across 100 runs, and CI fails on an unclassified difference.** The surfaces it wraps already meet this, measured — see 5.6 — so what remains is for the emitter to inherit the banner normalisation rather than to rediscover the problem.
- Every vintage store migrates green and passes `fsck`, and at least one is seeded from a scrubbed copy of the real store.
- A release carrying a breaking change cannot merge without an entry naming its migration and its user-facing sentence.
- A compatible update applies and announces itself in one line; one crossing a schema version stops and asks; `keel update --rollback` puts the previous binary back.
- The daemon fails loudly on a taken port and refuses a non-loopback bind unless told.
- `que_01KZSQHK2C0CTKN36WJ9G4ZHQC` is answered before section 3 begins.

An earlier draft asked for verification "on a Mac that has never had Rust installed", which KB has no way to do and which would have joined the two phase gates that cannot be checked without a human. Tiers replace it with something meetable.

---

## Appendix — what this rests on

| Claim | Source |
|---|---|
| `curl` does not set `com.apple.quarantine` | quarantine is applied by the downloading app via its Info.plist; curl has none. Apple treats this as intended |
| Apple Silicon kills unsigned executables | an ad-hoc signature is required at exec; native macOS toolchains add one at link time, cross-compiles do not |
| A browser-downloaded `.dmg` needs notarization | $99/yr, and macOS Sequoia removed the Control-click bypass |
| Plugins may declare an HTTP-transport MCP server | the official marketplace ships `linear`, `github` and `vercel` as `"type": "http"`; `./.mcp.json` is the default path and needs no manifest entry |
| A plugin cannot ship the binaries | a plugin is a git clone of text; `keel` and `keel-daemon` are 37 MB each |
| SQLite keeps `CREATE TABLE` text verbatim | which is why schema is emitted through `PRAGMA` rather than by dumping `sqlite_master` |
| Nine of ten contract surfaces already emit deterministically | KEEL-193, N=100 against a fixed fixture store; `scratchpad/determinism.sh` |
| `keel generate` flaps only on two timestamps | the per-file `keel:generated` banner and `manifest.json`'s `generated_at`; normalised, 100 runs give 1 hash |
| `keel generate --check` already normalises the banner | a banner four days stale still counts among "83 current" |
| A fixture store is 700 KB | `keel fixture --home <dir> --force`, so four schema versions of vintage corpus is under 3 MB |
| `dist` installer checksum bug | its generated script calls `sha256sum`, which stock macOS does not have; it then skips verification and returns success |
| `dist` is maintained | 0.32.0, May 2026; Astral forked it during a 2025 gap and the changes were merged back upstream |
| ONNX Runtime does not compile | `ort` downloads a prebuilt static library at build time |
| fastembed model download | 133 MB on first use, 66 MB quantized; default cache is relative to the working directory |
| GitHub macOS Intel runners | retire August 2027 |
| Serving a UI from a local daemon is ordinary | Jupyter, Syncthing, Grafana, Meilisearch, Qdrant, code-server, pgAdmin |
| CI has never run | `git remote -v` is empty and no keel repository exists under the account; the workflow has never been executed by anything |
| The MCP surface already survives an engine swap | Phase 9 changed the storage engine and both insta suites produced zero diffs |
| The store ships four migrations, and the live store is at schema 4 | `schema::migrations()` lists ids 1–4; `keel doctor` reports "the store is at schema 4, which is what this binary ships" |

