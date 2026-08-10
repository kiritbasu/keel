#!/usr/bin/env bash
#
# Stop — the last moment a session knows what became true.
#
# Step 6 of product/WAY-FORWARD.md, re-argued against the residual it actually
# addresses (TQ-21).
#
# The original justification was the closing-message boundary: an offer to
# record is composed *after* tool-calling has ended, so no instruction to
# "record it the moment it happens" can reach it. Run A has one offer across ten
# sessions. That justification is dead.
#
# The surviving one is better. Three sessions in Run A — s2, s7, s9, confirmed
# by KB as genuine misses — never noticed Keel at all. All three were pure
# implementation work: cache a lookup, fix `gc()` wiping a store on an empty
# keep set, make `put()` atomic. Heads-down in the code, the digest injected at
# session start is thousands of tokens back and has no salience at the moment
# the work finishes.
#
# Stop fires exactly then: the work is done, the session knows what happened,
# and nothing else is competing for attention.
#
# ---------------------------------------------------------------------------
# Four constraints, and the last one is what makes it safe
# ---------------------------------------------------------------------------
#
#  1. **No model call.** Deterministic text. A summariser would cost a call per
#     session, cannot reliably tell a decision from a mention, and its noise
#     would land in `keel_context` — the digest every future session reads.
#     Degraded digest degrades write quality: a loop with no natural floor.
#
#  2. **Guard on `stop_hook_active`.** Without it this blocks its own
#     continuation forever.
#
#  3. **At most once per session.** Enforced by a marker file, not by trust.
#
#  4. **Only for sessions that recorded nothing.** Seven of ten sessions in
#     Run A already write without being asked. Interrupting them to suggest
#     they write would be pure noise, and a forcing function that fires on
#     correct behaviour is one a user disables within a week. This hook is
#     silent for them and speaks only to the three that missed.
#
# Fails open in every direction: a missing daemon, an unparseable payload or a
# timeout all exit 0 silently. Blocking a session because a bookkeeping hook
# could not reach its store would be a far worse failure than a missed record.

set -uo pipefail

daemon="${KEEL_DAEMON_URL:-http://127.0.0.1:7654}"
state_dir="${TMPDIR:-/tmp}/keel-stop-hook"

payload="$(cat 2>/dev/null || true)"
[ -n "$payload" ] || exit 0

read -r session_id already_active <<EOF
$(printf '%s' "$payload" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    sys.exit(0)
print(d.get("session_id", ""), "yes" if d.get("stop_hook_active") else "no")
' 2>/dev/null)
EOF

[ -n "$session_id" ] || exit 0

# Constraint 2. Claude Code sets this when it is already continuing because of
# a stop hook; blocking again from here is an infinite loop.
[ "$already_active" = "yes" ] && exit 0

# Constraint 3. One nudge per session, ever.
mkdir -p "$state_dir" 2>/dev/null
marker="$state_dir/$session_id"
[ -f "$marker" ] && exit 0

# Constraint 4. Did this session already record something? The store is the
# only honest answer — a session can talk about recording without doing it,
# which is the entire failure this project exists to measure.
# Scoped by time, not just by limit. The event log returns oldest-first, so a
# bare `limit=200` on a store with 225 events returned everything *except* the
# writes being looked for — and the hook then nagged a session that had done
# exactly the right thing. A session cannot have written before it started, so
# a window comfortably longer than any conversation is both correct and bounded.
since="$(python3 -c '
import datetime
print((datetime.datetime.now(datetime.UTC) - datetime.timedelta(hours=12)).strftime("%Y-%m-%dT%H:%M:%SZ"))
' 2>/dev/null)"
wrote="$(
  curl -sf --max-time 5 --get "$daemon/api/activity" \
    --data-urlencode "limit=500" --data-urlencode "since=$since" 2>/dev/null \
    | python3 -c '
import json, sys
try:
    events = json.load(sys.stdin)["data"]["events"]
except Exception:
    # Unreachable or unparseable: say "wrote" so the hook stays silent. A
    # false nudge on every session in a project whose daemon is down would
    # make this the most annoying thing in the toolchain.
    print("yes")
    sys.exit(0)
target = "ses_" + sys.argv[1]
print("yes" if any(e.get("session_id") in (target, sys.argv[1]) for e in events) else "no")
' "$session_id" 2>/dev/null
)" || exit 0

[ "$wrote" != "no" ] && exit 0

touch "$marker" 2>/dev/null

# `block` sends this back into the session as a reason to continue. The wording
# is deliberately a question about *this* conversation rather than an
# instruction to use a tool: the failure being addressed is not reluctance, it
# is that Keel was out of mind entirely.
cat <<'JSON'
{
  "decision": "block",
  "reason": "Before you finish: nothing from this session has been recorded in Keel. Did anything become true here — a decision made, a bug worth remembering, a risk noticed, a question raised and left open, a task that should exist? If so, record it now with the keel_* tools; do not ask, and say in one line what you recorded. If genuinely nothing did — a routine edit, a question answered from the code — say so in one line and stop."
}
JSON
