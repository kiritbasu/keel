#!/usr/bin/env python3
"""Generate the Roadmap redesign artboards.

Every artboard is the same Specline shell — the 208px rail, the Page header —
so the only thing that differs between them is the idea being tested. The shell
is generated rather than hand-copied five times because a rail that drifts
between options makes the options incomparable.

Values are lifted from apps/desktop/src/styles.css, resolved to their light
half. Data is the real store as of 2026-08-19.
"""

W, H = 1280, 820

# --- the real data -----------------------------------------------------------

ACTIVE = [
    ("Phase 10 — Release, distribution and install", 30, 36, "today",
     "Turn Specline into something a stranger can install and trust: three commands "
     "and a restart inside Claude Code, with nothing compiling on their machine."),
    ("Phase 11 — Hardening: deep engineering review and strengthening", 43, 47, "today",
     "Take a hard look at everything already built — how it is structured, how safe "
     "it is, how fast it is — then fix what the review finds, in priority order."),
    ("Phase 14 — Feature requests: the Inbox and the lifecycle", 2, 12, "today",
     "Feature requests get a lifecycle: an Inbox for signals, and a path from a "
     "sentence somebody said to a row on the board."),
]

COMPLETE = [
    ("Phase 4 — Integrations", 3, 3, "Aug 11",
     "GitHub App, design artifacts, metrics charts. Needs KB's GitHub account."),
    ("Phase 12 — Search that reads the whole document", 4, 4, "Aug 15",
     "Semantic search has never run, and three things have to be fixed before it can."),
    ("Phase 13 — Rename to Specline", 16, 16, "Aug 17",
     "Rename the product from Keel to Specline everywhere the name is load-bearing."),
]

SHIPPED = [
    ("Phase 0 — Spine", 17, 17, "Aug 9",
     "Storage, schema, event log, graph, search, backup. No network, no UI."),
    ("Phase 1 — Daemon", 19, 19, "Aug 11",
     "axum, the nine MCP tools, keel_context, concurrency safety, render-status."),
    ("Phase 2 — Plugin", 15, 15, "Aug 11",
     "Skill, session-ID threading, project confirmation, mirror hooks."),
    ("Phase 3 — Desktop", 11, 11, "Aug 9",
     "Tauri shell, daemon as sidecar, screens 1–6 and 9."),
    ("Phase 6 — Make the tracker real", 15, 15, "Aug 10",
     "Turn the tracker from prose into rows you can work: every task gets an address and a page."),
    ("Phase 7 — Clean up the footprint", 7, 7, "Aug 10",
     "Cut the documentation down and give each instruction one owner."),
    ("Phase 8 — The working loop", 23, 23, "Aug 18",
     "File a bug in seconds, see what's ready to start, read the board without opening every card."),
    ("Phase 9 — One database", 8, 8, "Aug 18",
     "Fold DuckDB and Lance into one database, so a backup is one file."),
]

CUT = [("Phase 5 — Remote", 1, 1, "Aug 11", "Deployable daemon, auth, mobile client.")]

RELEASES = [
    ("0.3.0", "Aug 18", "what to pick up next, with a page of its own",
     "specline_next got a front door: the same ranking the digest carries, as its own "
     "tool and its own screen."),
    ("0.2.1", "Aug 17", "store relocation fixes",
     "The rename moved the store from ~/.keel to ~/.specline, and the move did not "
     "survive contact with real installs."),
    ("0.2.0", "Aug 16", "Keel is now Specline",
     "The rename, everywhere at once: binaries, MCP tools, plugin, home directory."),
    ("0.1.5", "Aug 16", "three platforms, and no embeddings in a released binary",
     "Intel macOS and Linux joined arm64, which meant dropping the embedding model."),
    ("0.1.5-rc.1", "Aug 16", "a prerelease to exercise the release path",
     "A deliberate dry run of the whole install and update path."),
    ("0.1.4", "Aug 15", "the update restarts the daemon", ""),
    ("0.1.3", "Aug 15", "the installer verifies what it downloads", ""),
    ("0.1.2", "Aug 15", "the updater, and the manifest it reads", ""),
    ("0.1.1", "Aug 15", "private release assets", ""),
    ("0.1.0", "Aug 15", "the first installable build", ""),
]

