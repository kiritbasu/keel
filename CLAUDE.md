# Specline

Local-first store for everything that describes a software project other than the code — specs, decisions, tasks, roadmap, design, feedback — with an MCP server as the primary interface and a Tauri desktop app as the read surface.

**All product documentation lives in `product/`.** Start there.

- `product/HANDOFF.md` — read once, first session
- `product/CLAUDE.md` — the standing contract, imported below
- `product/STATUS.md` — the tracker; current phase and task list
- `.specline/questions.md` — every question and risk, open and settled
- `product/DECISIONS.md` — build-time decision log
- `product/PRD.md` — what and why
- `product/SPEC.md` — how
- `product/GATE.md` — the unprompted-write measurement, and why it is frozen
- `product/JOURNAL.md` — what happened, session by session

The standing rules are imported here so they load in every session regardless of working directory:

@product/CLAUDE.md
