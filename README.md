# Keel

Keel is a local store for everything about a software project **except the code** — the specs, the decisions, the tasks, the open questions, the feedback, the reasons. It runs on your machine, an AI coding agent reads and writes it through [MCP](https://modelcontextprotocol.io), and a desktop app shows you what's in it.

---

## The problem

If you build software with an AI agent, you have probably noticed two things.

**Every session starts from nothing.** You explain the project again. What it is, what you decided last week, why the obvious approach doesn't work, what's half-finished. The agent is capable and has no memory, so the explaining never ends and it is never quite the same explanation twice.

**What you decide in a conversation evaporates.** You spend forty minutes working out that the queue has to be idempotent because retries are at-least-once, you both agree, the code gets written — and the reasoning exists nowhere. Six months later you find the code, can't remember why, and either rediscover the reasoning or quietly break it.

The usual answer is to write it down. In practice that means a wiki nobody updates, a `NOTES.md` that goes stale in a fortnight, and an issue tracker built for teams of thirty. All of them have the same flaw: **keeping them current is a separate chore from doing the work**, so it doesn't happen.

## What Keel does about it

The agent writes to Keel *while* you work, because it is right there in the conversation. No separate step, no context switch, no remembering.

- You mention a constraint → it becomes a **decision** with the reasoning attached.
- You say "we should probably…" → it becomes a **task**.
- Something turns out to be undecided → it becomes an **open question**, and every future session sees it before it re-litigates it.
- The agent finds out *why* something is slow → that goes on the task as a **note**, attributed to that conversation.

Next session — different day, different machine, no memory — starts by reading the store and knows where things stand.

**It stays yours.** Everything lives in `~/.keel` on your disk. No account, no cloud, no telemetry. The daemon binds `127.0.0.1` and nothing else can reach it.

**It writes readable files.** Keel generates markdown into your repository — the spec, the decision log, the tracker, the questions — so the whole thing is greppable, diffable and committed alongside your code. If Keel vanished tomorrow you'd still have the files.

### What it is not

- Not a team tracker. One person, one machine, no permissions, no assignees.
- Not a replacement for GitHub Issues if you have a team using them.
- Not a note-taking app. You don't type into it — you talk to the agent, and the agent writes.
- Not a chat log. It stores what became *true*, not what was said.

---

## Install

You need [Rust](https://rustup.rs) and Claude Code.

```bash
git clone <this repo> && cd keel
```

```bash
./plugin/install.sh
```

That builds the binaries, puts them in `~/.cargo/bin` — the same place a release installs, so there is only ever one copy — creates the store at `~/.keel`, and copies the agent's skill and hooks to `~/.claude/skills/keel/`.

SQLite is compiled in, so the binaries are self-contained — there is no database to install alongside them.

Then start the daemon and leave it running:

```bash
specline-daemon
```

It binds `127.0.0.1:7654`. Add `--embeddings` if you want semantic search as well as keyword search — the first run downloads the model, which takes a minute. Keyword search works either way.

**Not every build can do that.** The model needs the ONNX runtime, which has no prebuilt Intel macOS library and wants a newer glibc than the Linux binaries are built against, so those builds ship without it and `--embeddings` has nothing to switch on. It is a cargo feature, on by default, so a build from source has it wherever it links: `cargo build --no-default-features` leaves it out deliberately. `keel doctor` and `/api/health` both report which build you are running — a version number cannot tell you.

### Connect it to Claude

Register the MCP server once:

```bash
claude mcp add --scope user --transport http keel http://127.0.0.1:7654/mcp
```

Then add the hooks to `~/.claude/settings.json`. **`install.sh` deliberately won't edit this file for you** — it's yours, and a script rewriting your settings is one bug away from damage:

```json
{
  "hooks": {
    "SessionStart": [
      { "hooks": [ { "type": "command", "timeout": 10,
          "command": "/Users/you/.claude/skills/keel/keel-hook.sh session-start" } ] }
    ],
    "Stop": [
      { "hooks": [ { "type": "command", "timeout": 15,
          "command": "/Users/you/.claude/skills/keel/keel-hook.sh stop" } ] }
    ]
  }
}
```

The two hooks are what make it work without you asking:

- **SessionStart** injects a short summary of the project at the top of every conversation, so the agent is oriented before you type anything.
- **Stop** fires when a session ends having recorded nothing, and asks it to. It stays silent for sessions that already wrote — a prompt that fires when you've done the right thing is a prompt you turn off.

Check it:

```bash
curl -s http://127.0.0.1:7654/api/health
```

### Config

Everything has a working default. Override with environment variables:

| Variable | Default | What it does |
|---|---|---|
| `KEEL_HOME` | `~/.keel` | Where the store lives |
| `KEEL_DAEMON_URL` | `http://127.0.0.1:7654` | Where the CLI looks for the daemon |
| `KEEL_BIND` | `127.0.0.1:7654` | What the daemon binds |
| `KEEL_BIN_DIR` | `$CARGO_HOME/bin`, else `~/.cargo/bin` | Where `install.sh` puts binaries — the release installer's own default |
| `KEEL_SKILL_DIR` | `~/.claude/skills/keel` | Where the skill and hooks are installed |
| `KEEL_AUTO_UPDATE` | on | Set to `0` to stop the hourly check for a new release |

### What leaves your machine

One request, and it is worth naming rather than leaving to be discovered: the
daemon fetches the latest release manifest once an hour so it can tell you a new
version exists. It sends nothing from your store — no project names, no counts,
no identifier — and nothing is installed without you agreeing to the restart.

`--no-update-check` on `/keel:setup` turns it off at install time,
`KEEL_AUTO_UPDATE=0` turns it off afterwards, and `keel doctor` reports which it
is and when the last check ran. With it off, Keel makes no network requests at
all.

After editing anything under `plugin/`, re-run `./plugin/install.sh --skill-only`. It skips the build and copies the three files. **The copies under `~/.claude` are what actually run** — a change to the repo that isn't copied across does nothing at all.

---

## Using it

### Mostly, you don't

That's the point. Work with Claude the way you already do. Keel fills up as a side effect.

Some things that make the agent write, without you asking it to:

> "Let's go with the second option — Postgres, because we already run one."
> "That's a bug, the retry loop doesn't back off."
> "I don't know whether we need per-tenant keys. Leave it for now."

And things that make it read:

> "What's the state of the auth work?"
> "Why did we pick SQLite?"
> "What's blocking the release?"
> "What should I do next?"

### The interface

```bash
keel ui
```

The daemon serves it, compiled into the binary — no Node, no second process, nothing to start. It opens whatever address the daemon is actually listening on, so a non-default port needs no arguments.

A board, a roadmap, documents with revision history, a searchable everything, and an activity feed showing what changed and which conversation changed it.

To work on the interface itself, `npm run dev --prefix apps/desktop` gives you the usual hot reload against the same daemon.

**The app is read-only, on purpose.** Claude and the CLI are the only writers. If the app could also write, you'd have two sources of truth for the same row and no answer to which one is right when they disagree.

### The command line

You rarely need it, but:

```bash
keel status
```

```bash
keel fsck
```

```bash
keel generate <project>
```

```bash
keel backup
```

`fsck` checks referential integrity across both storage engines — 34 checks, and each finding says what it breaks and what to do about it. `generate` writes the markdown files into your repo. `backup` writes both engines to Parquet, and `restore` puts them back.

Everything works whether the daemon is running or not: the CLI asks the daemon when one is there, and opens the store directly when it isn't.

---

## Generated files

Point a project at your repo and Keel writes markdown into it:

```
product/SPEC.md          the spec, as prose
product/DECISIONS.md     every decision, numbered B-1, B-2…, with the reasoning
product/STATUS.md        the tracker — open work, rendered from the task rows
product/CHANGELOG.md     what closed, and the event log
.keel/questions.md       open questions and settled ones, with the answers
.keel/decisions/         one file per decision
.keel/specs/             one file per document
```

**These are outputs.** Every one carries a banner saying so. Editing them is not so much wrong as futile — the next `keel generate` overwrites them from the store, and your words are gone.

To change what they say, change the source: ask Claude to rewrite it, or edit it in the app. If you have already edited a file by hand and want the words kept, `keel import <file>` writes it back in as a proper revision.

There's a pre-commit hook that refuses a commit carrying a hand-edited generated file, so you find out immediately rather than when the next regeneration eats it:

```bash
ln -sf ../../scripts/pre-commit .git/hooks/pre-commit
```

---

## What's in it

Thirteen kinds of thing, and no more — the ceiling is deliberate, because "we need a new type for this" is nearly always a field or a label in disguise:

**project**, **milestone**, **task**, **spec**, **decision**, **question**, **term**, **feedback**, **design**, **environment**, **metric**, **metric observation**, **artifact**.

They're connected by a typed graph — a task `implements` a spec, a decision `supersedes` an earlier one, a task `blocks` another — and the graph is what makes "what's actually blocked" a query rather than a guess.

The agent sees ten tools: `specline_context`, `specline_search`, `specline_get`, `specline_projects`, `specline_activity`, `specline_create`, `specline_update`, `specline_write_doc`, `specline_note`, `specline_link`. Ten rather than forty because a model picks the right tool from a short list and the wrong one from a long list.

---

## Best practices

**Talk about the project, don't dictate records.** "We're going with Postgres because we already run one" gets you a decision with reasoning. "Create a decision record titled Postgres" gets you a row that says nothing in six months.

**Say the reason out loud.** The reasoning is the part that has value later — the choice itself is usually obvious in hindsight, and the rejected alternative almost never is.

**Use the readable IDs.** Tasks are `KEEL-42`, decisions are `B-12`. Say those in conversation; they're stable forever and they resolve everywhere an ID is taken.

**Let questions be questions.** If something is genuinely undecided, having it recorded as an open question is worth more than a confident guess. Every session sees open questions before it starts, which is what stops an agent quietly re-deciding something you already settled.

**Don't hand-edit generated files.** Change the source. The pre-commit hook will catch you, but the habit matters more.

**Run `keel fsck` occasionally**, and `keel backup` before anything drastic.

**One writer.** The daemon owns the only write path to the database. Don't run two daemons against one store.

**Restart the daemon after upgrading.** Keel refuses to start if the binary is older than the store's schema, which turns a silent corruption into an error you can read — but you still have to restart it.

---

## How it's built

Rust, one workspace, five crates.

```
crates/specline-core/     domain types, storage, graph, search, generation, backup
crates/specline-mcp/      the thirteen tools, the digest, protocol handling
crates/specline-embed/    the local embedding model, kept out of the core
crates/specline-daemon/   axum: the MCP endpoint and a local read API
crates/keel/          fsck, backup, restore, import, generate, notes —
                      and both shipped binaries, `keel` and `specline-daemon`
apps/desktop/         Tauri + React. Read and search only
```

Both binaries are declared by one package because `dist` builds one installer
per package that owns binaries, and there is meant to be one `keel-installer.sh`.
The daemon still lives in `specline-daemon`; only its entry point moved.

Storage is **one SQLite file** — entities, links, the event log, document revisions, images and vectors all in the same database. Search is hybrid: FTS5 keyword and `sqlite-vec` similarity, fused by reciprocal rank.

It was two engines until Phase 9: DuckDB for rows and Lance for documents, with the second attached into the first as a SQL namespace. That worked, but it cost a 22-minute build, a keyword index that was rebuilt wholesale on every write, and two backup formats that had to be kept in step. `product/SPEC.md` D-1 records the original reasoning and what overturned it.

Every change is an event with an author and the conversation that made it, so "who changed this and when" is always answerable.

### Development

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo fmt --all --check
```

657 Rust tests and 220 for the app.

### Where the documentation is

All of it is in `product/`, and all of it is generated from the store:

- `product/PRD.md` — what this is for
- `product/SPEC.md` — how it works
- `product/DECISIONS.md` — every decision and why
- `product/STATUS.md` — what's open and what's next
- `product/CHANGELOG.md` — what has closed, with the reason and the evidence
- `product/JOURNAL.md` — what happened, session by session
- `product/GATE.md` — the one measurement that mattered, and why it stopped
