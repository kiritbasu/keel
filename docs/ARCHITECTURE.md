# How Specline is built

Rust, one Cargo workspace, six crates, one SQLite file, one daemon that owns the
only write path. A React app compiled into the daemon binary. No services, no
containers, no account.

This document is about the shape of the thing. `product/SPEC.md` is the
normative specification and wins wherever the two disagree.

---

## The pieces

```
crates/specline-core/     domain types, storage, the graph, search,
                          generation, backup — everything that is not I/O
crates/specline-mcp/      the thirteen tools, the digest, protocol handling
crates/specline-daemon/   axum: the MCP endpoint, a local read API, the app
crates/specline-embed/    the local embedding model, kept out of the core
crates/specline-update/   version comparison, download, replace, roll back
crates/specline/          the CLI — and both shipped binaries
apps/desktop/             React. The read surface, and a person's own actions
```

Two binaries ship, `specline` and `specline-daemon`, and one package declares
both. That is not tidiness: the release tooling builds one installer per package
that owns binaries, and there is meant to be one installer. The daemon's code
still lives in `specline-daemon`; only its entry point moved.

### The boundary that matters

**`specline-core` never opens a network socket, never knows about MCP, and never
reads an environment variable.** Everything it needs is passed in. That is what
makes the CLI, the daemon and any future surface cheap to build — and it is the
one boundary worth defending, because every shortcut across it costs a surface
later.

Storage access goes through three traits:

- **`EntityStore`** — entity CRUD, links, events
- **`DocumentStore`** — documents and blobs: revisions, embeddings, search
- **`GraphStore`** — link traversal

No raw SQL exists outside their implementations. The traits are named for what
they hold and never for what holds it, which is why they came through a complete
change of storage engine unchanged.

---

## Storage

**One SQLite file.** Entities, links, the event log, document revisions, images
and vectors all live in the same database, in WAL mode. SQLite is compiled in,
so the binaries are self-contained — there is nothing to install alongside them.

Search is hybrid: FTS5 for keyword, `sqlite-vec` for similarity, fused by
reciprocal rank because BM25 scores and vector distances are not on comparable
scales. Keyword search covers every artifact whether or not the build can
embed.

It was two engines until Phase 9 — DuckDB for rows and Lance for documents, the
second attached into the first as a SQL namespace. That worked. It also cost a
22-minute build, a keyword index rebuilt wholesale on every write, and two
backup formats that had to be kept in step. `product/SPEC.md` D-1 records the
original reasoning and what overturned it.

### One writer

Only one process may hold the store open for writing at a time, enforced by an
advisory lock: taken by the daemon for its lifetime, and by a CLI command for
its duration. Reading takes no lock, because looking at a busy store is when you
most need to.

This used to be a convention. The previous engine enforced it by refusing a
second read-write connection outright; SQLite in WAL mode does not, and a second
daemon started with `--home` forgotten migrated the store underneath the one
already serving it. Now it is a lock, and `--force` is the deliberate escape.

### Everything that writes goes through one path

A Specline write is seven steps: validate, record provenance, append the event,
append the revision, embed it, index it, commit. Six of those are things the
database will not do for you. A writer that goes around the write path produces
rows that are poorer than every other row without anything appearing to fail —
which is why the rule is about the *path*, not about the process.

### Soft delete

Nothing that is a record is ever deleted — not rows, not links, not notes.
Archiving hides it and keeps it on disk.

The single exception is a derived index. `fts_source` and `document_chunks` hold
nothing that cannot be recomputed from the revision they came from, so both are
deleted when the thing they describe changes or is archived. A test keeps that
carve-out honest by asserting a passage can always be recomputed byte for byte.

---

## The model

Thirteen artifact types, and the ceiling is deliberate:

**project**, **milestone**, **task**, **spec**, **decision**, **question**,
**term**, **feedback**, **design**, **environment**, **metric**,
**metric observation**, **artifact**.

"We need a new type for this" is nearly always a field or a `kind` value in
disguise. Adding a fourteenth needs an argument, not a pull request.

They are connected by a typed graph — a task `implements` a spec, a decision
`supersedes` an earlier one, a task `blocks` another — and the graph is what
makes "what is actually blocked" a query rather than a guess. There is no
`blocked` status for the same reason: being blocked is a fact about the graph,
and holding it as a status too meant two facts that had to agree and did not.

### Graph direction is the dangerous part

An inverted traversal returns an empty result set that is indistinguishable from
a legitimate "nothing is linked here". It fails silently, plausibly, and in a
direction that makes the product look calm while it quietly loses data. The
first draft of the spec had *both* traversals inverted.

So: `product/SPEC.md` §3.3 holds the normative direction table and is the only
authority. `blocks` and `depends_on` are inverses, only `blocks` is ever stored,
and `specline-core` swaps the endpoints on write. Every relation has an explicit
test asserting what it returns outbound and what it returns inbound.

### Provenance

Every change is an event carrying an author, a surface and the session that made
it, so "who changed this, and in which conversation" is always answerable.

