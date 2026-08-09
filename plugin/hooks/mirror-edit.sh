#!/usr/bin/env bash
#
# PostToolUse hook: turn an edit to a generated mirror file into a proper,
# attributed revision.
#
# ---------------------------------------------------------------------------
# Why this is not a sync
# ---------------------------------------------------------------------------
#
# The mirror is one-directional and the database is canonical (D-3). This hook
# reads a mirror file, which looks like a violation, so be precise about why it
# is not:
#
#   * It is **event-triggered, not reconciliation-triggered.** It fires only on
#     an edit this session just made. It never runs on a schedule, never on
#     startup, and never walks the directory.
#   * It reads the file **once, immediately**, as the payload of a known edit.
#     It does not compare mirror state against database state. That comparison
#     is what a sync is, and it is what is forbidden.
#   * The database wins **unconditionally** afterwards: the file is regenerated
#     from the new revision. If the write is rejected — stale version, failed
#     validation — the edit is discarded and the file reverts.
#
# So divergence has no window in which to accumulate. There is never a moment
# where two versions coexist and something has to decide which is right.
#
# The payoff is that an agent edits markdown the way it naturally wants to, and
# the system records a properly attributed, versioned revision. File-editing
# ergonomics without file-based truth.
#
# ---------------------------------------------------------------------------
# Failure mode to know about
# ---------------------------------------------------------------------------
#
# Hooks only run in Claude Code. An edit to a mirror file made from Claude chat
# or Cowork is simply lost on the next regeneration. The generated header in
# every file says so.

set -uo pipefail

KEEL_URL="${KEEL_URL:-http://127.0.0.1:7654}"

# The hook receives the tool-use payload on stdin.
payload="$(cat)"

file_path="$(printf '%s' "$payload" | jq -r '.tool_input.file_path // .tool_input.path // empty' 2>/dev/null)"
[ -z "$file_path" ] && exit 0

# Only mirror files. Anything else is an ordinary edit and none of our business.
case "$file_path" in
  */.keel/*) ;;
  *) exit 0 ;;
esac

# Never round-trip the aggregate files or the manifest. `questions.md` and
# `glossary.md` are many rows rendered into one document, so there is no single
# artifact to write an edit back to — an edit there cannot be attributed without
# guessing which row it belonged to, and guessing is how a mirror becomes a
# second source of truth.
case "$(basename "$file_path")" in
  questions.md|glossary.md|manifest.json|README.md)
    printf 'keel: %s is generated from many artifacts and cannot be edited in place. Edit the individual question or term instead — keel_get it by id, then keel_write_doc.\n' "$(basename "$file_path")" >&2
    exit 0
    ;;
esac

[ -f "$file_path" ] || exit 0

# The generated header carries the artifact id. No header means this is not a
# file we wrote, so leave it alone.
entity_id="$(grep -m1 -oE '(spc|dec|que|fbk|dsg)_[0-9A-HJKMNP-TV-Z]{26}' "$file_path" 2>/dev/null || true)"
if [ -z "$entity_id" ]; then
  exit 0
fi

# Strip the generated header and the rendered front matter, leaving the body the
# human or agent actually wrote. Everything up to and including the `**Id:**`
# line is regenerated, so it must not be fed back in as content.
body="$(awk '
  /^<!-- keel:generated/ { in_header = 1 }
  in_header { if (/-->/) in_header = 0; next }
  /^\*\*Id:\*\*/ { front_matter_done = 1; next }
  front_matter_done || /^#/ { print }
' "$file_path" | sed '/./,$!d')"

if [ -z "$(printf '%s' "$body" | tr -d '[:space:]')" ]; then
  exit 0
fi

# The session id the skill minted for this conversation, if it exported one.
session_id="${KEEL_SESSION_ID:-}"

request="$(jq -n \
  --arg id "$entity_id" \
  --arg body "$body" \
  --arg session "$session_id" \
  '{
     jsonrpc: "2.0",
     id: 1,
     method: "tools/call",
     params: {
       name: "keel_write_doc",
       arguments: (
         { id: $id, body: $body, surface: "code" }
         + (if $session == "" then {} else { session_id: $session } end)
       ),
       _meta: { "io.modelcontextprotocol/protocolVersion": "2026-07-28" }
     }
   }')"

response="$(curl -sS --max-time 10 \
  -X POST "$KEEL_URL/mcp" \
  -H 'Content-Type: application/json' \
  -H 'MCP-Protocol-Version: 2026-07-28' \
  -H 'Mcp-Method: tools/call' \
  -H 'Mcp-Name: keel_write_doc' \
  -d "$request" 2>/dev/null)" || {
    # The daemon being down must not fail the edit. Say so and move on: the
    # file is still on disk, and the next regeneration will overwrite it, which
    # is the documented behaviour rather than a surprise.
    printf 'keel: could not reach the daemon at %s, so this edit was not saved as a revision. It will be overwritten on the next regeneration.\n' "$KEEL_URL" >&2
    exit 0
  }

error="$(printf '%s' "$response" | jq -r '.error.message // empty' 2>/dev/null)"
if [ -n "$error" ]; then
  printf 'keel: the edit to %s was rejected and has NOT been saved: %s\n' "$(basename "$file_path")" "$error" >&2
  exit 0
fi

summary="$(printf '%s' "$response" | jq -r '.result.content[0].text // empty' 2>/dev/null)"
[ -n "$summary" ] && printf 'keel: %s\n' "$summary" >&2

# Regenerate from the database, so the file reflects what was actually stored
# rather than what was typed. This is the step that makes the database win
# unconditionally.
project_id="$(printf '%s' "$response" | jq -r '.result.structuredContent.document.project_id // empty' 2>/dev/null)"
repo_root="${file_path%%/.keel/*}"
if [ -n "$project_id" ] && command -v keel >/dev/null 2>&1; then
  keel mirror "$project_id" --repo "$repo_root" >/dev/null 2>&1 || true
fi

exit 0
