"""One-off: turn the rows of DECISIONS.md and QUESTIONS.md into real artifacts.

Both files are stored in Keel as a single prose document each, so the individual
decisions and questions inside them are invisible to the board, to search
ranking, to links and to keel_context. This creates the ones that have no
artifact, and prefixes the existing ones with their canonical id so the two
representations can be matched by eye from then on.

Writes go through the daemon's MCP endpoint. Nothing here opens the store.
"""

import json
import re
import sys
import urllib.request

MCP = "http://127.0.0.1:7654/mcp"
SESSION = "ses_01KZKW4M8QJ3RTVN2P7XG9DAC1"
PROJECT = "keel"

# The twelve artifacts the bootstrap created, mapped to the row they came from.
# Hand-checked against the tables; there are only twelve and a wrong mapping
# would attach the wrong reasoning to the wrong id.
EXISTING = {
    "Schema creep kills it": "R-1",
    "The agent might simply not write to it": "R-2",
    "Retrieval quality may be mediocre": "R-3",
    "Lance is the one unhedged dependency": "R-5",
    "How long should the 2025-11-25 handshake be carried?": "TQ-11",
    "Should BM25 live in DuckDB rather than Lance?": "TQ-10",
    "Should idempotency_key be on all thirteen tables or only tasks?": "TQ-9",
    "How does a design image get into Keel from a Claude chat session?": "TQ-6",
    "Should Keel ingest anything automatically, or only explicit writes?": "Q-6",
    "What is the retention policy on the event log?": "Q-5",
    "Where does the store live, and does ~/.keel get a git remote?": "Q-2",
    "I maintained the tracker as prose all session and never touched a task row": "R-2a",
}

# Same for the twelve decisions. Fuzzy matching on the title text was tried
# first and silently proposed re-creating B-3, B-9, B-13 and B-17, which
# already exist under paraphrased titles — exactly the duplicate-artifact
# failure this whole exercise is meant to remove.
EXISTING_DECISIONS = {
    "chrono for time, not jiff": "B-1",
    "All Lance access goes through the DuckDB extension": "B-2",
    "Bundled DuckDB is a feature, not a requirement": "B-3",
    "ULIDs are minted from a monotonic generator": "B-9",
    "Every table gets idempotency_key, not just tasks": "B-10",
    "BM25 moves from Lance to DuckDB": "B-12",
    "Tool responses lift version to the top of the entity": "B-13",
    "The desktop app hand-writes its components": "B-14",
    "Keel's local REST API has more endpoints than the MCP surface has tools": "B-15",
    "Event summaries name artifacts, not ids": "B-16",
    "Serve MCP 2025-11-25 alongside 2026-07-28": "B-17",
    # "Fixture links are addressed by name, never by position" has no row in
    # DECISIONS.md at all — the mirror image of the gap being fixed here.
    # Left alone rather than given an invented id.
}

_id = [100]


def rpc(method, params):
    _id[0] += 1
    body = json.dumps(
        {"jsonrpc": "2.0", "id": _id[0], "method": method, "params": params}
    ).encode()
    req = urllib.request.Request(
        MCP,
        data=body,
        headers={"Content-Type": "application/json", "Accept": "application/json"},
    )
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            out = json.loads(r.read())
    except urllib.error.HTTPError as e:
        detail = e.read().decode()[:600]
        raise SystemExit(f"HTTP {e.code} on {json.dumps(params)[:300]}\n  -> {detail}")
    if "error" in out:
        raise SystemExit(f"MCP error: {out['error']}")
    result = out["result"]
    if result.get("isError"):
        raise SystemExit(f"tool error: {result}")
    return result.get("structuredContent", result)


def call(tool, args):
    args = dict(args)
    args.setdefault("session_id", SESSION)
    args.setdefault("surface", "code")
    return rpc("tools/call", {"name": tool, "arguments": args})


def cells(line):
    """Split a markdown table row into its cells."""
    parts = line.strip().strip("|").split("|")
    return [c.strip() for c in parts]


def strip_md(text):
    """Plain-ish text for a title: drop bold, code ticks and trailing colons."""
    text = re.sub(r"\*\*|`", "", text)
    return text.strip().rstrip(".").strip()


def short_title(text, limit=88):
    """First sentence, capped — the rest lives in the body."""
    text = strip_md(text)
    for sep in (". ", "? ", "! "):
        if sep in text[:limit + 20]:
            text = text.split(sep)[0] + sep.strip()
            break
    if len(text) > limit:
        text = text[:limit].rsplit(" ", 1)[0] + "…"
    return text