`session_id` is caller-supplied and **the daemon never invents one**. A
stateless transport has no session to borrow, so provenance is cooperative:
omitting it still writes, but the write is attributed only to some Claude
session.

Surfaces are `chat`, `cowork`, `code`, `ui` and `cli`. That is what makes
"somebody clicked it" distinguishable from "a page did it".

---

## The daemon

One axum server on `127.0.0.1:7654`. It serves three things:

- **The MCP endpoint** at `/mcp`, over streamable HTTP.
- **A local read API** under `/api`, which the app uses.
- **The app itself**, compiled into the binary. No Node, no second process.

It binds loopback and nothing else. There is no account, no cloud and no
telemetry.

**One request leaves your machine**, and it is worth naming rather than leaving
to be discovered: the daemon fetches the latest release manifest once an hour so
it can tell you a new version exists. It sends nothing from your store — no
project names, no counts, no identifier. `SPECLINE_AUTO_UPDATE=0` turns it off,
and with it off Specline makes no network requests at all.

---

## The MCP surface

Thirteen tools, which is the product:

| | |
|---|---|
| `specline_context` | The digest. What this project is, right now |
| `specline_search` | Hybrid search across everything |
| `specline_get` | Fetch by id, with graph traversal and revision diffs |
| `specline_projects` | What projects exist |
| `specline_activity` | What changed, and who changed it |
| `specline_create` | Create any of the thirteen types |
| `specline_update` | Fields around a document: title, status, kind |
| `specline_write_doc` | Append a revision of a prose body |
| `specline_note` | Append to a row's running commentary |
| `specline_link` | Draw an edge |
| `specline_ready` | What to work on next |
| `specline_claim` | Take a task |
| `specline_close` | Close one, with a reason and evidence |

Thirteen rather than forty because a model picks the right tool from a short
list and the wrong one from a long list. The cap was nine, then ten when
`specline_note` earned a slot, then thirteen when the three work verbs did. Each
rise needed an argument at least as good as the last, and they are recorded on
the doc comment on `tools::all()`.

How pleasant this surface is for a model to use is the actual product. Optimise
for that, and for correctness and clarity — not for throughput. This is one user
and a few thousand rows; there is nothing to make faster.

---

## Generation is one-directional

Specline is the source of truth. The markdown in your repository is an output.

`specline generate <project>` writes the adopted prose files at their recorded
paths, the `.specline/` mirror, and the tracker. Every generated file carries a
banner saying so.

**Nothing reads a generated file back into the store on its own.** `specline
import` exists for deliberate migrations and is run by a person. Any code that
diffs mirror state against database state is a bug.

There was once a hook that claimed to turn a hand edit into an attributed
revision. It never worked — it called a command that had been renamed out from
under it and swallowed the failure — so every edit it claimed to capture was
silently gone. It was deleted rather than repaired, because a safety mechanism
that quietly does nothing is worse than none: it gets relied upon. A pre-commit
check refuses the commit instead, which is loud and cannot be wrong about what
it did.

---

## What the app may write

The app is not read-only, and the line is not "reads versus writes".

**The interface may write what a person *does*.** Creating a task, commenting,
archiving, closing, moving a status or a priority — those are a person's own
actions, and they go through `specline-core`'s write path like everything else,
attributed `actor: human`, `surface: ui`.

**Authoring is the half it does not do.** The body of a spec, a decision or a
question is written by Claude in the conversation where the thinking happened.
That is not squeamishness about forms: the reasoning *is* the product. Specline
exists because why-this-and-not-that is the part that normally evaporates, and a
person typing into a textarea produces a tracker with an AI feature attached —
which is the thing this is trying not to be.

The line is **capture versus authoring**, and it is checkable: an endpoint that
accepts a document revision is on the wrong side of it.

---

## The one feature flag

`embeddings` is declared by `specline` and `specline-daemon`, on by default, and
it is the only feature in the workspace.

It exists because it decides whether a platform can be built at all. The ONNX
runtime the embedding model needs has no prebuilt Intel macOS library and wants
a newer glibc than the Linux build is pinned to, so two of three release targets
could not link while it was in the graph.

Released binaries are therefore built without it and cannot do semantic search.
Keyword search covers every artifact either way, and `specline doctor` and
`/api/health` both report which build you are running — a version number cannot
tell you.

Building from source gets you embeddings wherever it links.

---

## Working on it

```bash
cargo test --workspace
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo fmt --all --check
```

And the configuration two of the three released platforms actually ship:

```bash
cargo clippy --workspace --exclude specline-embed --all-targets --no-default-features -- -D warnings
```

`--exclude specline-embed` is not optional there. That crate is a workspace
member, so `--workspace` builds it whatever features anything asked for — and
building it *is* building the ONNX runtime. Without the exclusion, the
no-embeddings run links the very thing it exists to prove absent.

`rust-toolchain.toml` pins the compiler. A Homebrew `cargo` earlier on `PATH`
ignores it entirely, so the checks can pass against a different compiler from
the one CI uses. Either put `~/.cargo/bin` first, or run them through `rustup
run`.

To work on the interface, `npm run dev --prefix apps/desktop` gives the usual
hot reload against the same daemon.
