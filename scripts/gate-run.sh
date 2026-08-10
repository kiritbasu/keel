#!/usr/bin/env bash
#
# Phase 2's gate: ten unprompted sessions.
#
# FROZEN, 2026-08-10. The gate met its criterion at 18 of 20 and nobody is
# running this any more. It is kept, and kept working, because the next time the
# agent's orientation changes it is the only way to find out what that did.
# What it measured and why it stopped: product/GATE.md.
#
# Rewritten after the validity audit. The previous version measured
# four runs and every one of them was wrong in a different way. What changed:
#
#  1. **Parallel, one store per session.** Was sequential: ten Claude sessions
#     in a queue, 15-20 minutes a run, and the author watching a blank terminal
#     asking why. Also removes DuckDB write-lock contention between sessions,
#     which would otherwise manufacture a fake product requirement.
#
#  2. **A continuation turn.** The old harness was `claude -p … </dev/null` —
#     one prompt, one response, exit. Five sessions ended with "I'll hold off
#     until you say go." There was no "you" and no next turn. The write was not
#     refused; it was scheduled for a turn the harness could not supply. This is
#     instrument repair, not treatment: it removes an artefact.
#
#  3. **A manifest.** The launcher is the only thing that knows how many
#     sessions it started. Deriving that from observations is survivorship bias,
#     and it is the exact bug that let seven silent sessions vanish and the
#     score read "3 of 3".
#
#  4. **Transcripts archived, teardown separated.** The store was once torn down
#     before the transcripts were read, destroying the only evidence that could
#     distinguish a claimed write from a real one.
#
#  5. **Reachability asserted.** A wedged daemon makes a failed write
#     indistinguishable from a non-write (Problem 5).
#
# Sessions run in throwaway projects that mention Keel nowhere. Prompts are
# ordinary developer talk. None mentions Keel or asks for anything recorded.

set -uo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
runs_dir="${GATE_RUNS:-$root/.gate-runs}"
keel="${KEEL_BIN:-$root/target/release/keel}"
daemon_bin="${KEEL_DAEMON_BIN:-$root/target/release/keel-daemon}"
run_id="${GATE_RUN_ID:-run-$(date -u +%Y%m%dT%H%M%SZ)}"
run_dir="$runs_dir/$run_id"
work="$run_dir/projects"
base_port="${GATE_BASE_PORT:-7710}"

tools="Read,Grep,Glob,Edit,Write,Skill"
tools="$tools,mcp__keel__keel_context,mcp__keel__keel_create,mcp__keel__keel_update"
tools="$tools,mcp__keel__keel_search,mcp__keel__keel_get,mcp__keel__keel_link"
tools="$tools,mcp__keel__keel_projects,mcp__keel__keel_activity,mcp__keel__keel_write_doc"

say() { printf '\n\033[1m%s\033[0m\n' "$*"; }

# --- whose API are we talking to? ------------------------------------------
#
# This machine's ~/.zshrc routes Claude Code through OpenRouter. Requests then
# either hang or 401, and even when they work the sessions run on whatever
# OpenRouter routes to — measuring some other model's response to SKILL.md,
# which is not the claim. All three variables move together or none of them.
if [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ] ||
   { [ -n "${ANTHROPIC_BASE_URL:-}" ] && [ "${ANTHROPIC_BASE_URL}" != "https://api.anthropic.com" ]; }; then
  echo "This shell points Claude Code at a third-party endpoint."
  echo "  ANTHROPIC_BASE_URL   = ${ANTHROPIC_BASE_URL:-<unset>}"
  if [ -n "${ANTHROPIC_AUTH_TOKEN:-}" ]; then
    echo "  ANTHROPIC_AUTH_TOKEN = <set, ${#ANTHROPIC_AUTH_TOKEN} chars — not printed>"
  fi
  echo
  echo "Re-run with all three unset together:"
  echo "    env -u ANTHROPIC_BASE_URL -u ANTHROPIC_AUTH_TOKEN -u ANTHROPIC_API_KEY \\"
  echo "        ./scripts/gate-run.sh"
  exit 1
fi

[ -f "$HOME/.claude/skills/keel/SKILL.md" ] || {
  echo "The skill is not installed. It is part of what is being measured:"
  echo "  cp -r $root/plugin/skills/keel ~/.claude/skills/keel"
  exit 1
}
[ -x "$keel" ] || { echo "keel binary not found at $keel"; exit 1; }
[ -x "$daemon_bin" ] || { echo "keel-daemon not found at $daemon_bin"; exit 1; }