# --- shell -------------------------------------------------------------------

HELMET = """<helmet>
  <link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Geist:wght@400;500;600&amp;family=Geist+Mono:wght@400;500&amp;display=swap">
  <style>
    :root {
      --sunken: oklch(0.965 0.003 250);
      --surface: oklch(0.985 0.002 250);
      --raised: oklch(1 0 0);
      --hover: oklch(0.955 0.004 250);
      --line: oklch(0.90 0.006 250);
      --line-strong: oklch(0.80 0.010 250);
      --ink: oklch(0.22 0.010 250);
      --muted: oklch(0.46 0.012 250);
      --faint: oklch(0.60 0.010 250);
      --accent: oklch(0.50 0.17 250);
      --accent-quiet: oklch(0.50 0.17 250 / 0.10);
      --brand: oklch(0.55 0.12 70);
      --good: oklch(0.50 0.14 155);
      --warn: oklch(0.54 0.13 70);
      --bad: oklch(0.52 0.20 25);
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Geist", ui-sans-serif, system-ui, sans-serif;
      font-size: 14px; line-height: 1.5;
      color: var(--ink); background: var(--surface);
      -webkit-font-smoothing: antialiased;
    }
    a { color: var(--accent); text-decoration: none; }
    a:hover { color: oklch(0.42 0.17 250); }
    .num { font-variant-numeric: tabular-nums; }
    .mono { font-family: "Geist Mono", ui-monospace, "SF Mono", monospace; font-variant-numeric: tabular-nums; }
  </style>
</helmet>"""

CHEVRON = ('<svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" '
           'stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
           '<path d="m6 9 6 6 6-6"></path></svg>')

NAV = [
    ("Overview", "1"), ("What's next", "2"), ("Board", "3"),
    ("Inbox", "4"), ("Roadmap", "5"), ("Library", "6"),
]
GLOBAL_NAV = [("All projects", "7"), ("Search", "8"), ("What changed", "9")]


def nav_item(label, key, selected):
    if selected:
        style = ("display:flex;align-items:center;gap:8px;padding:5px 10px;border-radius:5px;"
                 "background:var(--accent-quiet);color:var(--accent);font-weight:500;")
    else:
        style = ("display:flex;align-items:center;gap:8px;padding:5px 10px;border-radius:5px;"
                 "color:var(--muted);")
    return (f'<div style="{style}">'
            f'<span style="flex:1">{label}</span>'
            f'<span style="font-size:11px;opacity:0.6">{key}</span></div>')


def rail(selected="Roadmap", extra=None):
    """The 208px navigation rail. `extra` inserts one more project screen."""
    items = list(NAV)
    if extra:
        items.insert(5, extra)
    project = "".join(nav_item(l, k, l == selected) for l, k in items)
    glob = "".join(nav_item(l, k, False) for l, k in GLOBAL_NAV)
    return f"""<aside style="width:208px;flex-shrink:0;display:flex;flex-direction:column;background:var(--sunken);border-right:1px solid var(--line);">
      <div style="padding:16px 12px 10px;">
        <div style="font-size:20px;font-weight:600;letter-spacing:-0.015em;color:var(--brand);">Specline</div>
        <div style="font-size:11px;color:var(--faint);margin-top:1px;">the project spine</div>
      </div>
      <div style="display:flex;align-items:center;gap:8px;padding:0 12px 12px;">
        <span style="display:inline-flex;align-items:center;gap:4px;border:1px solid var(--line);border-radius:5px;padding:2px 6px;font-size:11px;color:var(--muted);background:var(--raised);">Specline {CHEVRON}</span>
        <span style="font-size:11px;color:var(--faint);margin-left:auto;">Jump to… ⌘K</span>
      </div>
      <div style="display:flex;flex-direction:column;gap:1px;padding:0 8px;">{project}</div>
      <div style="height:1px;background:var(--line);margin:12px 12px;"></div>
      <div style="display:flex;flex-direction:column;gap:1px;padding:0 8px;">{glob}</div>
      <div style="margin-top:auto;padding:12px;font-size:11px;color:var(--faint);">Specline v0.3.0</div>
    </aside>"""


