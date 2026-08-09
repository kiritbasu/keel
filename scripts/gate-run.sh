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
# Overridable so the gate can run against a scratch store rather than the real
# one — a cold start is part of what is being measured, and a store that
# already knows the projects is not one.
daemon_url="${KEEL_DAEMON_URL:-http://127.0.0.1:7654}"

tools="Read,Grep,Glob,Edit,Write,Skill"
tools="$tools,mcp__keel__keel_context,mcp__keel__keel_create,mcp__keel__keel_update"
tools="$tools,mcp__keel__keel_search,mcp__keel__keel_get,mcp__keel__keel_link"
tools="$tools,mcp__keel__keel_projects,mcp__keel__keel_activity,mcp__keel__keel_write_doc"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

# Only the Keel MCP server, for two reasons.
#
# Speed: `claude -p` starts every configured server before it does anything.
# On this machine that means `npx firebase-tools` and a Vercel server that
# needs authentication — a minute of startup per session at best, and a wedged
# session at worst. Ten sessions pay it ten times. The first attempt at this
# script appeared to hang; it was waiting on firebase-tools.
#
# Validity: the gate asks whether Claude reaches for Keel. Tools for unrelated
# services are noise in that measurement, and one of them failing to start is
# noise that looks like a Keel failure.
mcp_config="$work/mcp.json"
write_mcp_config() {
  mkdir -p "$work"
  cat > "$mcp_config" <<JSON
{
  "mcpServers": {
    "keel": { "type": "http", "url": "$daemon_url/mcp" }
  }
}
JSON
}
write_mcp_config


# --- whose API are we actually talking to? ---------------------------------
#
# This machine's ~/.zshrc routes Claude Code through OpenRouter:
#
#   ANTHROPIC_BASE_URL=https://openrouter.ai/...
#   ANTHROPIC_AUTH_TOKEN=$OPENROUTER_API_KEY
#   ANTHROPIC_API_KEY=""
#
# Three consequences, all of which cost an evening:
#
#  - Requests go to OpenRouter, which for this shape of call did not answer at
#    all. The process sat in the foreground using no CPU and looked hung.
#  - Unsetting only ANTHROPIC_BASE_URL sends the OpenRouter *token* to
#    api.anthropic.com, which is exactly "401 Invalid bearer token".
#  - Even when it works, the sessions run on whatever OpenRouter routes to.
#    The gate would then measure some other model's response to SKILL.md,
#    which is not the claim being tested.
#
# So: all three variables have to go together, or none of them.
if [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ] ||
   { [ -n "${ANTHROPIC_BASE_URL:-}" ] && [ "${ANTHROPIC_BASE_URL}" != "https://api.anthropic.com" ]; }; then
  echo "This shell points Claude Code at a third-party endpoint."
  echo "  ANTHROPIC_BASE_URL   = ${ANTHROPIC_BASE_URL:-<unset>}"
  if [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ]; then
    echo "  ANTHROPIC_AUTH_TOKEN = <set, ${#ANTHROPIC_AUTH_TOKEN} chars — not printed>"
  else
    echo "  ANTHROPIC_AUTH_TOKEN = <unset>"
  fi
  echo
  echo "The gate must run against Claude, or it measures a different model's"
  echo "response to SKILL.md. Re-run with all three unset together:"
  echo
  echo "    env -u ANTHROPIC_BASE_URL -u ANTHROPIC_AUTH_TOKEN -u ANTHROPIC_API_KEY \\"
  echo "        ./scripts/gate-run.sh"
  echo
  echo "If that reports a login problem, log in the same way first:"
  echo "    env -u ANTHROPIC_BASE_URL -u ANTHROPIC_AUTH_TOKEN -u ANTHROPIC_API_KEY claude"
  echo "  then /login"
  exit 1
fi