def rows(path, prefix_re):
    """Every table row in the file whose first cell looks like an id."""
    out = []
    for line in open(path):
        if not line.startswith("|"):
            continue
        c = cells(line)
        if len(c) < 3 or not re.fullmatch(prefix_re, c[0]):
            continue
        out.append(c)
    return out


def existing(entity_type):
    url = f"http://127.0.0.1:7654/api/entities?project={PROJECT}&type={entity_type}&limit=200"
    with urllib.request.urlopen(url, timeout=30) as r:
        return json.loads(r.read())["data"]["items"]


def main():
    apply = "--apply" in sys.argv

    # --- rename the twelve so every artifact carries its id ---------------
    renames = []
    for e in existing("question"):
        canonical = EXISTING.get(e["title"])
        if canonical and not e["title"].startswith(canonical):
            renames.append((e, f"{canonical} — {e['title']}"))

    # --- questions --------------------------------------------------------
    q_rows = rows("product/QUESTIONS.md", r"(Q|TQ|R)-\d+[a-z]?")
    have = {EXISTING.get(e["title"], e["title"].split(" — ")[0]) for e in existing("question")}
    new_questions = []
    for c in q_rows:
        qid = c[0]
        if qid in have:
            continue
        kind = "risk" if qid.startswith("R-") else "question"
        # The tables have different shapes; cell 1 is always the substance.
        body = "\n\n".join(
            f"**{label}:** {value}"
            for label, value in zip(
                ["Question", "Status", "Working assumption", "Cost of getting it wrong"]
                if len(c) >= 5
                else ["Question", "Status"] if len(c) == 3
                else ["Risk", "Mitigation", "Watch for"],
                c[1:],
            )
            if value and value != "—"
        )
        status = "accepted" if kind == "risk" else "open"
        for marker, value in [("`answered`", "answered"), ("`moot`", "moot"), ("`provisional`", "open")]:
            if any(marker in cell for cell in c):
                status = value
        new_questions.append(
            {
                "type": "question",
                "project": PROJECT,
                "title": f"{qid} — {short_title(c[1])}",
                "body": f"Row `{qid}` of the open-questions log.\n\n{body}",
                "fields": {"kind": kind, "status": status},
            }
        )

    # --- decisions --------------------------------------------------------
    d_rows = rows("product/DECISIONS.md", r"B-\d+")
    d_have = set()
    d_renames = []
    for e in existing("decision"):
        m = re.match(r"(B-\d+)", e["title"])
        if m:
            d_have.add(m.group(1))
            continue
        canonical = EXISTING_DECISIONS.get(e["title"])
        if canonical:
            d_have.add(canonical)
            # Deliberately not renamed: an accepted decision's content is
            # immutable (D-6) and the store enforces it. The id lives in the
            # body of the new ones instead, so all twenty-two read alike.
            pass
    new_decisions = []
    for c in d_rows:
        bid, date, decision, reasoning = c[0], c[1], c[2], c[3] if len(c) > 3 else ""
        reversible = c[4] if len(c) > 4 else "unknown"
        gist = strip_md(decision)
        if bid in d_have:
            continue
        new_decisions.append(
            {
                "type": "decision",
                "project": PROJECT,
                "title": short_title(decision),
                "body": f"`{bid}` · {date}\n\n**Decision.** {decision}\n\n**Reasoning.** {reasoning}\n\n**Reversible?** {reversible}",
                "fields": {"status": "accepted", "decided_at": f"{date}T12:00:00Z"},
            }
        )

    print(f"rename {len(renames)} question(s)")
    for e, new in renames:
        print(f"   {e['title'][:60]!r} -> {new[:60]!r}")
    print(f"create {len(new_questions)} question(s): {[q['title'].split(' — ')[0] for q in new_questions]}")
    print(f"rename {len(d_renames)} decision(s)")
    print(f"create {len(new_decisions)} decision(s): {[d['title'].split(' — ')[0] for d in new_decisions]}")

    if not apply:
        print("\ndry run — pass --apply to write")
        return

    for e, new in renames + d_renames:
        call("keel_update", {"id": e["id"], "version": e["version"], "changes": {"title": new}})
    for q in new_questions:
        call("keel_create", q)
    for d in new_decisions:
        call("keel_create", d)
    print("\ndone")


main()
