<!-- keel:generated spec spc_01KZKSME2TCPVARX9M04836XD6
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — standing instructions

This file is loaded automatically into every Claude Code session in this repo. It is the contract. `product/HANDOFF.md` is orientation you read once; this is what you follow every time.

---

## Session ritual

**At the start of every session, in this order:**

1. Read `product/STATUS.md`. It tells you the current phase, what's in progress, and what's blocked. It is rendered from the task rows — read it, never edit it.
2. Read `.keel/questions.md`. It has two halves and you need both. **Open** is undecided: nothing there may be built on without saying so, and anything marked `blocked` halts work that depends on it. **Settled** is decided, with the reasoning — do not re-litigate it. Both halves are generated from the question rows, so there is nothing to keep in step.
3. `git log --oneline -15` — see what the last session actually did.
4. State in one line what you're picking up before you touch anything, and `keel_claim` it. `keel_ready` is what to ask if the tracker leaves the choice open.

**At the end of every session, without exception:**

1. Move the task rows — `keel_close` what you finished, with the reason, a message and the evidence, and put anything you learned as a note on the row it belongs to. The tracker and the changelog both derive from this; there is no second place to update.
2. Add any decisions you made to the decision log.
3. Add any new unknowns as question rows.
4. **Regenerate**: `keel generate keel`. See "Keel is the source of truth" below — the files in `product/` are outputs, and an edit that never reaches Keel is lost on the next run.
5. Commit. Never leave the tree dirty for the next session.
6. Close with a two-line summary: what landed, what's next.

**If you run out of context mid-task**, update the tracker first with enough detail for a fresh session to resume, then stop. An accurate tracker is more valuable than one more file edited.

---

## Keel is the source of truth

Every markdown file in `product/` and `.keel/` is **generated**. Each one carries
a `<!-- keel:generated … -->` banner naming the artifact it came from. Editing
one directly is not wrong so much as futile: the next `keel generate keel`
overwrites it from the store.

The loop is:

1. Edit the prose *in Keel* — `keel_write_doc` over MCP, or in the app.
2. `keel generate keel` writes the files.
3. Commit the result.

**An edit made to a generated file is lost, not recovered.** There is no hook
that captures it. There was one — a `PostToolUse` hook that claimed to turn an
edit into an attributed revision — and it never worked: it called `keel mirror`,
a command that had been renamed out from under it, and swallowed the failure.
Every edit it claimed to capture was silently gone. It was deleted rather than
repaired, because a safety mechanism that quietly does nothing is worse than
none: it is *relied upon*. `scripts/pre-commit` now refuses the commit instead,
which is loud and which cannot be wrong about what it did.

If you have edited files by hand and want them in — during a migration, say —
`keel import <files> --project keel` writes each one back as a revision. Stop
the daemon first. SQLite would let the import open the store alongside a running
daemon, which is exactly why this is worth saying: nothing would stop you, and
you would have two writers against a store whose design assumes one.

`keel generate keel --check` exits non-zero when a file differs from what Keel
would produce. The pre-commit hook runs it for you; run it yourself if you are
committing with `--no-verify`.

**One file is not generated and must not be**: the repository root's
`CLAUDE.md`. Claude Code loads it before anything else and imports this file
from it, so it is the bootstrap and cannot itself depend on a generation step
having run. Everything under `product/` and `.keel/`, this file included, is an
output.

---

## Tracker discipline

KB's primary window into this project is the desktop app, with `product/STATUS.md` as its committed shadow. Both read the same rows, so a stale tracker now means stale *rows* — there is nothing to update separately, and nothing that can drift.

**Rules:**

