# Keel

A local-first store for everything that describes a software project other than
the code — specs, decisions, tasks, roadmap, design, feedback. An MCP server is
the primary interface; a Tauri desktop app is the read surface.

**All product documentation lives in `product/`.** Start with `product/STATUS.md`.

---

## Where things stand

Phases 0–3 are built — the storage spine, the MCP daemon, the Claude Code
plugin, and the desktop app — and then rebuilt in places, because the first
version of the app was seven screens built to seven different rules. **408 Rust
tests and 146 desktop tests, nothing ignored, all CI gates green.**

**Phase 2's gate is met and frozen.** The question it asked — will a coding
agent write to Keel without being told to? — took seven runs to answer, and most
of that time was spent measuring the harness rather than the agent. It closed at
18 of 20 across two consecutive independent draws. The code is kept and nobody
is running it. `product/GATE.md` is the whole story in one page, including the
part where five evenings went into fixing a problem that turned out not to
exist.

**Phase 1's UC-1→UC-4 gate** passes mechanically — 21 tests drive a real daemon
over real HTTP — but a scripted client is told which tool to call. The gate runs
partly answer the harder question, since those sessions chose tools with no
instruction and chose them correctly; what has never been tested in isolation is
whether the *descriptions alone* are what did it.

---

## Try it

```bash
./plugin/install.sh          # build, install binaries, create ~/.keel
keel-daemon                  # leave running; binds 127.0.0.1:7654
```

Then, in another terminal:

```bash
keel --home /tmp/keel-demo fixture
```

```bash
keel --home /tmp/keel-demo render-status keel
```

```bash
keel --home /tmp/keel-demo fsck
```

The desktop app, with the daemon running:

```bash
cd apps/desktop && npm install && npm run dev
```

Install the pre-commit check once, if you are going to edit anything here:

```bash
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit
```

---

## Layout

```
crates/keel-core/     domain types, storage, graph, search, mirror, backup
crates/keel-mcp/      the ten tools, the digest, protocol handling
crates/keel-daemon/   axum: MCP endpoint + local REST/SSE. Owns the write handle
crates/keel-cli/      backup, restore, fsck, fixture, generate, render-status
crates/keel-github/   Phase 4 stub
apps/desktop/         Tauri v2 + React. Read and search only, by decision
plugin/               the skill, the session hooks, MCP config
product/              PRD, SPEC, STATUS, DECISIONS, GATE, JOURNAL — all generated
scripts/              the gate harness (frozen), the pre-commit check
```

Everything under `product/` and `.keel/` is an **output**. The store is the
source of truth and the files are written from it; an edit to one is overwritten
by the next `keel generate`. The one exception is the repository root's
`CLAUDE.md`, which bootstraps the rule and therefore cannot depend on it.

---

## The four things most likely to bite

1. **Graph direction.** An inverted traversal returns an empty set that looks
   exactly like "nothing is linked here". `product/SPEC.md` §3.3 is the only
   authority; `crates/keel-core/tests/graph_direction.rs` asserts both
   directions *and* both inversions for all nine relations.
2. **Silent truncation.** Every list reports what it cut. Questions and glossary
   terms are never cut at all — a truncated task list makes an agent less
   informed, but a truncated question register makes it confidently wrong.
3. **The mirror is one-directional, with no exceptions.**
   `crates/keel-core/src/mirror.rs` contains no function that reads a mirror as
   truth, and a test asserts that absence. There was once a hook that claimed to
   read one edit back safely; it never worked, and deleting it was the fix. See
   SPEC §8.1.
4. **`blocked` is not a status.** It is derived from the `blocks` edges. A
   status field and a graph that can disagree will disagree — this store had
   three tasks marked blocked and zero edges.

---

## Development

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo fmt --all --check
```

```bash
cargo deny check
```

Dev builds use `debug = "line-tables-only"` — full debug info for a vendored
C++ database runs to nineteen gigabytes, which is how that setting was
discovered.

### Faster builds

By default DuckDB is compiled from source, which takes several minutes the
first time. That default exists so the build works on a fresh machine with no
setup, and so the installed binary keeps working when Homebrew moves underneath
it.

If you already have a matching DuckDB, link it instead and the whole workspace
builds in under a minute:

```bash
brew install duckdb   # must match the version duckdb-rs targets — currently 1.5.5
```

```bash
export DUCKDB_LIB_DIR=/opt/homebrew/opt/duckdb/lib DUCKDB_INCLUDE_DIR=/opt/homebrew/opt/duckdb/include
```

```bash
cargo test --workspace --no-default-features
```

The suite passes either way, Lance extension included. Two caveats: the version
has to match what `duckdb-rs` ships bindings for, and a later
`brew upgrade duckdb` will break a binary linked this way until you rebuild —
which is exactly why `plugin/install.sh` still bundles.