def header(title, meta, toolbar="", crumb_leaf="Roadmap"):
    tb = (f'<div style="margin-top:10px;display:flex;flex-wrap:wrap;align-items:center;gap:6px;">{toolbar}</div>'
          if toolbar else "")
    return f"""<header style="flex-shrink:0;border-bottom:1px solid var(--line);padding:16px 24px 12px;">
        <nav style="margin-bottom:4px;display:flex;align-items:center;gap:6px;font-size:11px;color:var(--faint);">
          <span>Projects</span><span>/</span><span>specline</span><span>/</span><span>{crumb_leaf}</span>
        </nav>
        <div style="display:flex;align-items:baseline;gap:12px;">
          <h1 style="margin:0;font-size:20px;font-weight:600;letter-spacing:-0.015em;">{title}</h1>
          <span style="font-size:13px;color:var(--faint);">{meta}</span>
        </div>{tb}
      </header>"""


def badge(text, tone):
    tones = {
        "good": "color:var(--good);border-color:oklch(0.50 0.14 155 / 0.4);background:oklch(0.50 0.14 155 / 0.1);",
        "warn": "color:var(--warn);border-color:oklch(0.54 0.13 70 / 0.4);background:oklch(0.54 0.13 70 / 0.1);",
        "cut": "color:var(--faint);border-color:var(--line);background:var(--hover);text-decoration:line-through;",
        "plain": "color:var(--muted);border-color:var(--line);",
    }
    return (f'<span style="display:inline-flex;align-items:center;border:1px solid;border-radius:5px;'
            f'padding:2px 6px;font-size:11px;line-height:1;white-space:nowrap;{tones[tone]}">{text}</span>')


def bar(done, total, width=72):
    pct = round(done / total * 100) if total else 0
    return (f'<span aria-hidden="true" style="display:block;width:{width}px;height:5px;flex-shrink:0;'
            f'border-radius:999px;overflow:hidden;background:var(--line);">'
            f'<span style="display:block;height:100%;width:{pct}%;border-radius:999px;background:var(--accent);"></span></span>')


def tab(label, count, selected):
    if selected:
        style = ("display:inline-flex;align-items:center;gap:6px;padding:4px 10px;border-radius:5px;"
                 "background:var(--raised);color:var(--ink);font-weight:500;font-size:13px;"
                 "box-shadow:0 1px 2px oklch(0.22 0.01 250 / 0.08);")
        cs = "color:var(--muted);"
    else:
        style = ("display:inline-flex;align-items:center;gap:6px;padding:4px 10px;border-radius:5px;"
                 "color:var(--muted);font-size:13px;")
        cs = "color:var(--faint);"
    return f'<span style="{style}">{label}<span class="num" style="font-size:11px;{cs}">{count}</span></span>'


# --- row shapes --------------------------------------------------------------

def active_card(name, done, total, when, summary):
    return f"""<div style="display:flex;flex-direction:column;gap:6px;border:1px solid var(--line);border-left:2px solid var(--warn);border-radius:8px;background:var(--raised);padding:12px 14px;">
            <div style="display:flex;align-items:center;gap:8px;">
              <span style="font-weight:500;">{name}</span>
              {badge("active", "warn")}
              <span style="margin-left:auto;display:flex;align-items:center;gap:8px;font-size:13px;color:var(--faint);">
                {bar(done, total)}<span class="num">{done} / {total}</span>
                <span style="width:1px;height:11px;background:var(--line);"></span>
                <span>moved {when}</span>
              </span>
            </div>
            <p style="margin:0;font-size:13px;line-height:1.45;color:var(--muted);">{summary}</p>
          </div>"""


def quiet_row(name, done, total, when, summary, tone, label):
    return f"""<div style="display:flex;flex-direction:column;gap:1px;padding:7px 14px;border-radius:5px;">
            <div style="display:flex;align-items:center;gap:8px;">
              <span style="color:var(--ink);">{name}</span>
              {badge(label, tone)}
              <span style="margin-left:auto;display:flex;align-items:center;gap:8px;font-size:13px;color:var(--faint);">
                <span class="num">{done} / {total}</span>
                <span style="width:1px;height:11px;background:var(--line);"></span>
                <span style="width:52px;text-align:right;">{when}</span>
              </span>
            </div>
            <div style="font-size:12px;line-height:1.4;color:var(--faint);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{summary}</div>
          </div>"""


