#!/usr/bin/env bash
#
# SessionStart — put Keel's digest into the session before anything else does.
#
# TQ-19. The skill was the orientation mechanism and it does not fire: thirty
# headless sessions and an interactive one, all with `keel` installed and
# discoverable, and not one invoked it. A skill is model-invoked, so the
# instructions inside it never entered play. Rewording a file nobody opens
# changes nothing.
#
# This inverts the dependency. Orientation stops being a decision the model
# makes and becomes something that happens to the session. Whether to *write*
# is still judgement — that is the part worth spending model attention on, and
# it is what remains in SKILL.md.
#
# Design constraints, in order of how badly getting them wrong would hurt:
#
#  1. **Never block a session.** This runs before the human's first word. A
#     daemon that is down, slow, or serving nonsense must cost nothing: every
#     failure path exits 0 with no output, and curl carries a hard timeout.
#  2. **Never inject noise.** An empty store or an unrecognised directory
#     produces a short, honest line rather than three screens of other
#     projects' business. Context spent on irrelevance is context taken from
#     the actual work.
#  3. **Never write.** SessionStart is a read. The store is not touched here,
#     so a hook that misfires cannot corrupt anything.

set -uo pipefail

daemon="${KEEL_DAEMON_URL:-http://127.0.0.1:7654}"

# Claude Code passes the hook a JSON payload on stdin. `cwd` is the field we
# want; jq is optional, so fall back to the shell's own idea of the directory
# rather than making jq a hard dependency of starting a session.
payload="$(cat 2>/dev/null || true)"
cwd=""
claude_session=""
if [ -n "$payload" ]; then
  read -r cwd claude_session <<EOF
$(printf '%s' "$payload" | python3 -c '
import json, sys
try:
    d = json.load(sys.stdin)
except Exception:
    d = {}
print(d.get("cwd", ""), d.get("session_id", ""))
' 2>/dev/null)
EOF
fi
[ -n "$cwd" ] || cwd="$PWD"

# Claude Code assigns every session a UUID and hands it to this hook. Telling
# the model to use *that* removes an entire failure class: asked to invent a
# unique id, sessions minted date-based ones, two collided, and the gate scored
# five writing sessions as three — which is the number the strategy was then
# built on. It also makes the event log joinable to the transcript, so "what
# the session did" and "what reached the store" can finally be compared.
session_hint=""
if [ -n "$claude_session" ]; then
  session_hint="Use exactly this on every Keel call: session_id = \"ses_${claude_session}\". \
It is this conversation's own identifier — do not invent one, and do not derive \
one from the date.

"
fi

# `--max-time` is the whole safety story: an unreachable daemon fails in two
# seconds, and a hanging one cannot hold the session open. Silence on failure
# is deliberate — a session that starts with a stack trace is worse than one
# that starts without Keel.
response="$(
  curl -sf --max-time 5 --get "$daemon/api/context" \
    --data-urlencode "cwd=$cwd" \
    --data-urlencode "depth=brief" 2>/dev/null
)" || exit 0
[ -n "$response" ] || exit 0

digest="$(
  printf '%s' "$response" | python3 -c '
import json, sys

try:
    body = json.load(sys.stdin)
except Exception:
    sys.exit(1)

summary = body.get("summary")
if not isinstance(summary, str) or not summary.strip():
    sys.exit(1)

data = body.get("data") or {}
project = data.get("project") or {}

# An unmatched directory gets one line, not a roll-up of unrelated projects.
# The digest already leads with the "no project matches" sentence in that
# case, and repeating the rest would spend context on other peoples business.
if not project:
    print(summary.split("\n\n")[0].strip())
    sys.exit(0)

print(summary.strip())
' 2>/dev/null
)" || exit 0
[ -n "$digest" ] || exit 0

# `additionalContext` is the documented way to add to a session without
# pretending to be the user or the assistant. Printed as JSON so the payload
# cannot be mistaken for a transcript line.
python3 - "$digest" "$session_hint" <<'PY' 2>/dev/null || exit 0
import json
import sys

digest = sys.argv[1]
session_hint = sys.argv[2] if len(sys.argv) > 2 else ""
preamble = (
    "Keel holds this project's specs, decisions, tasks, questions and history. "
    "You did not have to ask for this — it is here so you start oriented.\n\n"
    "Write back to it when something becomes true: a decision made, a task "
    "agreed, a question raised and left open, feedback heard. Use the keel_* "
    "tools.\n\n"
    "Record it rather than offering to. In a measured run, five of ten sessions "
    "worked out exactly what should be captured, drafted it, then asked "
    "permission and stopped — so it was lost. Write it, then say in one line "
    "that you did. Asking turns a free write into an interruption.\n\n"
    "If you pick up one of the tasks under Next below, set it to in_progress "
    "before you start — keel_update with the id shown, one call, no need to "
    "ask. On a long-running project this is the only way the human can see "
    "what is being worked on right now rather than only what has finished. "
    "Set it back to todo if you end up not doing it.\n\n"
)
preamble += session_hint
print(json.dumps({
    "hookSpecificOutput": {
        "hookEventName": "SessionStart",
        "additionalContext": preamble + digest,
    }
}))
PY