# --- the ten sessions -------------------------------------------------------
# project | opening prompt | neutral continuation
#
# The continuation must NOT answer the offer. Answering it would be treatment:
# it would measure whether the model writes when told to, which nobody doubts.
# It moves the conversation on, exactly as a distracted human would.
SESSIONS=(
"tideline|high_waters misses the first peak if the window starts right on one — have a look at src/harmonics.py|ok. separately, does the datum default look right to you?"
"tideline|we should cache the constituent lookup, it gets recomputed on every height() call|right. what's the next most expensive thing in there?"
"tideline|let's go with 15-minute resolution as the default for the tide table rather than the current step|fine. does that change how table.py renders?"
"tideline|what's the risk if a station's chart datum is wrong? walk me through what breaks|understood. anything else in that file worth a second look?"
"tideline|a harbourmaster rang to say the 7-day table is unreadable on a phone — the times need to be local, not hours since epoch|ok. what would that do to the existing callers?"
"tideline|before I forget: constituent phases should be validated to 0-360, nothing checks that today|sure. is there anywhere else missing validation?"
"pellet|gc() in src/store.js deletes anything not in keep. if keep comes back empty by accident that wipes the store — fix it|thanks. what else in store.js is that unforgiving?"
"pellet|we picked sha256 early on but blake3 is a lot faster. worth switching?|ok, park it. how big are the blobs typically anyway?"
"pellet|what's stopping put() from being atomic? a crash mid-write leaves a truncated blob under a valid-looking digest|got it. does get() have a similar hole?"
"pellet|I want a size cap on the store with LRU eviction. roughly what's involved?|ok. which part of that would you do first?"
)
# Smoke-testing the harness on two sessions is cheaper than discovering a
# mechanical fault ten sessions in, which has now happened three times.
if [ -n "${GATE_LIMIT:-}" ]; then
  SESSIONS=("${SESSIONS[@]:0:$GATE_LIMIT}")
