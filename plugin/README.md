# The Keel plugin

Three pieces:

| Piece | What it does |
|---|---|
| `.mcp.json` | Points Claude at the local daemon's MCP endpoint. |
| `skills/keel/SKILL.md` | Teaches Claude *when* to read and write. This is the load-bearing part. |
| `hooks/mirror-edit.sh` | Turns an edit to a generated `.keel/*.md` file into a properly attributed revision. |

The daemon is the machinery. **The skill is the product.** If Claude has to be
reminded to use Keel every session, the whole idea fails — which is why Phase 2
is a real phase and not an afterthought (PRD R-2).

---

## Install

```bash
./plugin/install.sh
```

That builds the binaries, installs them to `~/.local/bin`, creates the store at
`~/.keel`, and prints what to add to your Claude Code configuration.

Start the daemon:

```bash
keel-daemon
```

It binds `127.0.0.1:7654` and stays there. Add `--embeddings` to enable semantic
search; the first run downloads the model, and search works without it in the
meantime.

---

## Phase 2's exit criterion, and how to run it

> Across 10 unprompted sessions, Claude writes to Keel in ≥9, threads
> `session_id` on every write, and creates 0 duplicate projects.

**This is the one gate that cannot be automated, and it is the one that matters
most.** "Unprompted" is the entire claim. A test that calls the tool has, by
definition, prompted it. Nothing in the test suite touches this.

### How to run it honestly

Do ten ordinary sessions of real work across at least two projects. Do not
mention Keel. Do not say "remember to record that". Just work — talk through a
feature, fix a bug, decide something, take a customer call.

Then score it:

```bash
keel gate --since <the moment you started>
```

That reports, per session: whether it wrote, whether every write carried a
`session_id`, and whether any near-duplicate projects appeared. It excludes the
sentinel writers (`ses_bootstrap`, `ses_import`, …) — they write and they thread
an id, so counting them would make `keel import` ten times a passing grade. Under
ten sessions it reports `INCOMPLETE` and exits 0: not a pass, and not a fail
either.

*(The three commands previously documented here did not work. `keel activity`
was never a command, and `keel fsck` and `keel status` open the store directly,
so they fail while the daemon holds the write lock. The gate that mattered most
had no instrument — see TQ-15.)*

### Two things that will make it fail for the wrong reason

**Run the sessions somewhere other than the Keel repo.** `CLAUDE.md` here is
four hundred lines telling Claude what Keel is and to keep it updated. A session
started in this repository is about as prompted as a session gets.

**Install the skill and register the server for every project**, or the sessions
have nothing to fire:

```bash
cp -r plugin/skills/keel ~/.claude/skills/keel
claude mcp add --scope user --transport http keel http://127.0.0.1:7654/mcp
```

`scripts/gate-run.sh` does all of this: it checks the preconditions, builds two
throwaway projects that mention Keel nowhere, runs ten ordinary-sounding
sessions across them, and scores the result. **Run it from your own terminal** —
`claude -p` reports "Not logged in" when spawned from inside a Claude Code
session, so this is not something the agent can run for you.

### What the failure modes look like

| Symptom | What it means |
|---|---|
| Claude reads but never writes | The skill's triggers are too narrow, or the "write when something becomes true" table is not landing. |
| Writes appear, but `session_id` is null | The skill is being read but the threading instruction is being skipped. Move it earlier. |
| Forty tasks where eight would do | The consolidation section is losing to the model's instinct to be helpful. Strengthen it. |
| A second project for something that exists | The `keel_projects`-first instruction is not firing. This is the most damaging one — it quietly ruins the cross-project view. |

Each of those is a fix to `SKILL.md`, not to the daemon. Change the wording, run
another ten sessions.

---

## The mirror hook

Claude Code writes markdown well and edits it naturally. The hook lets it do
that against `.keel/**` without the mirror becoming a second source of truth.

An edit to `.keel/specs/storage.md` is intercepted, the body is sent to
`keel_write_doc` as a new revision, and the file is regenerated from the
database. If the write is rejected, the edit is discarded and the file reverts.

It reads a mirror file, which is worth being precise about rather than glossing.
What makes it safe is that it is **event-triggered, not
reconciliation-triggered**: it fires on an edit that just happened, reads once,
and the database wins unconditionally afterwards. It never compares mirror state
to database state — that comparison is what a sync is, and it is what D-3
forbids.

Three things it deliberately refuses:

- `questions.md` and `glossary.md` — many artifacts rendered into one file, so
  an edit cannot be attributed to one without guessing.
- `manifest.json` and `README.md` — pure machine output.
- Any file with no `keel:generated` header — not ours.

**Hooks only run in Claude Code.** An edit made from Claude chat or Cowork is
lost on the next regeneration. Every generated file's header says so. If that
turns out to bite, make `.keel/` read-only outside Claude Code sessions.

### Requirements

`jq` and `curl`. Both are almost certainly already installed.

---

## If the daemon is not running

Every tool call fails with a connection error, and the hook says so and gets out
of the way rather than failing your edit. Start it:

```bash
keel-daemon
```

Check it:

```bash
curl -s http://127.0.0.1:7654/api/health | jq
```
