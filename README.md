# Keel

A local-first store for everything that describes a software project other than
the code — specs, decisions, tasks, roadmap, design, feedback. An MCP server is
the primary interface; a Tauri desktop app is the read surface.

**All product documentation lives in `product/`.** Start with `product/STATUS.md`.

---

## Where things stand

Phases 0–3 are built: the storage spine, the MCP daemon, the Claude Code
plugin, and the desktop app. 264 tests, nothing ignored, all four CI gates
green.

Two exit criteria are **not** met, both because they need a human:

- **Phase 2's gate** — "≥9 of 10 *unprompted* sessions write to Keel". This is
  the one the PRD calls the real test of the premise, and it is unrun.
  `plugin/README.md` has the protocol.
- **Phase 1's UC-1→UC-4 gate** — passes mechanically (21 tests drive a real
  daemon over real HTTP), but a scripted client is told which tool to call.
  Whether the tool descriptions lead a *model* to the right tool is untested.

`product/STATUS.md` has a table making both explicit.

---

## Try it

```bash
./plugin/install.sh          # build, install binaries, create ~/.keel
keel-daemon                  # leave running; binds 127.0.0.1:7654
```

Then, in another terminal:

```bash
keel --home /tmp/keel-demo fixture          # 212-entity sample corpus
keel --home /tmp/keel-demo render-status keel
keel --home /tmp/keel-demo fsck
```

The desktop app:

```bash
cd apps/desktop && npm install && npm run dev    # with the daemon running
```

---

## Layout

```
crates/keel-core/     domain types, storage, graph, search, mirror, backup
crates/keel-mcp/      the nine tools, the digest, protocol handling
crates/keel-daemon/   axum: MCP endpoint + local REST/SSE. Owns the write handle
crates/keel-cli/      backup, restore, fsck, fixture, mirror, render-status
crates/keel-github/   Phase 4 stub
apps/desktop/         Tauri v2 + React. Read and search only
plugin/               the skill, the mirror hook, MCP config
product/              PRD, SPEC, STATUS, DECISIONS, QUESTIONS
```

---

## The three things most likely to bite

1. **Graph direction.** An inverted traversal returns an empty set that looks
   exactly like "nothing is linked here". `product/SPEC.md` §3.3 is the only
   authority; `crates/keel-core/tests/graph_direction.rs` asserts both
   directions *and* both inversions for all nine relations.
2. **Silent truncation.** Every list reports what it cut. Open questions and
   glossary terms are never cut at all — a truncated task list makes an agent
   less informed, but a truncated question register makes it confidently wrong.
3. **The mirror is one-directional.** `crates/keel-core/src/mirror.rs` has no
   function that reads a mirror as truth, and a test asserts that absence.

---

## Development

```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
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

All 264 tests pass either way, Lance extension included. Two caveats: the
version has to match what `duckdb-rs` ships bindings for, and a later
`brew upgrade duckdb` will break a binary linked this way until you rebuild —
which is exactly why `plugin/install.sh` still bundles.
