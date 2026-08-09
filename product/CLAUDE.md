# Keel — standing instructions

This file is loaded automatically into every Claude Code session in this repo. It is the contract. `product/HANDOFF.md` is orientation you read once; this is what you follow every time.

---

## Session ritual

**At the start of every session, in this order:**

1. Read `product/STATUS.md`. It tells you the current phase, what's in progress, and what's blocked.
2. Read `product/QUESTIONS.md`. Do not re-decide anything already listed there. Items in the "Needs KB" table marked `BLOCKED — needs KB` halt work on anything that depends on them; items marked `open` mean ask when convenient and keep going. Items in "Provisionally resolved" are decided — build on the stated assumption.
3. `git log --oneline -15` — see what the last session actually did.
4. State in one line what you're picking up before you touch anything.

**At the end of every session, without exception:**

1. Update `product/STATUS.md` — task statuses, the changelog entry, and the "next up" line.
2. Add any decisions you made to `product/DECISIONS.md`.
3. Add any new unknowns to `product/QUESTIONS.md`.
4. Commit. Never leave the tree dirty for the next session.
5. Close with a two-line summary: what landed, what's next.

**If you run out of context mid-task**, update `product/STATUS.md` first with enough detail for a fresh session to resume, then stop. An accurate tracker is more valuable than one more file edited.

---

## Tracker discipline

KB's primary window into this project is `product/STATUS.md`. If it is stale, he is blind. Treat it as a deliverable, not bookkeeping.

**Rules:**

- Move a task to `in_progress` **before** starting it, not after.
- A task is `done` only when it meets the definition of done below. Not when the code is written.
- If a task turns out to be bigger than one task, split it and record the split. Don't silently expand scope.
- If you're blocked, mark it `blocked` with the reason on the same line. Never leave something `in_progress` across sessions without a note.
- Every session appends one changelog entry, even a session that achieved nothing. Especially that one.
- Never delete a task. Mark it `dropped` with a reason.

**Task IDs are stable and never reused.** `P0-7` means the same thing forever.

Once Phase 1 exits, Keel becomes the tracker and `product/STATUS.md` is regenerated from it by `keel-cli render-status` (not by the §8 mirror, which is prose-only — see TQ-5). Until then, `product/STATUS.md` is authoritative and hand-maintained.

---

## Definition of done

A task is not done until all of these are true:

- [ ] Code compiles with zero warnings: `cargo clippy --workspace --all-targets -- -D warnings`
      *(was `--all-features`; dropped 2026-08-09 — no workspace crate declares a feature, so it changed nothing except forcing a second full build of the vendored DuckDB, which filled the disk. See DECISIONS B-11.)*
- [ ] Formatted: `cargo fmt --all --check`
- [ ] Tests written **and** passing — including at least one failure case, not only the happy path. The one exception is a *forward-looking* test for behaviour a later phase delivers: mark it `#[ignore = "unblocks in Phase N — see STATUS.md P0-x"]` so CI stays green and the intent stays visible. Never `#[ignore]` a test for behaviour the current phase is supposed to deliver.
- [ ] No `unwrap()`, `expect()`, or `panic!()` in library code (binaries and tests may, with a message)
- [ ] Public items in `keel-core` have doc comments explaining *why*, not restating the signature
- [ ] `product/STATUS.md` updated
- [ ] Committed with a message that explains the change, not the diff

If you can't tick all of them, the task is `in_progress`.

---

## Engineering standards

**Language and tooling**

- Rust stable, edition 2024. One Cargo workspace.
- `thiserror` for library error types; `anyhow` only in binaries.
- `tracing` for all logging. No `println!` outside the CLI's user-facing output.
- `serde` for serialisation. `ulid` for IDs. `jiff` or `chrono` for time — pick one, record it in `product/DECISIONS.md`, never mix.
- `cargo deny check` in CI for licences and advisories.

**Structure**

- `keel-core` never opens a network socket, never knows about MCP, never reads env vars. Everything it needs is passed in. This boundary is what makes the CLI, daemon, and future surfaces cheap — protect it.
- Storage access goes through three traits, named here since the spec only names the first: **`GraphStore`** (link traversal), **`DocumentStore`** (Lance documents and blobs — revisions, embeddings, search), **`EntityStore`** (DuckDB entity CRUD, links, events). No raw SQL outside their implementations.
- All graph traversal goes through `GraphStore`. Nobody hand-writes a recursive CTE at a call site. See "Graph direction" below for why.

**Errors**

- Errors carry context about *what the caller was trying to do*, not just what failed.
- Never swallow an error to keep going. If recovery is correct, log at `warn` with the reason.
- Validation errors returned over MCP must be actionable by a model reading them — say what was wrong and what would be valid.

**Testing**

Write tests as you go, not in a batch at the end of a phase. Required coverage:

- **Round-trip** for every entity type: create → read → update → archive.
- **Graph direction**: one test per relation in `product/SPEC.md` §3.3 asserting traversal in both directions. This is non-negotiable — see below.
- **Concurrency**: two simultaneous writers producing zero duplicates and zero lost updates. This is Phase 1's exit criterion; write it in Phase 0 as `#[ignore = "unblocks in Phase 1"]` so the target exists from the start without breaking CI, and un-ignore it when the daemon lands.
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

1. **The daemon owns the single write path.** No other process writes to DuckDB. Even after Quack lands, writes go through the daemon — six of the seven steps in a write have nothing to do with locking.
2. **The mirror is one-directional.** The only permitted read is the event-triggered hook in `product/SPEC.md` §8.1. Any code that diffs mirror state against database state is a bug.
3. **Soft delete only.** Nothing is ever `DELETE`d, links included.
4. **No silent truncation.** Every list that can be cut reports that it was cut, with a total.
5. **`session_id` is caller-supplied.** The daemon never invents one.
6. **No new artifact types** without KB's agreement. Thirteen is the ceiling.
7. **No UI before Phase 2 exits.**

---

## Scale discipline

This is one user and a few thousand rows. There is nothing to optimise.

Do not add caches, queues, connection pools, sharding, background workers, or async where sync would do, unless a measurement says otherwise. If you want to add one, put the measurement in `product/DECISIONS.md` first.

Optimise instead for: correctness, clarity, and how pleasant the MCP surface is for a model to use. The last one is the actual product.

---

## Working with KB

- **He is not always around.** If you're blocked on a question, do the preparatory work you safely can, write the question into `product/QUESTIONS.md` with the specific options and your recommendation, and move to another task. Don't idle.
- **Ask about**: anything touching storage format, the MCP tool surface, phase order, or the decisions in `product/SPEC.md` §13.
- **Decide yourself about**: anything reversible — naming, internal structure, library choices within the constraints above, test approach. Record it in `product/DECISIONS.md` with one line of reasoning.
- **Don't ask permission to do the obvious.** If a task in `product/STATUS.md` is unambiguous, do it.
- **Tell him when something is wrong.** If the spec is unbuildable, if a phase is misordered, if a decision turns out to be a mistake — say so plainly and early. He wants to know at the point it's cheap.

---

## Anti-patterns

Things that look like progress and aren't:

- Writing the desktop app because the daemon is hard.
- Adding an artifact type because the modelling is awkward — it's almost always a field or a `kind` value.
- Building the GitHub integration before Phase 1 exits, because it's more fun.
- Expanding the MCP surface past nine tools. More tools means worse model selection, not more capability.
- Refactoring for elegance while `product/STATUS.md` says something is blocked.
- Marking a task done because the code exists but the tests don't.