- **Claim a task before starting it**, not after: `keel_claim` over MCP, or `keel claim KEEL-42` from a terminal. That records who is on it as well as moving the status, so the app answers "what is happening right now" and not only "what has finished". This used to be an instruction you had to remember, and across sixty-six tasks the number of transitions into `in_progress` before work began was zero — which is why it is a tool now.
- **Ask `keel_ready` what to pick up.** It is the ranking the digest carries, with a front door of its own and filters: unclaimed, by label, by milestone. Reaching for it costs a fraction of a full digest, and it orders by what a task unblocks before its priority.
- A task is `done` only when it meets the definition of done below. Not when the code is written.
- If a task turns out to be bigger than one task, split it and record the split. Don't silently expand scope.
- If you're blocked, say so on the row. **`blocked` is not a status** — it is derived from the `blocks` edges, so the way to mark something blocked is to draw the edge that blocks it. Never leave something `in_progress` across sessions without a note.
- Record what you *found* as a note on the task — `keel_note` over MCP, or `keel note add <task-id> "…"` from a terminal — not as a line in a markdown table. A status without the finding behind it is a colour, not information.
- The changelog is derived from the event log, so it writes itself. A session that achieved nothing still leaves a trace, which was the point of insisting on the entry.
- **Never delete a task.** Close it with `keel_close` and a reason. The five reasons are `done`, `wont_do`, `duplicate`, `superseded` and `no_change`; every one needs a message, and `done` needs at least one piece of evidence — `commit:<sha>`, `pr:<url>`, `test:<command>`, `doc:<id>`, `url:<url>` or `image:<blob-id>`. `duplicate` and `superseded` name the other task and draw the edge themselves.

  This line used to say "mark it `dropped` with a reason". There has never been a `dropped` status, so a session following it literally got an enum rejection listing five values, none of them the word — quietly, for as long as it was written down.

**Task IDs are stable and never reused.** `KEEL-42` means the same thing forever, and it is what to use in conversation — the ULID underneath it is for machines.

**The tracker is rows, not prose.** `product/STATUS.md` is rendered from the
task rows by `keel render-status`. There is no tracker document to edit and no
markdown table to keep in step — changing what the tracker says means changing a
row. This closed TQ-14, and it closed the gap that made the question worth
asking: task rows now carry a **note stream**, so the findings that used to live
in the tracker's Notes column live on the task itself, attributed to the session
that learned them.

Two consequences worth internalising:

- **Never hand-edit `product/STATUS.md`.** It has no stored copy at all — it is
  a projection of rows, so there is nothing an edit could become a revision
  *of*. The next render overwrites it.
- **The narrative moved.** Session-by-session accounts — what was tried, what
  broke, what a measurement actually said — are in `product/JOURNAL.md`, which
  *is* a document and is edited like any other prose. Findings that belong to
  one task go on that task as a note; the story of a session goes in the
  journal.

---

## Definition of done

A task is not done until all of these are true:

- [ ] Code compiles with zero warnings: `cargo clippy --workspace --all-targets -- -D warnings`
      *(was `--all-features`; dropped 2026-08-09 because no workspace crate declared a feature it changed anything for, while it forced a second full build of the vendored DuckDB and filled the disk. See DECISIONS B-11. Since Phase 9 no workspace crate declares a feature at all — the `bundled` chain existed only to let someone link a system DuckDB instead of the vendored one — so the flag is now a no-op rather than a hazard. It stays dropped because there is nothing left for it to do.)*
- [ ] Formatted: `cargo fmt --all --check`
- [ ] Tests written **and** passing — including at least one failure case, not only the happy path. The one exception is a *forward-looking* test for behaviour a later phase delivers: mark it `#[ignore = "unblocks in Phase N — see STATUS.md KEEL-x"]` so CI stays green and the intent stays visible. Never `#[ignore]` a test for behaviour the current phase is supposed to deliver.
- [ ] No `unwrap()`, `expect()`, or `panic!()` in library code (binaries and tests may, with a message)
- [ ] Public items in `keel-core` have doc comments explaining *why*, not restating the signature
- [ ] The task **closed in Keel** with `keel_close` — reason `done`, a message saying what happened, and at least one piece of evidence — plus anything learned recorded as a note on it, and `keel generate keel` run
- [ ] Committed with a message that explains the change, not the diff