def group_label(text, count):
    return f"""<div style="display:flex;align-items:center;gap:8px;margin:18px 0 6px;">
            <span style="font-size:11px;font-weight:500;letter-spacing:0.04em;text-transform:uppercase;color:var(--faint);">{text}</span>
            <span class="num" style="font-size:11px;color:var(--faint);">{count}</span>
            <span style="flex:1;height:1px;background:var(--line);"></span>
          </div>"""


def phases_grouped():
    """In flight first, then what is waiting on a word, then the record."""
    out = [group_label("In flight", len(ACTIVE)),
           '<div style="display:flex;flex-direction:column;gap:8px;">']
    out += [active_card(*a) for a in ACTIVE]
    out.append("</div>")
    out.append(group_label("Finished, not yet declared", len(COMPLETE)))
    out.append('<div style="display:flex;flex-direction:column;gap:1px;">')
    out += [quiet_row(n, d, t, w, sm, "plain", "complete") for n, d, t, w, sm in COMPLETE]
    out.append("</div>")
    out.append(group_label("Shipped", len(SHIPPED)))
    out.append('<div style="display:flex;flex-direction:column;gap:1px;">')
    out += [quiet_row(n, d, t, w, sm, "good", "shipped") for n, d, t, w, sm in SHIPPED]
    out.append("</div>")
    out.append(group_label("Cut", len(CUT)))
    out.append('<div style="display:flex;flex-direction:column;gap:1px;">')
    out += [quiet_row(n, d, t, w, sm, "cut", "cut") for n, d, t, w, sm in CUT]
    out.append("</div>")
    return "\n".join(out)


def releases_table(limit=None, dense=False):
    rows = RELEASES[:limit] if limit else RELEASES
    out = []
    for v, when, title, blurb in rows:
        sub = ("" if dense or not blurb else
               f'<div style="font-size:12px;color:var(--faint);margin-top:2px;">{blurb}</div>')
        out.append(f"""<div style="display:flex;align-items:baseline;gap:12px;padding:8px 14px;border-radius:5px;">
            <span class="mono" style="width:74px;flex-shrink:0;font-size:13px;color:var(--ink);">{v}</span>
            <div style="flex:1;min-width:0;">
              <div style="font-size:13px;color:var(--muted);">{title}</div>{sub}
            </div>
            <span class="num" style="font-size:13px;color:var(--faint);width:48px;text-align:right;flex-shrink:0;">{when}</span>
          </div>""")
    return "\n".join(out)


def wrap(body):
    return f"""<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <script src="./support.js"></script>
</head>
<body>
<x-dc>
{HELMET}
<div style="width:{W}px;height:{H}px;display:flex;overflow:hidden;background:var(--surface);">
{body}
</div>
</x-dc>
<script data-dc-script data-props='{{"$preview":{{"width":{W},"height":{H}}}}}'>
class Component extends DCLogic {{
  renderVals() {{ return {{}}; }}
}}
</script>
</body>
</html>
"""


def scroll(inner, pad="16px 24px 24px"):
    return f'<div style="flex:1;min-height:0;overflow:hidden;padding:{pad};">{inner}</div>'


# --- Option A — two tabs on one screen ---------------------------------------

toolbar_a = (f'<div style="display:inline-flex;align-items:center;gap:2px;padding:2px;border-radius:6px;'
             f'background:var(--hover);border:1px solid var(--line);">'
             f'{tab("Phases", 15, True)}{tab("Releases", 10, False)}</div>')

option_a = wrap(f"""  {rail()}
  <main style="flex:1;min-width:0;display:flex;flex-direction:column;">
    {header("Roadmap", "specline", toolbar_a)}
    {scroll(phases_grouped())}
  </main>""")

# --- Option B — releases become their own screen ------------------------------

option_b = wrap(f"""  {rail(extra=("Releases", "6"))}
  <main style="flex:1;min-width:0;display:flex;flex-direction:column;">
    {header("Roadmap", "15 phases · 3 in flight")}
    {scroll(phases_grouped())}
  </main>""")

