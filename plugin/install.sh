#!/usr/bin/env bash
#
# Build Keel, install the binaries and the skill, and print the Claude Code
# configuration.
#
# Two kinds of file live under ~/.claude, and this script treats them
# differently on purpose.
#
#   settings.json is *yours*. This script never edits it. Rewriting someone's
#   settings from a shell script is the kind of helpfulness that is
#   indistinguishable from damage the one time it gets it wrong, so this prints
#   what to add and lets you paste it.
#
#   ~/.claude/skills/keel/ is *Keel's*. Its contents are this repository's
#   files and nothing else authors them, so copying them there is installation
#   rather than interference.
#
# That distinction is why TQ-26 exists. The skill and hooks were hand-copied
# once and then drifted: the repository was edited, the copies were not, and
# nothing anywhere said so — a plugin change simply landed inert. The
# hand-copies were already stale again within one session of being made.
#
# `--skill-only` skips the build and installs just the skill and hooks, which
# is what you want after editing them. A full build to copy three files is the
# friction that made people skip the copy in the first place.

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="${KEEL_BIN_DIR:-$HOME/.local/bin}"
keel_home="${KEEL_HOME:-$HOME/.keel}"
skill_dir="${KEEL_SKILL_DIR:-$HOME/.claude/skills/keel}"
# Adoption is its own skill because it is used once per project and the everyday
# one is loaded in every project conversation. Folding eighty lines about
# backfilling a repository into that would tax every session for a workflow
# nobody runs twice.
adopt_dir="${KEEL_ADOPT_SKILL_DIR:-$HOME/.claude/skills/keel-adopt}"

skill_only=false
case "${1:-}" in
  --skill-only) skill_only=true ;;
  "") ;;
  *) echo "usage: $0 [--skill-only]" >&2; exit 2 ;;
esac

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }
note() { printf '  %s\n' "$*"; }

# Copy one file only if it differs, and say which of the three things happened.
# "unchanged" is worth printing: it is the evidence that the copy is in step,
# which is the single fact this whole step exists to establish.
install_file() {
  local src="$1" dest="$2" mode="$3"
  local name; name="$(basename "$src")"
  if [ -f "$dest" ] && cmp -s "$src" "$dest"; then
    note "$name — unchanged"
    return
  fi
  local verb="installed"
  [ -f "$dest" ] && verb="updated"
  install -m "$mode" "$src" "$dest"
  note "$name — $verb"
}

install_skill() {
  say "Installing the skill and hooks to $skill_dir"
  mkdir -p "$skill_dir"
  install_file "$repo_root/plugin/skills/keel/SKILL.md" "$skill_dir/SKILL.md" 644
  mkdir -p "$adopt_dir"
  install_file "$repo_root/plugin/skills/keel-adopt/SKILL.md" "$adopt_dir/SKILL.md" 644
  # One shim now, not two scripts. KEEL-206 moved the logic into the binary as
  # `keel hook session-start` and `keel hook stop`; what is left here is the
  # only part that has to run *without* the binary, so a session between
  # installing the plugin and running setup can say the binary is missing.
  install_file "$repo_root/plugin/hooks/keel-hook.sh" "$skill_dir/keel-hook.sh" 755

  # The two scripts this replaced become forwarders rather than disappearing.
  #
  # This is not tidiness, it is the upgrade path. `settings.json` is the user's
  # file and this script will not edit it — which is right, and it means an
  # existing install still names `session-start.sh` and `stop.sh` by absolute
  # path. Deleting them outright breaks every hook on the machine at the moment
  # of upgrade, silently, until somebody edits a file they were never told to
  # edit. That was tried here and it did exactly that.
  #
  # Three lines each, forwarding to the shim. Nothing needs to change for an
  # upgrade to work, and a settings.json that is simplified later keeps working
  # too.
  for pair in "session-start.sh session-start" "stop.sh stop"; do
    stale="${pair%% *}"
    event="${pair##* }"
    cat > "$skill_dir/$stale" <<FORWARD
#!/bin/sh
# Compatibility forwarder. The hooks moved into the binary in KEEL-206; this
# exists so a settings.json written before that keeps working unchanged.
# Nothing needs it once settings.json points at keel-hook.sh directly.
exec "\$(dirname "\$0")/keel-hook.sh" $event
FORWARD
    chmod 755 "$skill_dir/$stale"
    note "$stale — forwards to keel-hook.sh"
  done

  # Read-only inspection, not a rewrite. A settings file that does not mention
  # these paths means the hooks are installed and never run, which looks
  # exactly like the hooks not working.
  local settings="$HOME/.claude/settings.json"
  if [ -f "$settings" ] && ! grep -q "$skill_dir/keel-hook.sh" "$settings" 2>/dev/null; then
    note ""
    note "NOTE: $settings does not reference these hooks, so they will not run."
    note "See the settings snippet printed at the end."
  fi
}

if [ "$skill_only" = true ]; then
  install_skill
  say "Done"
  note "Skipped the build and the binaries — drop --skill-only for those."
  exit 0
fi

say "Building Keel"
note "SQLite is compiled in, so the installed binary is self-contained: there"
note "is no database to install alongside it and nothing to keep in step."
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

install_skill

# The dependency check that used to be here warned about `jq` and `curl` and
# named the session hooks as the reason. It was wrong twice over: the hooks
# never used `jq`, and the thing they *did* need — `python3`, absent on a Mac
# until the Xcode command line tools arrive — was never checked at all. So the
# one dependency check in the installer was checking the wrong tools for the
# wrong component.
#
# KEEL-206 removed the need rather than correcting the warning. The hooks are
# `keel hook session-start` and `keel hook stop` now, and the only shell left is
# `keel-hook.sh`, which is POSIX `sh` and shells out to nothing. There is
# nothing here to warn about, so there is no warning.

say "Next"
cat <<EOF
  1. Start the daemon, and leave it running:

       keel-daemon

     Add --embeddings for semantic search. The first run downloads the model;
     keyword search works either way.

  2. Wire up the hooks. The files are installed; nothing runs them until
     $HOME/.claude/settings.json says so. Add this yourself — the one thing
     this script will not touch:

       {
         "hooks": {
           "SessionStart": [
             { "hooks": [ { "type": "command", "timeout": 10,
                 "command": "$skill_dir/keel-hook.sh session-start" } ] }
           ],
           "Stop": [
             { "hooks": [ { "type": "command", "timeout": 15,
                 "command": "$skill_dir/keel-hook.sh stop" } ] }
           ]
         }
       }

  3. Or register the repository as a Claude Code plugin instead, which brings
     its own hooks.json and needs no settings edit:

       $repo_root/plugin

  4. Or wire up the MCP server alone, without the skill or hooks:

       claude mcp add --transport http keel http://127.0.0.1:7654/mcp

  5. Check it:

       curl -s http://127.0.0.1:7654/api/health | jq

  6. Load the sample corpus into a scratch store to see what it looks like:

       keel --home /tmp/keel-demo fixture
       keel --home /tmp/keel-demo render-status keel

  After editing anything under plugin/, re-run:

       ./plugin/install.sh --skill-only

EOF

say "Phase 2's gate"
cat <<'EOF'
  Met and frozen. ">=9 of 10 unprompted sessions write to Keel" closed at 18 of
  20 across two independent draws, and nobody is running it any more. The
  harness is kept and still tested, because the next time the agent's
  orientation changes it is the only way to find out what that did.

  product/GATE.md is the whole story, including the five evenings spent fixing
  a problem that turned out not to exist.
EOF