Three of these are now enforced rather than asked for. A task cannot reach a terminal status without a reason, a message and — for `done` — evidence: the check is in the storage layer, so the CLI and MCP cannot disagree and moving the status by hand does not get round it. The rest of this list is still a list.

If you can't tick all of them, the task is `in_progress`.

---

## Engineering standards

**Language and tooling**

- Rust stable, edition 2024. One Cargo workspace.
- `thiserror` for library error types; `anyhow` only in binaries.
- `tracing` for all logging. No `println!` outside the CLI's user-facing output.
- `serde` for serialisation. `ulid` for IDs. `chrono` for time — never `jiff`, and never both (DECISIONS B-1).
- `cargo deny check` in CI for licences and advisories.

**Structure**

- `keel-core` never opens a network socket, never knows about MCP, never reads env vars. Everything it needs is passed in. This boundary is what makes the CLI, daemon, and future surfaces cheap — protect it.
- Storage access goes through three traits, named here since the spec only names the first: **`GraphStore`** (link traversal), **`DocumentStore`** (documents and blobs — revisions, embeddings, search), **`EntityStore`** (entity CRUD, links, events). No raw SQL outside their implementations. The traits are named for what they hold, never for what holds it — they came through the move from DuckDB-and-Lance to one SQLite file unchanged, which is the whole return on having drawn them in Phase 0.
- All graph traversal goes through `GraphStore`. Nobody hand-writes a recursive CTE at a call site. See "Graph direction" below for why.

**Errors**

- Errors carry context about *what the caller was trying to do*, not just what failed.
- Never swallow an error to keep going. If recovery is correct, log at `warn` with the reason.
- Validation errors returned over MCP must be actionable by a model reading them — say what was wrong and what would be valid.

**Testing**

Write tests as you go, not in a batch at the end of a phase. Required coverage:

- **Round-trip** for every entity type: create → read → update → archive.
- **Graph direction**: one test per relation in `product/SPEC.md` §3.3 asserting traversal in both directions. This is non-negotiable — see below.
- **Concurrency**: two simultaneous writers producing zero duplicates and zero lost updates.
- **Idempotency**: the same create called twice returns the same entity with `created: false`.
- **Optimistic concurrency**: a stale-version update is rejected and returns the current state.
- **Backup round-trip**: back up, wipe, restore, diff. Assert equality, don't eyeball it.
- **Snapshot tests** (`insta`) for MCP tool responses — they're an API contract, and drift should be visible in a diff.
- **Property tests** (`proptest`) for the graph traversal and the revision chain, where invariants matter more than examples.

**Git**

- Small, focused commits. One logical change each.
- Conventional commit prefixes: `feat:`, `fix:`, `refactor:`, `test:`, `docs:`, `chore:`.
- Short-lived branches off `main`, merged when green. Don't accumulate long-running branches — you're the only developer and they only create merge pain.
- Never commit secrets, `~/.keel` contents, or model weights.
- Never force-push `main`.

---

## Graph direction — read this before touching `links`

The first draft of the spec had **both** graph traversals inverted. This is the most dangerous bug class in the codebase, because an inverted traversal returns an empty result set that is indistinguishable from a legitimate "nothing is linked here." It fails silently, plausibly, and in a direction that makes the product look calm and correct while it quietly loses data.

Rules:

- `product/SPEC.md` §3.3 has the normative direction table. It is the only authority. Read it every time.
- `blocks` and `depends_on` are inverses. Only `blocks` is ever stored; `keel-core` swaps the endpoints on write. Never store both.
- Every relation gets an explicit test asserting what it returns traversing **outbound** and what it returns traversing **inbound**.
- Any query returning an empty graph result in development gets treated as a suspected direction bug until proven otherwise.

