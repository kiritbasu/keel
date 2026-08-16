#!/usr/bin/env python3
"""Rename the product in stored prose that describes it *now*.

Written because the alternative is worse. The prose lives in the store, not in
files, so `specline import` cannot reach it — and re-emitting 55 KB of bodies
by hand through `specline_write_doc` is the one editing operation this project
has already identified as able to go wrong without anything noticing.

So the substitution is computed from what is in the store and posted straight
back through the daemon, which is `specline-core`'s write path like everything
else. Nothing is transcribed, so nothing can be dropped in transcription.

`--apply` writes. Without it, prints every line that would change and stops.

What it deliberately does not touch is in `KEEP` below. The short version: a
readable task id means the same task forever, and a sentence quoting a command
or a package that really was called that is a record rather than a mistake.
"""

import argparse
import json
import re
import sqlite3
import subprocess
import sys
from pathlib import Path

STORE = Path.home() / ".specline" / "specline.sqlite"
DAEMON = "http://127.0.0.1:7654/mcp"

# Left alone. Each is either an identifier that still resolves, or a sentence
# about a name that really was that at the time.
KEEP = re.compile(
    r"KEEL-\d+|keel-\d+|Keel-\d+|KEEL-x"      # readable task ids
    r"|keel-cli|`keel mirror`"                  # packages and commands as they were
    r"|_keel_migrations"                        # the migration ledger, deliberately unrenamed
    r"|keel\.sqlite|\.keel\b"                   # the old store, named by the compatibility paths
    r"|mcp__keel__[a-z_]*"                      # transcripts recorded before the rename
    # Decision titles quoted verbatim. A quote that has been renamed is a
    # misquote, and these two are cited by id in a question that is asking
    # which decisions have no reasoning recorded.
    r"|Keel ships as a product|the package becomes keel"
)

# Applied in order, longest first, so `keel-core` is not eaten by bare `keel`.
RULES = [
    (re.compile(r"\bkeel[-_](core|daemon|mcp|update|embed)\b"), r"specline-\1"),
    (re.compile(r"\bkeel_(activity|claim|close|context|create|get|link|note|projects|ready|search|update|write_doc)\b"),
     r"specline_\1"),
    (re.compile(r"\bKEEL_([A-Z_]+)"), r"SPECLINE_\1"),
    (re.compile(r"\bkeel-receipt\b"), "specline-receipt"),
    # The glob, as prose writes it when it means "all of them".
    (re.compile(r"\bkeel_\*"), "specline_*"),
    # A slash is *not* excluded from the lookbehind. It was at first, to protect
    # `development/keel` — but that directory has been renamed too, and
    # excluding it silently skipped `/keel:setup`, `~/.cargo/bin/keel` and
    # `crates/keel/src/main.rs`, all of which are the thing being renamed.
    (re.compile(r"(?<![A-Za-z0-9_.-])keel(?![A-Za-z0-9_-])"), "specline"),
    (re.compile(r"(?<![A-Za-z0-9_-])Keel(?![A-Za-z0-9_-])"), "Specline"),
]

# Never touched. Two reasons, and the first is the one that nearly went wrong.
#
# **History.** A phase plan, a dated snapshot, a build journal, a frozen
# measurement and an outside review all say what was true when they were
# written. The first dry run of this script rewrote the journal's own account
# of the rename into "Specline became Specline", and turned the sentence about
# the crate `keel-update` colliding with the tool `keel_update` into one where
# both are spelled the same and the collision it describes is invisible. A
# substitution cannot tell a record from a description; only this list can.
#
# **The rename itself.** Phase 13 and KEEL-282 name the old product because
# that is their subject.
SKIP = {
    "spc_01KZR487EHQGGE3HV3JH3XN213": "Phase 8 plan",
    "spc_01KZR487RKNSTBD8V9WXV27NBP": "Phase 9 plan",
    "spc_01KZR4882HZTJ4HHGZ5Y6HQDPM": "Phase 10 plan",
    "spc_01KZPJXC5RG006KJANQ6G4TBQS": "dependency verification, a dated snapshot",
    "spc_01KZNA1ZQPM0MGY86BHKE98DZA": "the build journal",
    "spc_01KZPDVA3THNZG533KZZ6772JX": "the gate, frozen by decision",
    "spc_01KZYFPFNZEZT5VEZMDRTZV83N": "the outside review, a snapshot of findings",
    "mst_01M05CWTRS0J8D012KC1NZQK06": "Phase 13, whose subject is the rename",
    "tsk_01M05YT4Z5SDYQ7QBNZSFJAMW7": "KEEL-282, which counts the old name on purpose",
}