fi
launched=${#SESSIONS[@]}

say "Run $run_id — $launched sessions, in parallel"
rm -rf "$run_dir"; mkdir -p "$work" "$run_dir/logs" "$run_dir/transcripts" "$run_dir/stores"

# Captured before anything launches, and stored. Deriving it afterwards from
# the run directory's mtime gave a t0 *later* than the events it was meant to
# bound, so `since=t0` filtered out every write and the run scored 0% recall
# against a 70% ceiling.
t0="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "$t0" > "$run_dir/t0"

scaffold() {
  mkdir -p "$1/tideline/src" "$1/pellet/src"
  cat > "$1/tideline/README.md" <<'EOF'
# Tideline

Tide prediction for small harbours. Reads a station's harmonic constituents and
produces a 7-day tide table.
EOF
  cat > "$1/tideline/src/harmonics.py" <<'EOF'
"""Harmonic tide prediction."""
import math

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
  cat > "$1/tideline/src/table.py" <<'EOF'
"""Render a tide table."""
from harmonics import height, high_waters


def render(start, hours=168):
    rows = []
    for t in high_waters(start, hours):
        rows.append(f"{t:8.2f}h  {height(t):5.2f}m")
    return "\n".join(rows)
EOF
  cat > "$1/pellet/README.md" <<'EOF'
# Pellet

A tiny content-addressed blob store. Writes go to `.pellet/<sha256>`; reads are
by digest. Used by the build cache.
EOF
  cat > "$1/pellet/src/store.js" <<'EOF'
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
}

# --- launch -----------------------------------------------------------------
pids=(); ports=()
for i in $(seq 0 $((launched - 1))); do
  n=$((i + 1))
  IFS='|' read -r project prompt followup <<< "${SESSIONS[$i]}"
  port=$((base_port + i))
  ports+=("$port")

  # One store per session. Isolation is not tidiness: a shared store means ten
  # sessions contend on DuckDB's single write lock, and a write that loses that
  # race is indistinguishable in the log from a session that chose not to write.
  home="$run_dir/stores/s$n"
  sdir="$work/s$n"
  mkdir -p "$home" "$sdir"
  scaffold "$sdir"

  "$daemon_bin" --home "$home" --bind "127.0.0.1:$port" \
    > "$run_dir/logs/daemon-$n.log" 2>&1 &

  cat > "$sdir/mcp.json" <<JSON
{ "mcpServers": { "keel": { "type": "http", "url": "http://127.0.0.1:$port/mcp" } } }
JSON

  (
    for _ in $(seq 1 40); do
      curl -sf "http://127.0.0.1:$port/api/health" >/dev/null 2>&1 && break
      sleep 0.25
    done
    # Reachability is asserted before and after. A daemon that wedged mid-run
    # (Problem 5) turns a failed write into an apparent non-write, and the two
    # have opposite remedies.
    curl -sf "http://127.0.0.1:$port/api/health" >/dev/null 2>&1 \
      && echo up > "$run_dir/logs/reachable-before-$n" || echo DOWN > "$run_dir/logs/reachable-before-$n"

    cd "$sdir/$project" || exit 1
    KEEL_DAEMON_URL="http://127.0.0.1:$port" \
      claude -p "$prompt" \
        --mcp-config "$sdir/mcp.json" --strict-mcp-config --allowedTools "$tools" \
        </dev/null > "$run_dir/logs/session-$n-turn1.log" 2>&1

    # The continuation. The turn the old harness could not supply.
    KEEL_DAEMON_URL="http://127.0.0.1:$port" \
      claude -p "$followup" --continue \
        --mcp-config "$sdir/mcp.json" --strict-mcp-config --allowedTools "$tools" \
        </dev/null > "$run_dir/logs/session-$n-turn2.log" 2>&1

    curl -sf "http://127.0.0.1:$port/api/health" >/dev/null 2>&1 \
      && echo up > "$run_dir/logs/reachable-after-$n" || echo DOWN > "$run_dir/logs/reachable-after-$n"
  ) &
  pids+=($!)
  printf '  [%2d/%d] %-9s %s\n' "$n" "$launched" "$project" "${prompt:0:58}"
done

say "All $launched launched. Waiting."
for p in "${pids[@]}"; do wait "$p"; done

# --- archive ----------------------------------------------------------------
# Copied out before anything is torn down. The previous run destroyed its store
# before the transcripts were read, and the only reason run 4 could be
# re-audited at all is that Claude Code keeps its own copy.
say "Archiving transcripts"
manifest_sessions=""
for i in $(seq 0 $((launched - 1))); do
  n=$((i + 1))
  IFS='|' read -r project _ _ <<< "${SESSIONS[$i]}"
  # Claude Code encodes a project directory by replacing /, _ and . with -.
  # Getting this wrong loses every transcript, which the completeness assertion
  # catches — but only after the sessions have been spent.
  encoded="$(printf '%s' "$work/s$n/$project" | sed 's#^/#-#; s#/#-#g; s#_#-#g; s#\.#-#g')"
  src="$(ls -t "$HOME/.claude/projects/"*"$encoded"*/*.jsonl 2>/dev/null | head -1)"
  dest="$run_dir/transcripts/session-$n.jsonl"
  if [ -n "$src" ] && [ -f "$src" ]; then cp "$src" "$dest"; else dest=""; fi
  [ -n "$manifest_sessions" ] && manifest_sessions="$manifest_sessions,"
  sport=$((base_port + i))
  manifest_sessions="$manifest_sessions{\"n\":$n,\"project\":\"$project\",\"transcript\":\"$dest\",\"daemon\":\"http://127.0.0.1:$sport\"}"
done

cat > "$run_dir/manifest.json" <<JSON
{
  "run_id": "$run_id",
  "launched": $launched,
  "t0": "$t0",
  "daemon": "http://127.0.0.1:$base_port",
  "sessions": [$manifest_sessions]
}
JSON

wedged=0
for n in $(seq 1 $launched); do
  grep -q DOWN "$run_dir/logs/reachable-before-$n" 2>/dev/null && wedged=$((wedged+1))
  grep -q DOWN "$run_dir/logs/reachable-after-$n" 2>/dev/null && wedged=$((wedged+1))
done
[ "$wedged" -gt 0 ] && echo "  ⚠ $wedged reachability check(s) failed — some non-writes may be wedged daemons"

say "Scoring"
"$keel" gate --run "$run_dir"
status=$?

say "Stores are still up, and the run is archived at:"
echo "  $run_dir"
echo
echo "  Tear down when you have finished reading:"
echo "      pkill -f 'keel-daemon --home $run_dir'"
exit $status