---

## Hard constraints

Violating these means rework, not a refactor:

1. **Everything that writes goes through `keel-core`'s write path.** That is the thing being protected, and it is worth saying in those words rather than as "no other process writes to the store" — six of the seven steps in a Keel write are validation, provenance, the event, the revision, the embedding and the index, none of which the database does for you, and a writer that goes round them produces rows that are poorer than every other row without anything failing.

   Only one process may hold the store open for writing at a time, and since B-60 that is enforced rather than asked for: an advisory lock, taken by the daemon for its lifetime and by a CLI command for its duration. DuckDB used to enforce it and SQLite in WAL mode does not, so for a while it was a convention — until a second daemon was started with `--home` forgotten and migrated the store under the one already serving it. Reading takes no lock, because looking at a busy store is when you most need to. `--force` skips it, for the wedged daemon that is the reason the flag exists.
2. **The mirror is one-directional, with no exceptions.** Nothing reads a generated file back into the store on its own. `keel import` exists for deliberate migrations and is run by a person. Any code that diffs mirror state against database state is a bug.
3. **Soft delete only, for anything that is a record.** Nothing is ever `DELETE`d — rows, links, notes. The one exception is a *derived index*, which is not a record: `fts_source` and `document_chunks` hold nothing that cannot be rebuilt from the revision they came from, and both are deleted when the thing they describe changes or is archived (B-55). The test that keeps the exception honest asserts a passage can always be recomputed byte-for-byte; if that ever fails, the carve-out is unsound and passages go back to being archived like everything else.
4. **No silent truncation.** Every list that can be cut reports that it was cut, with a total.
5. **`session_id` is caller-supplied.** The daemon never invents one.
6. **No new artifact types** without KB's agreement. Thirteen is the ceiling.
7. **The desktop app is read-only.** Claude and Keel are the only writers. No write endpoints on the daemon for it, no forms in it.

---

## Scale discipline

This is one user and a few thousand rows. There is nothing to optimise.

Do not add caches, queues, connection pools, sharding, background workers, or async where sync would do, unless a measurement says otherwise. If you want to add one, put the measurement in `product/DECISIONS.md` first.

Optimise instead for: correctness, clarity, and how pleasant the MCP surface is for a model to use. The last one is the actual product.

---

## Working with KB

- **He is not always around.** If you're blocked on a question, do the preparatory work you safely can, record the question in Keel with the specific options and your recommendation, and move to another task. Don't idle.
- **Ask about**: anything touching storage format, the MCP tool surface, phase order, or the decisions in `product/SPEC.md` §13.
- **Decide yourself about**: anything reversible — naming, internal structure, library choices within the constraints above, test approach. Record it in `product/DECISIONS.md` with one line of reasoning.
- **Don't ask permission to do the obvious.** If a task in `product/STATUS.md` is unambiguous, do it.
- **Tell him when something is wrong.** If the spec is unbuildable, if a phase is misordered, if a decision turns out to be a mistake — say so plainly and early. He wants to know at the point it's cheap.

---

## Anti-patterns

Things that look like progress and aren't:

- Writing the desktop app because the daemon is hard.
- Adding an artifact type because the modelling is awkward — it's almost always a field or a `kind` value.
- Building the GitHub integration before the surface it decorates is finished, because it's more fun.
- Expanding the MCP surface past **thirteen** tools. More tools means worse model selection, not more capability. Nine was the cap, then ten when `keel_note` earned a slot, then thirteen when the three work verbs did (TQ-31). Each rise needed KB's agreement and an argument at least as good as the last — both are in the doc comment on `tools::all()`.
- Refactoring for elegance while the tracker says something is blocked.
- Hand-editing a generated file and committing it with `--no-verify`. The pre-commit check exists to stop exactly this, the next `keel generate` reverts it, and the reasoning in it is lost.
- Marking a task done because the code exists but the tests don't.