def convert(text):
    """Substitute around the protected spans, never inside them."""
    out, last = [], 0
    for m in KEEP.finditer(text):
        out.append(_sub(text[last:m.start()]))
        out.append(m.group(0))
        last = m.end()
    out.append(_sub(text[last:]))
    return "".join(out)


def _sub(chunk):
    for pattern, repl in RULES:
        chunk = pattern.sub(repl, chunk)
    return chunk


def call(tool, args):
    body = json.dumps({"jsonrpc": "2.0", "id": 1, "method": "tools/call",
                       "params": {"name": tool, "arguments": args}})
    proc = subprocess.run(
        ["curl", "-sS", "-X", "POST", DAEMON,
         "-H", "content-type: application/json",
         "-H", "accept: application/json, text/event-stream",
         "-H", "MCP-Protocol-Version: 2026-07-28",
         "-H", "Mcp-Method: tools/call",
         "-H", f"Mcp-Name: {tool}",
         "-d", body],
        capture_output=True, text=True, check=True)
    reply = json.loads(proc.stdout)
    if "error" in reply:
        raise SystemExit(f"  daemon refused: {reply['error']}")
    return reply["result"]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--apply", action="store_true")
    ap.add_argument("--session", required=True)
    opts = ap.parse_args()

    con = sqlite3.connect(f"file:{STORE}?mode=ro", uri=True)
    changed = 0

    # --- Rows: fields that are plain columns ---------------------------------
    plain = []
    for i, t, s, b, v in con.execute(
        "select id, title, coalesce(summary,''), coalesce(body,''), version from tasks "
        "where archived_at is null and closed_at is null"
    ):
        plain.append(("task", i, v, {"title": t, "summary": s, "body": b}))
    for i, t, d, v in con.execute(
        "select id, term, coalesce(definition,''), version from terms where archived_at is null"
    ):
        plain.append(("term", i, v, {"term": t, "definition": d}))
    for i, n, s, v in con.execute(
        "select id, name, coalesce(summary,''), version from milestones "
        "where archived_at is null and shipped_at is null"
    ):
        plain.append(("milestone", i, v, {"name": n, "summary": s}))
    for i, t, v in con.execute(
        "select id, title, version from questions where archived_at is null and status='open'"
    ):
        plain.append(("question", i, v, {"title": t}))
    for i, t, v in con.execute(
        "select id, title, version from specs where archived_at is null"
    ):
        plain.append(("spec", i, v, {"title": t}))

    for kind, ident, version, fields in plain:
        if ident in SKIP:
            continue
        edits = {k: convert(v) for k, v in fields.items() if v and convert(v) != v}
        if not edits:
            continue
        changed += 1
        print(f"\n{kind} {ident}")
        for k, new in edits.items():
            print(f"  {k}:")
            _show(fields[k], new)
        if opts.apply:
            call("specline_update", {"id": ident, "version": version, "changes": edits,
                                     "session_id": opts.session, "surface": "cli"})
            print("  → written")

    # --- Documents: prose bodies --------------------------------------------
    for kind, table in (("question", "questions"), ("spec", "specs")):
        cond = "and e.status='open'" if kind == "question" else ""
        for ident, title, body in con.execute(
            f"select e.id, e.title, d.body from {table} e "
            f"join documents d on d.entity_id = e.id and d.status='current' "
            f"where e.archived_at is null {cond}"
        ):
            if ident in SKIP:
                continue
            new = convert(body)
            if new == body:
                continue
            changed += 1
            print(f"\n{kind} document {ident} ({title})")
            _show(body, new)
            if opts.apply:
                call("specline_write_doc", {"id": ident, "body": new,
                                            "session_id": opts.session, "surface": "cli"})
                print("  → written")

    print(f"\n{changed} artifact(s) {'updated' if opts.apply else 'would change'}")
    return 0


def _show(old, new):
    """Print only the lines that differ, so a dry run is readable."""
    for a, b in zip(old.splitlines(), new.splitlines()):
        if a != b:
            frag = re.search(r".{0,45}(specline|Specline).{0,45}", b, re.I)
            print(f"    - …{(frag.group(0) if frag else b)[:110]}…")


if __name__ == "__main__":
    sys.exit(main())