# --- preconditions ---------------------------------------------------------
curl -sf "$daemon_url/api/health" >/dev/null || {
  echo "The daemon is not answering on $daemon_url. Start it with: keel-daemon"
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

# One throwaway session before spending ten. The first run of this script
# produced ten identical 55-byte authentication errors over twenty-one minutes,
# because nothing checked that a session could start at all. A gate that cannot
# distinguish "Claude declined to write" from "Claude never ran" is worse than
# no gate: both look like a row of empty logs.
# Output goes through tee, never into a variable. Capturing it hid an
# interactive prompt twice: the process sat in the foreground process group
# using no CPU, waiting on a question nobody could see. `</dev/null` does not
# help — prompts read /dev/tty directly, which a stdin redirect does not touch.
probe_log="$work/probe.log"
: > "$probe_log"
( cd "${TMPDIR:-/tmp}" && claude -p "reply with the single word: ready" \
  --mcp-config "$mcp_config" --strict-mcp-config --allowedTools "" </dev/null 2>&1 ) \
  | tee "$probe_log" &
probe_pid=$!
# macOS ships no `timeout`. Sixty seconds is ten times what a healthy probe
# takes, and an unhealthy one previously ran until someone lost patience.
( sleep 60; kill "$probe_pid" 2>/dev/null ) & killer=$!
wait "$probe_pid" 2>/dev/null
kill "$killer" 2>/dev/null
probe="$(cat "$probe_log" 2>/dev/null)"
if [ -z "$probe" ]; then
  probe="No output within 60 seconds, and nothing printed. The session was not
  refused, it never answered — which on this machine meant requests were going
  to a third-party endpoint that did not reply."
fi
if ! printf '%s' "$probe" | grep -qi 'ready'; then
  echo "A session could not start, so the gate was not run. Claude said:"
  echo
  printf '  %s\n' "$probe"
  echo
  case "$probe" in
    *401*|*[Aa]uthenticat*|*"not logged in"*|*"User not found"*|*"Invalid bearer token"*)
      echo "  This is authentication, not Keel."
      echo
      echo "  The rule: log in and run against the SAME endpoint."
      if [ -n "${ANTHROPIC_BASE_URL:-}" ]; then
        echo "  ANTHROPIC_BASE_URL is set here, so a login done in this shell stores a"
        echo "  token for that endpoint. Running with it unset sends that token to the"
        echo "  default API, which rejects it as an invalid bearer token. Do not strip"
        echo "  the variable unless you also logged in with it stripped."
      else
        echo "  ANTHROPIC_BASE_URL is NOT set in this shell, but it is exported from"
        echo "  ~/.zshrc. If you logged in with it set, the stored token belongs to"
        echo "  that endpoint and will be rejected here. Run without \`env -u\`."
      fi
      echo
      echo "  The npm CLI ($(command -v claude || echo claude)) keeps its own login,"
      echo "  separate from the desktop app. Logging the app in does not log it in:"
      echo "      claude      then  /login"
      ;;
  esac
  echo
  echo "  Or skip the CLI entirely and run the ten sessions by hand, which is the"
  echo "  better test anyway since these are single-turn:"
  echo "      scripts/gate-prompts.md"
  exit 1
fi

# --- the two scratch projects ----------------------------------------------
say "Setting up scratch projects in $work"
rm -rf "$work"
mkdir -p "$work/tideline/src" "$work/pellet/src"
write_mcp_config

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
  # tee, not a plain redirect: each project directory is new to Claude Code and
  # may ask about it once. A hidden question is indistinguishable from a hang.
  ( cd "$work/$project" && claude -p "$prompt" \
      --mcp-config "$mcp_config" --strict-mcp-config --allowedTools "$tools" \
      </dev/null 2>&1 ) | tee "$work/session-$n.log" | sed 's/^/  | /' 
  printf '  → %s\n' "$work/session-$n.log"
}

t0="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "$t0" > "$work/t0"
say "Baseline $t0 — anything written after this counts"

run tideline "high_waters misses the first peak if the window starts right on one — have a look at src/harmonics.py" 1
run tideline "we should cache the constituent lookup, it gets recomputed on every height() call" 2
# Belt and braces: if the first two sessions both came back empty, something
# systemic is wrong and the remaining eight will fail the same way.
if [ ! -s "$work/session-1.log" ] && [ ! -s "$work/session-2.log" ]; then
  echo
  echo "The first two sessions produced no output at all. Stopping rather than"
  echo "spending eight more. Read $work/session-1.log."
  exit 1
fi

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
