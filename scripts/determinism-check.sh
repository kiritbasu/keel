#!/usr/bin/env bash
#
# Phase 10 §5.6 — is emission deterministic?
#
# The emitter does not exist yet, so this runs against the surfaces it will
# wrap. Each is emitted N times against fixed state; a surface is deterministic
# if every run hashes the same.
#
# Fixed state matters more than N. A surface emitted against the live store
# would differ between runs because the store changed, which says nothing about
# the emitter. Everything here reads the fixture store, which nothing writes to
# for the duration.

set -uo pipefail

KEEL="${KEEL:-./target/release/keel}"
# Derived, not written down. The `cwd` probe below needs a real path that
# `specline_context` will match against a project's `root_path`, and hardcoding one
# put this machine's username into a file that is going public.
REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
STORE=/tmp/keel-det/store
WORK=/tmp/keel-det
N="${N:-100}"
DAEMON="${SPECLINE_DAEMON_URL:-http://127.0.0.1:7654}"

mkdir -p "$WORK/runs"
pass=0; fail=0

# Emit N times, hash each, report how many distinct results came back.
# Distinct > 1 is the whole finding: it means the same state produced different
# bytes, which is what would make a CI gate flap.
probe() {
  local name="$1"; shift
  local hashes=() h
  for _ in $(seq 1 "$N"); do
    h="$("$@" 2>/dev/null | shasum -a 256 | cut -d' ' -f1)"
    hashes+=("$h")
  done
  local distinct
  distinct="$(printf '%s\n' "${hashes[@]}" | sort -u | wc -l | tr -d ' ')"
  if [ "$distinct" = "1" ]; then
    printf '  \033[32mstable\033[0m    %-28s %s runs, 1 hash\n' "$name" "$N"
    pass=$((pass+1))
  else
    printf '  \033[31mFLAPS\033[0m     %-28s %s runs, %s distinct hashes\n' "$name" "$N" "$distinct"
    fail=$((fail+1))
    # Keep two differing runs so the cause can be read rather than guessed.
    "$@" > "$WORK/runs/$name.a" 2>/dev/null
    "$@" > "$WORK/runs/$name.b" 2>/dev/null
  fi
}

# --- schema, the way §5.1 says to emit it -----------------------------------
# PRAGMA rather than sqlite_master, because SQLite stores the original CREATE
# TABLE text verbatim and a comment change would read as a schema change.
schema_dump() {
  python3 - "$STORE/keel.sqlite" <<'PY'
import json, sqlite3, sys
c = sqlite3.connect(sys.argv[1])
out = {}
tables = sorted(r[0] for r in c.execute(
    "SELECT name FROM sqlite_master WHERE type='table'"))
for t in tables:
    out[t] = {
        "columns": [list(r) for r in c.execute(f'PRAGMA table_info("{t}")')],
        "indexes": [list(r) for r in c.execute(f'PRAGMA index_list("{t}")')],
        "foreign_keys": [list(r) for r in c.execute(f'PRAGMA foreign_key_list("{t}")')],
    }
print(json.dumps(out, indent=2, sort_keys=True))
PY
}

# --- the MCP tool surface ---------------------------------------------------
tools_list() {
  curl -sf --max-time 10 "$DAEMON/mcp" \
    -H 'content-type: application/json' \
    -H 'accept: application/json, text/event-stream' \
    -H 'mcp-protocol-version: 2026-07-28' \
    -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
}

http_get() { curl -sf --max-time 10 "$DAEMON$1"; }

# --- generate, into a scratch tree so the repo is never touched -------------
generate_tree() {
  local out="$WORK/gen"
  rm -rf "$out"; mkdir -p "$out"
  "$KEEL" generate specline --home "$STORE" --repo "$out" >/dev/null 2>&1
  # Hash the tree's content, not its mtimes: every file's path and bytes.
  find "$out" -type f | sort | while read -r f; do
    printf '%s ' "${f#"$out"/}"; shasum -a 256 "$f" | cut -d' ' -f1
  done
}

# The same tree, normalised the way `specline generate --check` already normalises
# it: the `specline:generated` banner line dropped, and the manifest excluded.
#
# Both carry a wall-clock timestamp and nothing else that moves, so this
# separates "the emitter is non-deterministic" from "two fields record when the
# emitter ran". Only the first would be a problem, and it is the one this whole
# probe exists to find.
generate_tree_normalised() {
  local out="$WORK/gn"
  rm -rf "$out"; mkdir -p "$out"
  "$KEEL" generate specline --home "$STORE" --repo "$out" >/dev/null 2>&1
  find "$out" -type f ! -name manifest.json | sort | while read -r f; do
    printf '%s ' "${f#"$out"/}"
    grep -v 'specline:generated' "$f" | shasum -a 256 | cut -d' ' -f1
  done
}

echo
echo "Phase 10 §5.6 — determinism of the contract surfaces"
echo "N=$N runs each, against the fixture store at $STORE"
echo

echo "CLI"
probe "cli-help"            "$KEEL" --help
probe "cli-help-generate"   "$KEEL" generate --help
probe "cli-help-close"      "$KEEL" close --help

echo "Store"
probe "schema-pragma"       schema_dump

echo "MCP"
probe "tools-list"          tools_list

echo "HTTP"
probe "api-health"          http_get /api/health
probe "api-context"         http_get "/api/context?cwd=$REPO_ROOT&depth=brief"
probe "api-activity"        http_get "/api/activity?limit=20"

echo "Generated markdown"
probe "generate-raw"        generate_tree
probe "generate-normalised" generate_tree_normalised

echo
echo "  $pass stable, $fail flapping"
[ "$fail" = "0" ] || echo "  differing runs kept under $WORK/runs/ for reading"
echo
