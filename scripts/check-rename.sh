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
#    And decision titles quoted verbatim: a renamed quote is a misquote.
keep="$keep"'|Keel ships as a product|the package becomes keel'

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
keep="$keep"'|keel\.sqlite|the name Keel|\.keel|directory Keel|Keel home|Keel-shaped|Keel used|Keel wrote|renamed to Specline|called Keel|keel_attach'

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
# `rename-stored-prose.py` joins it for the same reason: its patterns *are* a
# list of the old name.
excluded_files='^scripts/check-rename\.sh$|^scripts/rename-stored-prose\.py$|^product/DECISIONS\.md$|^product/CHANGELOG\.md$|^product/STATUS\.md$|^product/JOURNAL\.md$|^PHASE-[0-9]+\.md$|^contracts/BREAKING\.md$|package-lock\.json$'

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


# --- The store, which is the other half of the surface ----------------------
#
# The tree was never the whole answer and this script said so only by omission.
# `.specline/` is excluded above because a mention there is a mention in a
# document body — but that reasoning holds only for documents that *also* have
# a file elsewhere. Every decision, question and glossary term renders into the
# mirror and nowhere else, so excluding the mirror removed their only copy from
# view and nothing reported them at all. The check was green from the day it
# was written while 124 stored bodies still said the old name (KEEL-282).
#
# **History is excluded here, not allowlisted.** A decision records what was
# decided at the time, a closed task records work that was done, an answered
# question records a thing that was settled — all under the old name, all
# true. Rewriting them produces a record of decisions nobody made. So this
# pass reads only prose that describes the product *now*: open questions, open
# tasks, the glossary, and specs that are not dated snapshots or phase plans.
# Specs that are records of a moment rather than descriptions of the product,
# named individually because the distinction is a judgement and not a query.
# A phase plan, a dated snapshot, a build journal, a frozen measurement and an
# outside review all describe what was true when they were written, under the
# name it had. Their titles are handled separately — a title is a label you
# navigate by today, a body is the record.
store_history="
  'spc_01KZR487EHQGGE3HV3JH3XN213',  -- Phase 8 plan
  'spc_01KZR487RKNSTBD8V9WXV27NBP',  -- Phase 9 plan
  'spc_01KZR4882HZTJ4HHGZ5Y6HQDPM',  -- Phase 10 plan
  'spc_01KZPJXC5RG006KJANQ6G4TBQS',  -- dependency verification, dated snapshot
  'spc_01KZNA1ZQPM0MGY86BHKE98DZA',  -- the build journal
  'spc_01KZPDVA3THNZG533KZZ6772JX',  -- the gate, frozen by decision
  'spc_01KZYFPFNZEZT5VEZMDRTZV83N'   -- the outside review, a snapshot of findings
"

store="${SPECLINE_HOME:-$HOME/.specline}/specline.sqlite"

# Python rather than shell, and not for elegance. A document body contains
# pipes, tabs and newlines, so every shell-friendly separator appears in the
# data; two attempts at this in bash produced a loop that silently read 15
# rows of 60 and a query that failed into an empty result. Both reported
# clean. A scanner for "is the check lying" must not be the easiest thing in
# the file to make lie.
if [ -r "$store" ]; then
  if ! command -v python3 >/dev/null 2>&1; then
    echo "check-rename: no python3, so stored prose was NOT checked." >&2
    exit 2
  fi
  store_out="$(python3 - "$store" "$keep" <<'PYEOF'
import re, sqlite3, sys

store, keep = sys.argv[1], sys.argv[2]

# Specs that record a moment rather than describe the product. Named one by
# one because the distinction is a judgement: a phase plan, a dated snapshot,
# a build journal, a frozen measurement and an outside review all say what was
# true when they were written, under the name it had then.
HISTORY = {
    "spc_01KZR487EHQGGE3HV3JH3XN213": "Phase 8 plan",
    "spc_01KZR487RKNSTBD8V9WXV27NBP": "Phase 9 plan",
    "spc_01KZR4882HZTJ4HHGZ5Y6HQDPM": "Phase 10 plan",
    "spc_01KZPJXC5RG006KJANQ6G4TBQS": "dependency verification, a dated snapshot",
    "spc_01KZNA1ZQPM0MGY86BHKE98DZA": "the build journal",
    "spc_01KZPDVA3THNZG533KZZ6772JX": "the gate, frozen by decision",
    "spc_01KZYFPFNZEZT5VEZMDRTZV83N": "the outside review, a snapshot of findings",
    # Not history — their subject *is* the rename, so naming the old product
    # is what makes them accurate.
    "mst_01M05CWTRS0J8D012KC1NZQK06": "Phase 13, the rename itself",
    "tsk_01M05YT4Z5SDYQ7QBNZSFJAMW7": "KEEL-282, which counts the old name on purpose",
}

con = sqlite3.connect(f"file:{store}?mode=ro", uri=True)

def rows(sql):
    return con.execute(sql).fetchall()

items = []
for i, t, b in rows("""
    select q.id, q.title, d.body from questions q
      left join documents d on d.entity_id = q.id and d.status = 'current'
      where q.archived_at is null and q.status = 'open'"""):
    items.append(("question", i, t, f"{t} {b or ''}"))

for i, t, sm, b in rows("""
    select id, title, coalesce(summary,''), coalesce(body,'') from tasks
      where archived_at is null and closed_at is null"""):
    items.append(("task", i, t, f"{t} {sm} {b}"))

for i, t, d in rows("""
    select id, term, coalesce(definition,'') from terms where archived_at is null"""):
    items.append(("term", i, t, f"{t} {d}"))

for i, t, sm in rows("""
    select id, name, coalesce(summary,'') from milestones
      where archived_at is null and shipped_at is null"""):
    items.append(("milestone", i, t, f"{t} {sm}"))

for i, t, b in rows("""
    select s.id, s.title, d.body from specs s
      left join documents d on d.entity_id = s.id and d.status = 'current'
      where s.archived_at is null"""):
    items.append(("spec", i, t, f"{t} {b or ''}"))

pat = re.compile(keep)
found = 0
for kind, ident, label, text in items:
    if ident in HISTORY:
        continue
    if "keel" not in text.lower():
        continue
    stripped = pat.sub("", text)
    if "keel" not in stripped.lower():
        continue
    m = re.search(r".{0,60}keel.{0,60}", stripped, re.I | re.S)
    snippet = " ".join(m.group(0).split()) if m else ""
    print(f"store {kind} {ident} ({label})")
    print(f"      …{snippet}…")
    found += 1
print(f"__HITS__ {found}")
PYEOF
)" || { echo "check-rename: the store scan failed, so stored prose was NOT checked." >&2; exit 2; }

  store_hits="$(printf '%s' "$store_out" | sed -n 's/^__HITS__ //p')"
  [ "$list_only" -eq 0 ] && printf '%s' "$store_out" | grep -v '^__HITS__' | sed '/^$/d'
  hits=$((hits + ${store_hits:-0}))
else
  echo "check-rename: no store at $store, so stored prose was NOT checked." >&2
  exit 2
fi

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
