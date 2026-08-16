#!/usr/bin/env bash
#
# Is the rename to Specline finished?
#
# A rename is the change where everything compiles and something is still
# wrong, because most of the surface is strings no compiler reads. This greps
# the tracked tree for the old name and fails on anything not deliberately
# kept.
#
# **The allowlist is the point of the script, not an escape hatch.** A rename
# that erases every trace of the old name also erases the record of what
# happened — the decisions that were made under it, the commands that really
# were called that, the transcripts the gate measurement is computed from. So
# each entry below says why, and an entry with no reason is a bug in this file.
#
# Run:  scripts/check-rename.sh          (fails on anything unexplained)
#       scripts/check-rename.sh --list   (prints every kept mention, with counts)

set -uo pipefail
cd "$(git rev-parse --show-toplevel)" || exit 1

list_only=0
[ "${1:-}" = "--list" ] && list_only=1

# --- What is deliberately kept ----------------------------------------------

# 1. Readable task ids. The project key is still KEEL (B-81), so `KEEL-42`
#    means the same task it always did. Lowercase `keel-42` appears in tests
#    asserting that a reference typed in a sentence still resolves.
keep='KEEL-[0-9]+|keel-[0-9]+|Keel-[0-9]+|KEEL-x|KEEL-B[0-9]+'
#    And the key on its own — `projectKey="KEEL"`, and the doc comments that
#    explain it is "the `KEEL` of `KEEL-42`".
keep="$keep"'|\bKEEL\b'

# 2. The old names, where the sentence is *about* the old name. `keel-cli` and
#    `keel-daemon` were real packages before they were merged; `keel mirror`
#    was a real command that was renamed out from under a hook. Rewriting these
#    produces a record of decisions nobody made.
keep="$keep"'|keel-cli|keel-daemon|keel-cli-installer|`keel mirror`'

# 3. Compatibility constants, each of which exists so that something written
#    under the old name still works: an old backup, an old store directory, an
#    old schema ledger, an old repository mirror.
keep="$keep"'|_keel_migrations|LEGACY_STORE_FILE|LEGACY_SNAPSHOT|LEGACY_HOME_DIR|LEGACY_MIRROR_DIR'
keep="$keep"'|relocated-from-keel|keel_version'

# 4. The gate scorer reads transcripts recorded before the rename, so it has to
#    go on recognising `mcp__keel__keel_…` forever. See rubric.rs.
keep="$keep"'|mcp__keel__[a-z_]*|old\.jsonl|name under Keel|moving a Keel one|Keel was renamed|vec!\[.keel_context.\]'

# 5. The old store filename, which `restore` still accepts, and the sentences
#    explaining why. See backup.rs: refusing an archive written under the old
#    name would break the recovery path at exactly the moment somebody needs it.
keep="$keep"'|keel\.sqlite|the name Keel|\.keel|directory Keel|Keel home|Keel used|Keel wrote|renamed to Specline|called Keel|keel_attach'

# 6. Two lines in rubric.rs that *are* the old prefix, and the pre-commit
#    hook's account of a stale `keel` binary that really was on PATH.
keep="$keep"'|TOOL_PREFIXES.*|from an earlier day on PATH|had a .keel. from'

# 7. The leaked-test-store sweeper recognises stores by filename, and old ones
#    are exactly what it is for. It looks for all three names on purpose.
keep="$keep"'|keel\.duckdb'

# --- Files that are records, not documentation ------------------------------
#
# The changelog and tracker are projections of closed rows: there is nothing to
# edit, and the rows they render describe work done under the old name. The
# journal and the phase plans are session-by-session history. BREAKING.md is a
# list of past breaking changes by definition. The decision log is the same
# shape: 87 decisions taken under the old name, rendered from their rows, and
# rewriting them would produce a record of decisions nobody made.
# This script itself: its allowlist is a list of the old name by construction.
excluded_files='^scripts/check-rename\.sh$|^product/DECISIONS\.md$|^product/CHANGELOG\.md$|^product/STATUS\.md$|^product/JOURNAL\.md$|^PHASE-[0-9]+\.md$|^contracts/BREAKING\.md$|package-lock\.json$'

# The generated mirror is written from the store, so a mention there is a
# mention in a document body and is reported against the document, not twice.
excluded_files="$excluded_files"'|^\.specline/'

hits=0
while IFS= read -r file; do
  [[ "$file" =~ $excluded_files ]] && continue
  while IFS= read -r line; do
    [ -z "$line" ] && continue
    n="${line%%:*}"
    text="${line#*:}"
    # Strip the explained mentions, then see whether any remain.
    stripped="$(printf '%s' "$text" | perl -pe "s/$keep//g")"
    if printf '%s' "$stripped" | grep -qi 'keel'; then
      if [ "$list_only" -eq 0 ]; then
        printf '%s:%s: %s\n' "$file" "$n" "$text"
      fi
      hits=$((hits + 1))
    fi
  done < <(grep -niE 'keel' -- "$file" 2>/dev/null)
done < <(git ls-files)

if [ "$list_only" -eq 1 ]; then
  echo "kept mentions, by shape:"
  git ls-files | grep -vE "$excluded_files" \
    | xargs grep -ohiE '[a-z0-9_.-]*keel[a-z0-9_.-]*' 2>/dev/null \
    | sort | uniq -c | sort -rn
  exit 0
fi

if [ "$hits" -gt 0 ]; then
  echo
  echo "$hits unexplained mention(s) of the old name." >&2
  echo "Either rename them, or add the reason to the allowlist in this script." >&2
  exit 1
fi

echo "No unexplained mentions of the old name."
