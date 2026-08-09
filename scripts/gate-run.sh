#!/usr/bin/env bash
#
# Phase 2's gate: ten unprompted sessions.
#
#   ≥9 of 10 sessions write to Keel · every write carries a session_id ·
#   0 duplicate projects
#
# Run this from a terminal where you are logged in to Claude Code. It cannot be
# run from inside a Claude Code session: `claude -p` in a non-interactive shell
# reports "Not logged in", because the credential lives somewhere a spawned
# shell cannot reach.
#
# ---------------------------------------------------------------------------
# Why the sessions run in scratch projects and not in the Keel repo
#
# `keel/CLAUDE.md` is four hundred lines telling Claude what Keel is and to
# keep it updated. A session started in this repository is about as prompted as
# a session can be, and passing the gate there would prove nothing. So the
# sessions run against two throwaway projects — a tide predictor and a blob
# store — that mention Keel nowhere.
#
# The prompts are ordinary developer talk: a bug, a refactor, a decision, a
# customer complaint. Several are things SKILL.md claims to trigger on ("we
# should", "let's go with", "what's blocking", "I spoke to a customer"), which
# is fair — they are what people actually say. What none of them do is mention
# Keel, or ask for anything to be recorded.
# ---------------------------------------------------------------------------

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
work="${GATE_DIR:-${TMPDIR:-/tmp}/keel-gate}"
keel="${KEEL_BIN:-$root/target/release/keel}"

tools="Read,Grep,Glob,Edit,Write,Skill"
tools="$tools,mcp__keel__keel_context,mcp__keel__keel_create,mcp__keel__keel_update"
tools="$tools,mcp__keel__keel_search,mcp__keel__keel_get,mcp__keel__keel_link"
tools="$tools,mcp__keel__keel_projects,mcp__keel__keel_activity,mcp__keel__keel_write_doc"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

# --- preconditions ---------------------------------------------------------
curl -sf http://127.0.0.1:7654/api/health >/dev/null || {
  echo "The daemon is not answering on 127.0.0.1:7654. Start it with: keel-daemon"
  exit 1
}
[ -f "$HOME/.claude/skills/keel/SKILL.md" ] || {
  echo "The skill is not installed. It is the thing being measured:"
  echo "  cp -r $root/plugin/skills/keel ~/.claude/skills/keel"
  exit 1
}
claude mcp list 2>/dev/null | grep -q '^keel:.*Connected' || {
  echo "Claude cannot reach the Keel MCP server. Register it for every project:"
  echo "  claude mcp add --scope user --transport http keel http://127.0.0.1:7654/mcp"
  exit 1
}

# --- the two scratch projects ----------------------------------------------
say "Setting up scratch projects in $work"
rm -rf "$work"
mkdir -p "$work/tideline/src" "$work/pellet/src"

cat > "$work/tideline/README.md" <<'EOF'
# Tideline

Tide prediction for small harbours. Reads a station's harmonic constituents and
produces a 7-day tide table.
EOF

cat > "$work/tideline/src/harmonics.py" <<'EOF'
"""Harmonic tide prediction."""
import math

# Amplitude in metres, phase in degrees, speed in degrees/hour.
CONSTITUENTS = {
    "M2": (1.42, 121.0, 28.984104),
    "S2": (0.48, 158.0, 30.0),
    "N2": (0.29, 100.0, 28.439730),
    "K1": (0.11, 20.0, 15.041069),
}


def height(hours_since_epoch, datum=3.1):
    """Predicted height in metres above chart datum."""
    total = datum
    for amplitude, phase, speed in CONSTITUENTS.values():
        total += amplitude * math.cos(math.radians(speed * hours_since_epoch - phase))
    return total


def high_waters(start, hours=168, step=0.25):
    """Times of high water over the window, in hours since epoch."""
    peaks = []
    previous = height(start)
    rising = True
    t = start + step
    while t < start + hours:
        current = height(t)
        if rising and current < previous:
            peaks.append(t)
            rising = False
        elif not rising and current > previous:
            rising = True
        previous = current
        t += step
    return peaks
EOF

cat > "$work/tideline/src/table.py" <<'EOF'
"""Render a tide table."""
from harmonics import height, high_waters


def render(start, hours=168):
    rows = []
    for t in high_waters(start, hours):
        rows.append(f"{t:8.2f}h  {height(t):5.2f}m")
    return "\n".join(rows)
EOF

cat > "$work/pellet/README.md" <<'EOF'
# Pellet

A tiny content-addressed blob store. Writes go to `.pellet/<sha256>`; reads are
by digest. Used by the build cache.
EOF

cat > "$work/pellet/src/store.js" <<'EOF'
const crypto = require("crypto");
const fs = require("fs");
const path = require("path");

const ROOT = ".pellet";

function digest(buffer) {
  return crypto.createHash("sha256").update(buffer).digest("hex");
}

function put(buffer) {
  const key = digest(buffer);
  fs.mkdirSync(ROOT, { recursive: true });
  fs.writeFileSync(path.join(ROOT, key), buffer);
  return key;
}

function get(key) {
  return fs.readFileSync(path.join(ROOT, key));
}

function gc(keep) {
  for (const entry of fs.readdirSync(ROOT)) {
    if (!keep.includes(entry)) {
      fs.unlinkSync(path.join(ROOT, entry));
    }
  }
}

module.exports = { put, get, gc, digest };
EOF

# --- the ten sessions ------------------------------------------------------
run() {
  local project="$1" prompt="$2" n="$3"
  printf '\n\033[1m[%2d/10] %s\033[0m\n  %s\n' "$n" "$project" "$prompt"
  ( cd "$work/$project" && claude -p "$prompt" --allowedTools "$tools" ) \
    > "$work/session-$n.log" 2>&1
  printf '  → %s\n' "$work/session-$n.log"
}

t0="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "$t0" > "$work/t0"
say "Baseline $t0 — anything written after this counts"

run tideline "high_waters misses the first peak if the window starts right on one — have a look at src/harmonics.py" 1
run tideline "we should cache the constituent lookup, it gets recomputed on every height() call" 2
run tideline "let's go with 15-minute resolution as the default for the tide table rather than the current step" 3
run tideline "what's the risk if a station's chart datum is wrong? walk me through what breaks" 4
run tideline "a harbourmaster rang to say the 7-day table is unreadable on a phone — the times need to be local, not hours since epoch" 5
run tideline "before I forget: constituent phases should be validated to 0-360, nothing checks that today" 6
run pellet "gc() in src/store.js deletes anything not in keep. if keep comes back empty by accident that wipes the store — fix it" 7
run pellet "we picked sha256 early on but blake3 is a lot faster. worth switching?" 8
run pellet "what's stopping put() from being atomic? a crash mid-write leaves a truncated blob under a valid-looking digest" 9
run pellet "I want a size cap on the store with LRU eviction. roughly what's involved?" 10

# --- score -----------------------------------------------------------------
say "Scoring"
"$keel" gate --since "$t0"
status=$?

say "Session transcripts"
echo "  $work/session-*.log"
echo
echo "  If a session did not write, read its log before changing anything."
echo "  plugin/README.md maps each failure mode to the part of SKILL.md at fault."
exit $status