# --- Option C — the plan, with the record as a dated rail ---------------------

rail_releases = "\n".join(
    f"""<div style="display:flex;align-items:baseline;gap:8px;padding:5px 0;">
        <span class="mono" style="font-size:12px;color:var(--ink);width:62px;flex-shrink:0;">{v}</span>
        <span style="flex:1;min-width:0;font-size:12px;color:var(--faint);overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{title}</span>
        <span class="num" style="font-size:11px;color:var(--faint);flex-shrink:0;">{when}</span>
      </div>""" for v, when, title, _ in RELEASES)

option_c = wrap(f"""  {rail()}
  <main style="flex:1;min-width:0;display:flex;flex-direction:column;">
    {header("Roadmap", "specline")}
    <div style="flex:1;min-height:0;display:flex;overflow:hidden;">
      <div style="flex:1;min-width:0;overflow:hidden;padding:16px 20px 24px;">{phases_grouped()}</div>
      <div style="width:300px;flex-shrink:0;border-left:1px solid var(--line);background:var(--sunken);padding:16px 18px;overflow:hidden;">
        <div style="display:flex;align-items:baseline;gap:8px;margin-bottom:2px;">
          <span style="font-size:11px;font-weight:500;letter-spacing:0.04em;text-transform:uppercase;color:var(--faint);">Released</span>
          <span class="num" style="font-size:11px;color:var(--faint);">10</span>
          <a href="#" style="margin-left:auto;font-size:11px;">All</a>
        </div>
        <p style="margin:0 0 8px;font-size:11px;line-height:1.4;color:var(--faint);">What actually went out. Nothing here holds tasks.</p>
        {rail_releases}
      </div>
    </div>
  </main>""")

# --- Option D — one page, releases demoted rather than moved ------------------

option_d = wrap(f"""  {rail()}
  <main style="flex:1;min-width:0;display:flex;flex-direction:column;">
    {header("Roadmap", "specline")}
    {scroll(phases_grouped()
            + group_label("Released", 10)
            + '<p style="margin:-2px 0 6px;font-size:12px;color:var(--faint);">Ten versions, none of which holds a task. Kept here so the page answers &quot;what shipped&quot; as well as &quot;what is planned&quot;.</p>'
            + '<div style="display:flex;flex-direction:column;gap:1px;">' + releases_table(dense=True) + '</div>')}
  </main>""")

# --- The releases view, for A and B -------------------------------------------

toolbar_r = (f'<div style="display:inline-flex;align-items:center;gap:2px;padding:2px;border-radius:6px;'
             f'background:var(--hover);border:1px solid var(--line);">'
             f'{tab("Phases", 15, False)}{tab("Releases", 10, True)}</div>')

releases_view = wrap(f"""  {rail()}
  <main style="flex:1;min-width:0;display:flex;flex-direction:column;">
    {header("Roadmap", "specline", toolbar_r)}
    <div style="flex:1;min-height:0;overflow:hidden;padding:16px 24px 24px;">
      <div style="display:flex;align-items:baseline;gap:10px;padding:0 14px 8px;">
        <span style="font-size:11px;font-weight:500;letter-spacing:0.04em;text-transform:uppercase;color:var(--faint);width:74px;">Version</span>
        <span style="flex:1;font-size:11px;font-weight:500;letter-spacing:0.04em;text-transform:uppercase;color:var(--faint);">What went out</span>
        <span style="font-size:11px;font-weight:500;letter-spacing:0.04em;text-transform:uppercase;color:var(--faint);width:48px;text-align:right;">Shipped</span>
      </div>
      <div style="height:1px;background:var(--line);margin-bottom:4px;"></div>
      <div style="display:flex;flex-direction:column;gap:1px;">{releases_table()}</div>
    </div>
  </main>""")

for name, content in [
    ("Main.dc.html", option_a),
    ("OptionB.dc.html", option_b),
    ("OptionC.dc.html", option_c),
    ("OptionD.dc.html", option_d),
    ("Releases.dc.html", releases_view),
]:
    with open(name, "w") as fh:
        fh.write(content)
    print(f"wrote {name}")
