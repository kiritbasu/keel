#!/usr/bin/env bash
#
# Build Keel, install the binaries, and print the Claude Code configuration.
#
# Deliberately does not edit any Claude configuration file itself. Rewriting
# someone's settings from a shell script is the kind of helpfulness that is
# indistinguishable from damage the one time it gets it wrong — so this prints
# what to add and lets you paste it.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${KEEL_BIN_DIR:-$HOME/.local/bin}"
keel_home="${KEEL_HOME:-$HOME/.keel}"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }

say "Building Keel"
note "The first build compiles DuckDB from source and takes a few minutes."
note "That keeps the installed binary self-contained. For fast *development*"
note "builds, see README 'Faster builds' — it links a system libduckdb instead."
cd "$repo_root"
cargo build --release --workspace

say "Installing binaries to $bin_dir"
mkdir -p "$bin_dir"
install -m 755 target/release/keel "$bin_dir/keel"
install -m 755 target/release/keel-daemon "$bin_dir/keel-daemon"
note "keel"
note "keel-daemon"

if ! command -v keel >/dev/null 2>&1; then
  note ""
  note "WARNING: $bin_dir is not on your PATH. Add it:"
  note "  export PATH=\"$bin_dir:\$PATH\""
fi

say "Creating the store at $keel_home"
"$bin_dir/keel" --home "$keel_home" status >/dev/null
note "done"

# ~/.keel is its own git repo, which is recovery tier 1 (SPEC §11): full
# fidelity, including revision history. No remote — that is KB's call (Q-2).
if [ ! -d "$keel_home/.git" ]; then
  say "Initialising $keel_home as a git repository"
  git -C "$keel_home" init -q
  cat > "$keel_home/.gitignore" <<'GITIGNORE'
# Model weights are large and re-downloadable.
models/
GITIGNORE
  git -C "$keel_home" add -A
  git -C "$keel_home" commit -q -m "chore: initialise the Keel store" || true
  note "done — no remote configured, which is deliberate (QUESTIONS Q-2)"
fi

for tool in jq curl; do
  command -v "$tool" >/dev/null 2>&1 || note "WARNING: \`$tool\` is missing; the mirror hook needs it."
done

say "Next"
cat <<EOF
  1. Start the daemon, and leave it running:

       keel-daemon

     Add --embeddings for semantic search. The first run downloads the model;
     keyword search works either way.

  2. Add the plugin to Claude Code. In your settings, register this directory
     as a plugin:

       $repo_root/plugin

  3. Or, to wire up the MCP server alone without the skill:

       claude mcp add --transport http keel http://127.0.0.1:7654/mcp

  4. Check it:

       curl -s http://127.0.0.1:7654/api/health | jq

  5. Load the sample corpus into a scratch store to see what it looks like:

       keel --home /tmp/keel-demo fixture
       keel --home /tmp/keel-demo render-status keel

EOF

say "Phase 2's gate"
cat <<'EOF'
  The exit criterion is ">=9 of 10 unprompted sessions write to Keel". It cannot
  be automated: "unprompted" is the whole claim, and a test that calls the tool
  has prompted it. plugin/README.md has the protocol for running it by hand.
EOF
