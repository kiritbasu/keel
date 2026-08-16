#!/usr/bin/env bash
#
# What a board load and a search cost, in milliseconds and kilobytes.
#
# KEEL-123. The board "felt slow now and then" for months and nobody had a
# number for it. This is the number — taken the same way every time, so the run
# before the SQLite migration and the run after it can be put side by side and
# the difference read off rather than argued about.
#
#   scripts/measure-performance.sh                        # the live daemon, specline
#   scripts/measure-performance.sh --project harbour      # another project
#   scripts/measure-performance.sh --rounds 30            # more samples
#   scripts/measure-performance.sh --base http://127.0.0.1:7655
#   scripts/measure-performance.sh --label after-sqlite   # names the run
#
# What it measures, and why that list:
#
#   - Every read the board makes on load, because that is the screen the
#     complaint was about.
#   - The digest and the full note bodies alongside their cheap replacements,
#     because the board used to fetch those two and the point of KEEL-123 was
#     partly to stop it. Keeping both in the table is what makes the saving
#     visible in the same run rather than in someone's memory of last week's.
#   - Search, because search is where the stall lives: the first search after
#     any write rebuilds the whole full-text index, so the number that matters
#     is the *worst* one, not the mean.
#
# It reports mean and max over N rounds, never a single sample. One sample of a
# local HTTP round trip is mostly whatever else the machine was doing.
#
# It does not write anything, needs no arguments, and can be run against a
# daemon that is being used at the same time — though the numbers will be worse
# if it is, and the footer says so.

set -euo pipefail

BASE="http://127.0.0.1:7654"
PROJECT="specline"
ROUNDS=20
LABEL=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --base) BASE="$2"; shift 2 ;;
    --project) PROJECT="$2"; shift 2 ;;
    --rounds) ROUNDS="$2"; shift 2 ;;
    --label) LABEL="$2"; shift 2 ;;
    -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
done

if ! curl -sf "$BASE/api/health" >/dev/null; then
  echo "No daemon answering at $BASE. Start one with \`specline-daemon\`, or pass --base." >&2
  exit 1
fi

# Each row is: group | name | path.
#
# `board:` is what a board load waits on today. `was:` is what it used to wait
# on and no longer does — kept so the saving is measured rather than remembered.
# `other:` is everything else worth a number.
READS=(
  "board|tasks|/api/entities?project=$PROJECT&type=task&limit=2000"
  "board|milestones|/api/entities?project=$PROJECT&type=milestone&limit=200"
  "board|note counts|/api/notes?project=$PROJECT&counts=true"
  "board|ranking + blocked|/api/ready?project=$PROJECT&blocked=true&limit=3"
  "was|digest (depth=full)|/api/context?project=$PROJECT&depth=full"
  "was|every note body|/api/notes?project=$PROJECT"
  "other|search|/api/search?query=sqlite&project=$PROJECT&limit=20"
  "other|ready (no blocked)|/api/ready?project=$PROJECT&limit=10"
  "other|what changed|/api/changes?project=$PROJECT&limit=50"
)

printf '\n'
printf 'Specline read budget — %s' "$BASE"
[[ -n "$LABEL" ]] && printf '  [%s]' "$LABEL"
printf '\n'
printf 'project %s · %s rounds · %s\n' "$PROJECT" "$ROUNDS" "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
printf '\n'
printf '%-8s %-22s %9s %9s %10s\n' "group" "read" "mean" "max" "size"
printf '%-8s %-22s %9s %9s %10s\n' "--------" "----------------------" "---------" "---------" "----------"

BOARD_MEAN_TOTAL=0
BOARD_BYTES_TOTAL=0
WAS_MEAN_TOTAL=0
WAS_BYTES_TOTAL=0

for row in "${READS[@]}"; do
  IFS='|' read -r group name path <<<"$row"
  url="$BASE$path"

  # One warm-up, discarded. The first read after a write pays for the full-text
  # index rebuild, and attributing that to whichever read happened to be first
  # would make the table depend on the order of this loop.
  curl -s -o /dev/null "$url" || true

  samples=""
  bytes=0
  for _ in $(seq 1 "$ROUNDS"); do
    out=$(curl -s -o /tmp/specline-measure-body.json -w '%{time_total} %{size_download}' "$url")
    samples="$samples ${out%% *}"
    bytes="${out##* }"
  done

  read -r mean max <<<"$(python3 -c "
import sys
s = [float(x) for x in '''$samples'''.split()]
print(f'{1000*sum(s)/len(s):.1f} {1000*max(s):.1f}')
")"

  printf '%-8s %-22s %8sms %8sms %9sKB\n' \
    "$group" "$name" "$mean" "$max" \
    "$(python3 -c "print(f'{$bytes/1024:.1f}')")"

  case "$group" in
    board)
      BOARD_MEAN_TOTAL=$(python3 -c "print(f'{$BOARD_MEAN_TOTAL + $mean:.1f}')")
      BOARD_BYTES_TOTAL=$((BOARD_BYTES_TOTAL + bytes))
      ;;
    was)
      WAS_MEAN_TOTAL=$(python3 -c "print(f'{$WAS_MEAN_TOTAL + $mean:.1f}')")
      WAS_BYTES_TOTAL=$((WAS_BYTES_TOTAL + bytes))
      ;;
  esac
done

# The two rows that matter to a person: what the board costs now, and what the
# two calls it dropped would have cost on top of the ones it kept.
printf '\n'
printf 'board load          %8sms serial %9sKB\n' \
  "$BOARD_MEAN_TOTAL" "$(python3 -c "print(f'{$BOARD_BYTES_TOTAL/1024:.1f}')")"
printf 'what it dropped     %8sms        %9sKB\n' \
  "$WAS_MEAN_TOTAL" "$(python3 -c "print(f'{$WAS_BYTES_TOTAL/1024:.1f}')")"
printf '\n'
printf 'Serial, so it is an upper bound — the board issues these in parallel.\n'
printf 'Numbers taken against a daemon in use will be worse; the store has one\n'
printf 'writer and one lock, so a session writing while this runs shows up here.\n'
printf '\n'

rm -f /tmp/specline-measure-body.json
