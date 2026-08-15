# Keel — Decision log

<!-- keel:generated decisions prj_01KZKMPVHJNCCQH3JQNAXJJ03M -->
> Generated from the decision rows — edits here are not saved.

Every decision made while building, with the reasoning and what was rejected. In six months nobody will remember why a library was chosen or an approach abandoned, and one line written now saves an hour of archaeology later.

`B-12` is a real identifier, not a convention: it resolves to a row, `keel_get KEEL-B12` returns it, and `fsck` checks that citations of it point at something. It was prose until 2026-08-10, which is why every `B-n` citation in this repository was unverifiable until then.

## Index

| | Decision | Status |
|---|---|---|
| B-1 | [chrono for time, not jiff](#b-1) | `accepted` |
| B-2 | [All Lance access goes through the DuckDB extension](#b-2) | `accepted` |
| B-3 | [Bundled DuckDB is a feature, not a requirement](#b-3) | `accepted` |
| B-4 | [No vector or FTS index on the Lance dataset initially — brute-force scan](#b-4) | `accepted` |
| B-5 | [unwrap/expect/panic/todo/unimplemented are workspace clippy lints, promoted to errors by CI](#b-5) | `accepted` |
| B-6 | [missing_docs is a workspace lint, not just a keel-core convention](#b-6) | `accepted` |
| B-7 | [Build scope is Phases 0–3; git stays local; session_id is skill-minted](#b-7) | `accepted` |
| B-8 | [Surface carries five values, not four: chat, cowork, code, ui, cli](#b-8) | `accepted` |
| B-9 | [ULIDs are minted from a monotonic generator](#b-9) | `accepted` |
| B-10 | [Every table gets idempotency_key, not just tasks](#b-10) | `accepted` |
| B-11 | [Dev builds use line-tables-only debug info, and the clippy gate drops --all-features](#b-11) | `accepted` |
| B-12 | [BM25 moves from Lance to DuckDB](#b-12) | `accepted` |
| B-13 | [Tool responses lift version to the top of the entity](#b-13) | `accepted` |
| B-14 | [The desktop app hand-writes its components](#b-14) | `accepted` |
| B-15 | [Keel's local REST API has more endpoints than the MCP surface has tools](#b-15) | `accepted` |
| B-16 | [Event summaries name artifacts, not ids](#b-16) | `accepted` |
| B-17 | [Serve MCP 2025-11-25 alongside 2026-07-28](#b-17) | `accepted` |
| B-18 | [keel import is a bridge, not a migration: re-importable and content-addressed](#b-18) | `accepted` |
| B-19 | [The document reader renders markdown with react-markdown, mapping every element by hand](#b-19) | `accepted` |
| B-20 | [Keel is the source of truth; every product/*.md is generated.](#b-20) | `accepted` |
| B-21 | [Generation runs inside the daemon, exposed as POST /api/generate.](#b-21) | `accepted` |
| B-22 | [projects gets a nullable status_path; a path claimed twice is reported, not resolved](#b-22) | `accepted` |
| B-23 | [keel_context answers "what next" with named, ranked tasks](#b-23) | `proposed` |
| B-24 | [A task marked blocked with no blocks edge is reported as a data problem, not ranked](#b-24) | `superseded` |
| B-25 | ["Waiting on a human decision" is the decision-needed label, not a new task kind](#b-25) | `proposed` |
| B-26 | [Fixture links are addressed by name, never by position](#b-26) | `accepted` |
| B-27 | [Outside panel: the gate measurement was invalid and the file plan is rejected](#b-27) | `proposed` |
| B-28 | [s2, s7 and s9 are genuine misses, not L0](#b-28) | `accepted` |
| B-29 | [Phase 2 closed on 18 of 20](#b-29) | `accepted` |
| B-30 | [The p0 wedge was SIGKILL mid-write, not the FTS index](#b-30) | `accepted` |
| B-31 | [restore now re-establishes the store's git repository](#b-31) | `accepted` |
| B-32 | [KB confirmed: idempotency keys stay on all thirteen tables](#b-32) | `accepted` |
| B-33 | [KB confirmed: BM25 stays in DuckDB, Lance does vectors only](#b-33) | `accepted` |
| B-34 | [The desktop app routes on the hash, and the router is hand-written](#b-34) | `accepted` |
| B-35 | [Eight named type sizes in two scales, not eleven anonymous ones](#b-35) | `accepted` |
| B-36 | [The light scheme overrides :root, not a second @theme block](#b-36) | `accepted` |
| B-37 | [The desktop app gets Vitest and Testing Library](#b-37) | `accepted` |
| B-38 | [Graph traversal carries the neighbour's label](#b-38) | `accepted` |
| B-39 | [The Tauri shell is suspended; the web build is the surface](#b-39) | `accepted` |
| B-40 | [Readable identifiers are composed, never stored](#b-40) | `accepted` |
| B-41 | [KB confirmed: a task holds a list of external links](#b-41) | `accepted` |
| B-42 | [KB confirmed: blocked is derived from the links, not a status](#b-42) | `accepted` |
| B-43 | [An accepted decision can be corrected; the revision chain is the guard](#b-43) | `proposed` |
| B-44 | [A missing readable number is read as unassigned, not as an error](#b-44) | `proposed` |
| B-45 | [Every milestone carries a plain-English explainer, required at creation](#b-45) | `accepted` |
| B-46 | [The plain-English rule covers every prose field, not just milestone summaries](#b-46) | `accepted` |
| B-47 | [The close reason is a column, and closing is checked on the transition](#b-47) | `accepted` |
| B-48 | [A claim is optimistic concurrency, not a lock](#b-48) | `accepted` |
| B-49 | [Reading an image off the disk is a field, not a fourteenth tool](#b-49) | `accepted` |
| B-50 | [A glossary term can declare which type it is a spelling of](#b-50) | `accepted` |
| B-51 | [Phase 9 runs before Phase 10, and DuckDB and Lance come out of the tree entirely](#b-51) | `accepted` |
| B-52 | [Taking the payload out of a tool result is one named function, not two lines](#b-52) | `accepted` |
| B-53 | [The write-path atomicity fix: &Connection primitives, transaction-of-one, one typed composite on Store](#b-53) | `proposed` |
| B-54 | [The fixture corpus stays compiled in, ungated](#b-54) | `accepted` |
| B-55 | [Documents are embedded as passages, and the passage table is an index rather than a record](#b-55) | `accepted` |
| B-56 | [Superseded decisions stay in search results and carry a label saying what replaced them](#b-56) | `accepted` |
| B-57 | [A phase's state is derived; only shipped, cut and paused are declared](#b-57) | `accepted` |
| B-58 | [Leaked test stores get a working sweeper, not a redirected TMPDIR](#b-58) | `accepted` |
| B-59 | [A changed model is an ordinary re-embed, because search refuses to mix models at all](#b-59) | `accepted` |
| B-60 | [Say what the write path actually protects, and put an advisory lock on the store](#b-60) | `accepted` |
| B-61 | [Explicit writes only, and adopting an existing project is a two-layer backfill](#b-61) | `accepted` |
| B-62 | [Spec decisions that outlive their reasoning are annotated, not rewritten](#b-62) | `accepted` |
| B-63 | [The keel-github stub comes out of the tree; SPEC §1.1 stays as the intended layout](#b-63) | `accepted` |
| B-64 | [The write-ahead log stays on SQLite's defaults, unwatched](#b-64) | `accepted` |
| B-65 | [Keel ships as a product: Apache-2.0, 0.x, and the Claude Code plugin as the front door](#b-65) | `accepted` |
| B-66 | [Updates apply themselves when compatible, and stop and ask across a schema change](#b-66) | `accepted` |
| B-67 | [Phase 10 runs after Phase 11, drops Windows, and plans for a release cadence that starts fast and slows down](#b-67) | `accepted` |
| B-68 | [Mutation testing comes out of CI until there is traction worth protecting](#b-68) | `accepted` |
| B-69 | [Serving a read-only page does not touch hard constraint 7, the repo stays private for now, and the package becomes keel](#b-69) | `accepted` |
| B-70 | [One package owns both binaries, because dist builds one installer per package](#b-70) | `accepted` |
| B-71 | [The installer refuses a download it cannot verify, rather than skipping the check](#b-71) | `accepted` |
| B-72 | [The repository stays private and macOS builds run on a self-hosted runner](#b-72) | `accepted` |

## Reversals

**B-24 — A task marked blocked with no blocks edge is reported as a data problem, not ranked**

**Reversed 2026-08-10.** `blocked` is no longer a status — the enum value was removed in migration 8 and blocked is derived from the `blocks` edges, with `blocked_tasks()` as the single definition every surface reads.

This was the right fix for the wrong shape. Reconciling two sources of truth is work that only exists because there are two, and the reconciliation itself was load-bearing, which is what made it look like a feature rather than a symptom. Removing the status removed the disagreement instead of reporting it. KB's call, since it cost a forward-only migration and a visible behaviour change.

---

## Decisions

### B-1 — chrono for time, not jiff

`accepted` · decided 2026-08-09 · `dec_01KZKMPVPM94XEZGCSFS73XQ9T`

#### Decision

chrono for time, not jiff.

#### Reasoning

`duckdb-rs` ships a first-class `chrono` feature with `ToSql`/`FromSql` for `TIMESTAMP`; there is no `jiff` feature. Choosing `jiff` would mean a hand-written conversion shim at every storage boundary — the exact place a timezone bug would hide — for no domain benefit. Recorded here because `product/CLAUDE.md` requires picking one and never mixing.

#### Reversible?

Yes, painfully.


### B-2 — All Lance access goes through the DuckDB extension

`accepted` · decided 2026-08-09 · `dec_01KZKMPVPZ81H8PHKF8RHZK13R`

#### Decision

All Lance access goes through the DuckDB lance extension. The lance and lancedb Rust crates are not dependencies.

#### Reasoning

Verified empirically (see P0-2 table): `ATTACH … (TYPE lance)` gives full `SELECT`/`INSERT`/`UPDATE` over Lance datasets, and the three search functions work. Using the extension means one connection, one SQL surface, one transaction story — and it drops `lance` v10 + `arrow` v59 from the build entirely. Rejected: the native crate, which would have meant marshalling Arrow record batches by hand and keeping two Lance versions in step.

#### Consequences

DocumentStore is a trait precisely so this can be swapped.

#### Reversible?

Yes — `DocumentStore` is a trait precisely so this can be swapped.


### B-3 — Bundled DuckDB is a feature, not a requirement

`accepted` · decided 2026-08-09 · `dec_01KZKMPVQD8N0ZYBZZBMWTCP04`

#### Context

Compiling DuckDB from source costs about ten minutes on a cold build.

#### Decision

bundled is the default, but it is now a feature — --no-default-features links a system libduckdb instead.

#### Reasoning

Originally justified as "the binary can never disagree with the extension versions it loads", which **overstated it**: `INSTALL lance` re-fetches for whatever version is running, so a system library self-heals given network. The genuine reasons are narrower — a self-contained binary that keeps working after `brew upgrade duckdb`, and a build that needs no setup on a fresh machine. Neither justifies a ten-minute wait on a machine that already has the right library, so it is a feature now. **Verified both ways:** system-linked builds the workspace in 54s versus roughly ten minutes, and all 264 tests pass against Homebrew's libduckdb 1.5.5, Lance extension included. The default stays `bundled` because the *installed* binary should not break when Homebrew moves underneath it; the fast path is for development.

#### Consequences

System-linked builds the workspace in 54s versus roughly ten minutes, with all tests passing either way.

#### Reversible?

Yes.


### B-4 — No vector or FTS index on the Lance dataset initially — brute-force scan

`accepted` · decided 2026-08-09 · `dec_01KZKWMSX25E73XSGB9Q9A0P5W`

#### Decision

No vector or FTS index on the Lance dataset initially — brute-force scan.

#### Reasoning

Verified that `lance_fts`, `lance_vector_search` and `lance_hybrid_search` all return correct results with no index present. At a few thousand documents an index is pure cost. Per the scale-discipline rule, a measurement comes before an index.

#### Reversible?

Yes.


### B-5 — unwrap/expect/panic/todo/unimplemented are workspace clippy lints, promoted to errors by CI

`accepted` · decided 2026-08-09 · `dec_01KZKWMSYT6WTETRJ6DF82A42E`

#### Decision

unwrap/expect/panic/todo/unimplemented are workspace clippy lints at warn, promoted to errors by CI's -D warnings.

#### Reasoning

The definition of done forbids these in library code. Encoding it as a lint means CI catches it; leaving it to review discipline means it lands. Tests and binaries opt out locally with `#[allow]` where genuinely warranted.

#### Reversible?

Yes.


### B-6 — missing_docs is a workspace lint, not just a keel-core convention

`accepted` · decided 2026-08-09 · `dec_01KZKWMT0JWXM2JGX7MZ0QZ7DV`

#### Decision

missing_docs is a workspace lint, not just a keel-core convention.

#### Reasoning

The contract only requires doc comments on `keel-core` public items, but scoping the lint per-crate is more machinery than it saves, and documenting the daemon's public surface costs little.

#### Reversible?

Yes.


### B-7 — Build scope is Phases 0–3; git stays local; session_id is skill-minted

`accepted` · decided 2026-08-09 · `dec_01KZKWMTFN212CPD921AY3PX6D`

#### Decision

Build scope this stretch is Phases 0–3; git stays local with no remote; session_id is a skill-minted per-conversation ULID; unverifiable human phase gates get an automated proxy and an honest note in product/STATUS.md.

#### Reasoning

All four confirmed directly by KB on 2026-08-09 before he went away. Q-8 in `product/QUESTIONS.md` moves from `open` to answered on the strength of the third.

#### Reversible?

N/a — KB's call.


### B-8 — Surface carries five values, not four: chat, cowork, code, ui, cli

`accepted` · decided 2026-08-09 · `dec_01KZKWMT28K0HMJ1Y5JQ16TT8T`

#### Decision

Surface carries five values, not four: chat \| cowork \| code \| ui \| cli.

#### Reasoning

SPEC §3.1's audit-block comment lists four; §6.5 separately names `cli` as a fixed sentinel for the command line. The two passages disagree and something had to give. Five is the reconciliation — `keel-cli` writes fixtures and restores backups, and those writes need an honest surface rather than a borrowed `ui`. The column is a bare `VARCHAR` with no check constraint, so this costs nothing at the storage layer. Raised with KB as TQ-8 rather than treated as settled.

#### Reversible?

Yes.


### B-9 — ULIDs are minted from a monotonic generator

`accepted` · decided 2026-08-09 · `dec_01KZKMPVQWSF1TN6TYEWQ3BJ61`

#### Context

`Ulid::new()` re-randomises its low 80 bits on every call, so two ids created in the same millisecond sort arbitrarily.

#### Decision

All ULIDs are minted from a single process-wide *monotonic* generator, never Ulid::new().

#### Reasoning

Found by a test, not by reading: `Ulid::new()` re-randomises its low 80 bits every call, so two ids created in the same millisecond sort arbitrarily. SPEC §3.4 rests on ULID order *being* chronological order so that "changed since T" is a range scan over `events.id` — and a burst of writes inside one millisecond is an agent's normal behaviour, not an edge case. Non-monotonic ids would make an event-cursor query silently skip or repeat rows, which is the same class of quiet-wrong-answer bug as an inverted graph traversal. Rejected: ordering every query by `(created_at, id)`, which pushes the problem to every call site instead of solving it once.

#### Consequences

Found by a test, not by reading.

#### Reversible?

Yes.


### B-10 — Every table gets idempotency_key, not just tasks

`accepted` · decided 2026-08-09 · `dec_01KZKMPVR92GNRQXTE8836ZD1E`

#### Context

SPEC §7.2 and REQ-7 say every create is idempotent; §3.2 gives the column only to tasks.

#### Decision

Every table gets idempotency_key, not just tasks.

#### Reasoning

SPEC §7.2 and PRD REQ-7 say *every* create is idempotent, but §3.2 only gives the column to `tasks`. Honouring the requirement means honouring it everywhere; the alternative silently drops idempotency for twelve of thirteen types, including `projects` — the one type where duplicates are called out as the failure that ruins the aggregate view (UC-8, REQ-8). Marked `PROVISIONAL` and raised as TQ-9, because adding a column is a storage-format change and those are KB's call.

#### Consequences

Marked PROVISIONAL; raised as TQ-9 because adding a column is a storage-format change and those are KB's call.

#### Reversible?

Expensive — it is a schema column.


### B-11 — Dev builds use line-tables-only debug info, and the clippy gate drops --all-features

`accepted` · decided 2026-08-09 · `dec_01KZKWMT3ZRNB06RMYBSTAKDV6`

#### Decision

Dev builds use debug = "line-tables-only" and debug = false for dependencies; the clippy gate drops --all-features.

#### Reasoning

The vendored DuckDB's full debug info is enormous: `target/` reached **19 GB** and filled KB's disk mid-session (`ranlib: errno=28`). `--all-features` made it worse by building DuckDB a second time under a different feature set, while changing nothing — no workspace crate declares a feature. Line tables keep file-and-line in every backtrace, which is the part that matters; what is lost is stepping through DuckDB's C++ internals, which this project does not do. `product/CLAUDE.md`'s definition of done was amended to match. **Worth KB knowing separately: the machine is at 95% disk (327 GB of 373 GB used) independent of this repo.**

#### Reversible?

Yes.


### B-12 — BM25 moves from Lance to DuckDB

`accepted` · decided 2026-08-09 · `dec_01KZKMPVRTXA717P2854N17HQ5`

#### Context

SPEC §5 put both halves of hybrid search inside lance_hybrid_search.

#### Decision

BM25 moves from Lance to DuckDB. lance_hybrid_search is not used; the Lance index does vectors only.

#### Reasoning

SPEC §5 put both halves of hybrid search inside `lance_hybrid_search`. Its keyword half could not be characterised. On an un-indexed dataset, multi-term queries match inconsistently: `"onboarding metering"` returns a document containing only *metering*, while `"onboarding slow"` returns **nothing** despite a document containing *onboarding*. A third query returned an unrelated document with a score identical to an unrelated query's. The extension's documentation shows only single-word examples (`'puppy'`) and documents no way to build the index that would presumably fix this. A search returning plausible-but-wrong results is the same failure class as an inverted graph traversal, so it gets the same answer: put it where the semantics are known. DuckDB's `fts` extension is a real BM25 index with documented behaviour, and the index now covers *every* searchable artifact — prose titles and bodies joined in from Lance — so a spec and a task compete in one ranking instead of two. Lance keeps what it is uniquely for: the vector index and the multimodal blobs. `keel-core` was already doing the cross-index RRF fusion, so nothing else moved. **This is a §5 design change and is flagged to KB as TQ-10.**

#### Consequences

The DuckDB index now covers prose too, so a spec and a task compete in one ranking. Flagged as TQ-10.

#### Reversible?

Yes — it is one module, and `DocumentStore` is a trait.


### B-13 — Tool responses lift version to the top of the entity

`accepted` · decided 2026-08-09 · `dec_01KZKMPVSRVWSF4N42E9Y1M7A1`

#### Context

`version` lives inside the audit block in the domain model.

#### Decision

Tool responses lift version (and archived) to the top of the entity, alongside the nested audit block.

#### Reasoning

`version` lives inside `audit` in the domain model, which is right there and wrong on the wire: `keel_update` documents a `version` argument, so an agent that has just read an entity should be able to copy the field of that name straight across. Making it hunt inside `audit` is the papercut that becomes a 409 and a confused retry. Found by writing the UC-3 handoff test as an agent would actually do it. The audit block is untouched — this adds a field rather than moving one.

#### Consequences

Found by writing the UC-3 test the way an agent would actually do it.

#### Reversible?

Yes.


### B-14 — The desktop app hand-writes its components

`accepted` · decided 2026-08-09 · `dec_01KZKMPVTSVQB53R5AGXMB5WZ5`

#### Decision

The desktop app hand-writes its components rather than using shadcn/ui's generator.

#### Reasoning

SPEC §10 names shadcn/ui. What a read-only, seven-screen app actually needs from it is a card, a badge, a status colour and an empty state — four small components. Running the generator to obtain those pulls in Radix primitives and a registry dependency for a surface with no dialogs, popovers or focus traps to manage. The conventions are kept: one component per concern, styling through class names, no theme provider, Tailwind 4 tokens in one place. Total frontend dependency footprint is 81 packages and a 227 KB bundle. Revisit the moment the app grows anything genuinely interactive.

#### Consequences

81 packages, a 227 KB bundle. Revisit the moment the app grows anything genuinely interactive.

#### Reversible?

Yes.


### B-15 — Keel's local REST API has more endpoints than the MCP surface has tools

`accepted` · decided 2026-08-09 · `dec_01KZKMPVVAC6EZA35F1E87SC0C`

#### Decision

Keel's local REST API has more endpoints than the MCP surface has tools.

#### Reasoning

The nine-tool ceiling exists because a *model* chooses worse among forty tools than among nine (SPEC §6.1). That reasoning does not transfer to a UI, which knows exactly what it wants and would otherwise fetch everything and filter client-side. `/api/entities`, `/api/document/{id}` and `/api/graph/{id}` are UI-facing and thin. The MCP surface is untouched at nine.

#### Reversible?

Yes.


### B-16 — Event summaries name artifacts, not ids

`accepted` · decided 2026-08-09 · `dec_01KZKMPVVY4DAPXQD0H99HB27C`

#### Context

"linked tsk_01KZK163THQG7 references fbk_01KZK16505G3J" is not a sentence.

#### Decision

Event summaries name artifacts, not ids.

#### Reasoning

Found by looking at the finished Home screen: "linked tsk_01KZK163THQG7DPQWGV4C9FFZ7 references fbk_01KZK16505G3JJ2M7Z27JTP1WJ" is not a sentence a human can read, and the activity feed and Sunday-review digest are the two places that text is actually shown. Now "“Invoices round to the wrong cent” references “Invoices do not match our own metering”". The ids are still on the event for anything that needs them.

#### Reversible?

Yes.


### B-17 — Serve MCP 2025-11-25 alongside 2026-07-28

`accepted` · decided 2026-08-09 · `dec_01KZKMPVS9XTWRN7303BPY0F18`

#### Context

Claude Code 2.1.185 opens with the legacy initialize handshake and declares 2025-11-25. A daemon speaking only the current revision reported "Failed to connect".

#### Decision

The daemon serves 2025-11-25 as well as 2026-07-28.

#### Reasoning

Found the moment KB pointed a real client at it: `claude mcp list` said "Failed to connect". Captured the wire traffic — **Claude Code 2.1.185 opens with the legacy `initialize` handshake and declares `2025-11-25`**, and sends none of the mirrored headers the current revision requires. A daemon that speaks only 2026-07-28 is unusable with the client this entire product exists to serve, which would have made Phase 2's gate impossible to even attempt. The spec's backward-compatibility section makes this a MAY; here it is the difference between working and not. `initialize`, `notifications/initialized` and `ping` are answered; `Mcp-Method`/`Mcp-Name` are required only of a 2026-07-28 caller; `resultType` and `_meta.serverInfo` are sent only to clients whose revision defines them. 2025-06-18 and 2025-03-26 are served the same way — they differ from 2025-11-25 only in ways the tool surface never touches. **SPEC §6's opening line is now wrong** and is flagged as TQ-11.

#### Consequences

Mirrored headers are required only of a 2026-07-28 caller; resultType goes only to clients whose revision defines it. Flagged as TQ-11.

#### Reversible?

Yes.


### B-18 — keel import is a bridge, not a migration: re-importable and content-addressed

`accepted` · decided 2026-08-09 · `dec_01KZKWMT5SMKXQ07NKBKT87SXC`

#### Decision

keel import is a bridge, not a migration: re-importable, content-addressed, and it leaves the repo copy alone.

#### Reasoning

KB asked whether whole specs can live in Keel and be read in the app. They can — a 51 KB SPEC.md round-trips byte-identical, stays searchable and diffs between revisions — so the only real question was how the repo files and the store stay in step. Import resolves an existing artifact by title before creating one, and `write_revision` is content-addressed, so re-running it on an unchanged file appends nothing. That makes it safe in a script or a hook, and it means `product/*.md` can stay authoritative for exactly as long as KB wants without the two copies drifting. Rejected: a one-way "move the files in and delete them", which forecloses a decision that is his (see TQ-13).

#### Reversible?

Yes.


### B-19 — The document reader renders markdown with react-markdown, mapping every element by hand

`accepted` · decided 2026-08-09 · `dec_01KZKWMT7GFNZBYEQBV44NPY4R`

#### Decision

The document reader renders markdown with react-markdown + remark-gfm, mapping every element by hand.

#### Reasoning

The bodies were being shown as preformatted text, which made a real spec unreadable — the point of storing it. `react-markdown` over a string-to-HTML library because it does **not** render raw HTML by default: document bodies are written by a model, arrive from the store, and are displayed in an app served from the same origin as the daemon, so an injected `<script>` would be same-origin. Elements are mapped explicitly rather than pulling in a typography plugin, on the same reasoning as B-14. Tables get their own scroll container — the decision log and status tracker are almost entirely tables and would otherwise force the page wide.

#### Reversible?

Yes.


### B-20 — Keel is the source of truth; every product/*.md is generated.

`accepted` · decided 2026-08-09 · `dec_01KZKWMT9M8EJQM7TJDZH8KX22`

#### Decision

Keel is the source of truth; every product/*.md is generated. A prose artifact records the repository file it *is*, as mirror_path, and generation writes its body there verbatim.

#### Reasoning

KB's call, made directly. The alternative that was running — repo files authoritative, `keel import` keeping Keel in step — worked, but it left two copies that agree only as long as someone remembers to run the import, and the failure mode is silent. Verbatim rather than re-rendered: these documents carry their own heading and front matter, and injecting a generated preamble would corrupt a file written to be read whole. The banner is an HTML comment for the same reason — invisible in every renderer, and harmless at the top of `product/CLAUDE.md`, which Claude Code loads verbatim on every session. Adopted files are excluded from the `.keel/` mirror, so no document has two homes.

#### Reversible?

Yes — the files are on disk and in git; deleting the `mirror_path` values reverts to the mirror's slugged layout.


### B-21 — Generation runs inside the daemon, exposed as POST /api/generate.

`accepted` · decided 2026-08-09 · `dec_01KZKWMTBYP6P7B8DQTB3586G9`

#### Decision

Generation runs inside the daemon, exposed as POST /api/generate. The CLI is a client.

#### Reasoning

Not a preference — a discovery. D-5 says non-daemon processes "connect read-only or go through the daemon's API", and **the read-only half does not exist**: DuckDB refuses a read-only connection while any process holds the write lock, so no second process can read the store while the daemon runs. Verified by implementing `open_read_only` and watching it fail with the same conflicting-lock error a writer gets; the code was reverted. Since the daemon is always running, "go through the API" is the only path, which resolves TQ-12 for every read-shaped command. `keel generate` falls back to opening the store directly only when no daemon answers, which is safe precisely because nothing else holds the lock then. **SPEC D-5's wording is now wrong** and is flagged as TQ-15.

#### Reversible?

Yes.


### B-22 — projects gets a nullable status_path; a path claimed twice is reported, not resolved

`accepted` · decided 2026-08-09 · `dec_01KZKWMTDQZPZJ46PCEWATF0XY`

#### Decision

projects gets a nullable status_path; a path claimed by both a document and the tracker is reported, not resolved.

#### Reasoning

The tracker is rendered from task and milestone rows, so no single artifact *is* `product/STATUS.md` the way the spec artifact is `product/SPEC.md` — the destination is a property of the project. Migration 4, additive and nullable. The collision case is real today: Keel's own `product/STATUS.md` is both an adopted prose document and the project's `status_path`. Rather than let whichever writer runs last win — which is how a file silently loses half its content — neither is written and the conflict is reported. The prose survives because it cannot be regenerated and the tracker can.

#### Reversible?

Expensive — it is a schema column, though a nullable additive one.


### B-23 — keel_context answers "what next" with named, ranked tasks

`proposed` · `dec_01KZPFPHCFPZ1X930DEC2ZRR7R`

#### Decision

keel_context answers "what next" with named, ranked tasks. Ready work is ordered by what it unblocks first, then priority.

#### Reasoning

KB, looking at the finished app: *"I don't understand what's next to build."* The digest was returning counts and a query to run — and the query returned nothing, because no `blocks` edge existed. Option (a) of TQ-16, chosen by KB. Ranking on the graph before the label is the part worth defending: a p1 that releases three tasks moves the project further than a p0 that releases none, and the count comes from edges a human already drew rather than a judgement this code invents. Three buckets, not one list — **ready**, **waiting on a human**, **blocked** — because a p0 decision nobody can start must not outrank work someone can. `ready` is capped at three and the truncation is reported: a ranked list of thirty is the same "you work it out" as a count. Rejected: a hand-written `next_action` field on the project, which is the STATUS.md problem again — right while maintained, silently stale after.

#### Reversible?

Yes — one module, and the old count-based advice is still there under "Also worth noticing".


### B-24 — A task marked blocked with no blocks edge is reported as a data problem, not ranked

`superseded` · `dec_01KZPFPPEMGCEB5HXXPF1RFWDC`

#### Decision

A task marked blocked with no blocks edge is reported as a data problem, not ranked.

#### Reasoning

Keel's own store was in exactly this state: three blocked tasks, zero edges. The tempting behaviour is to trust the status and hide the task; the honest one is to say the status has no referent, because otherwise the thing that made the board unreadable stays invisible. The desktop board and the digest share one ranking from `keel-core`, so they cannot word it differently or disagree.

#### Reversible?

Yes.

#### Superseded

**Reversed 2026-08-10.** `blocked` is no longer a status — the enum value was removed in migration 8 and blocked is derived from the `blocks` edges, with `blocked_tasks()` as the single definition every surface reads.

This was the right fix for the wrong shape. Reconciling two sources of truth is work that only exists because there are two, and the reconciliation itself was load-bearing, which is what made it look like a feature rather than a symptom. Removing the status removed the disagreement instead of reporting it. KB's call, since it cost a forward-only migration and a visible behaviour change.


### B-25 — "Waiting on a human decision" is the decision-needed label, not a new task kind

`proposed` · `dec_01KZPFPS0KK4YE59E3A8GJQ0VW`

#### Decision

"Waiting on a human decision" is the decision-needed label, not a new task kind or column.

#### Reasoning

The bootstrap already used the label, so the data existed. A new `TaskKind` would be a schema change to express something a label expresses, and `product/CLAUDE.md` is explicit that a new type or field is almost always the wrong answer to awkward modelling. The cost is that it is a convention: nothing enforces it, and a decision task without the label ranks as ordinary work.

#### Reversible?

Yes — trivially.


### B-26 — Fixture links are addressed by name, never by position

`accepted` · decided 2026-08-09 · `dec_01KZKMPVT876SD8CJJPGY9ZVXY`

#### Context

Two Harbour feedback items ended up linked to a Keel spec.

#### Decision

Look artifacts up by label; error if the label is missing.

#### Reasoning

The link section used positional indices, and appending rows near the top of each list shifted every index below. The edges silently rewired themselves and nothing complained, because a link to the wrong artifact is still a valid link.

#### Consequences

A renamed artifact now breaks the fixture loudly rather than quietly dropping an edge.


### B-27 — Outside panel: the gate measurement was invalid and the file plan is rejected

`proposed` · `dec_01KZMGPPJ0MM4VSGAP4KF724DQ`

Six-expert panel review with adversarial cross-examination, delivered as `product/WAY-FORWARD.md`, 2026-08-09.

**The headline finding, and it is correct.** The gate harness is headless single-turn — `claude -p ... </dev/null`, one prompt, one response, process exits. Five silent sessions ended with *"I'll hold off until you say go."* There was no "you" and no next turn. The write was not refused; it was scheduled for a turn the harness architecturally could not supply. I recorded that caveat for run 1 and then dropped it, and every strategic conclusion since — including "the premise may be dead" — was drawn as though 3/10 measured judgement.

**The statistical point is also right.** 9-of-10 at n=10 has a 95% CI of [0.555, 0.997]; two projects means effective n≈5. The gate cannot distinguish a 55% agent from a 100% agent. It was never a usable instrument.

**The cause I missed, sitting in the highest-traffic text in the system.** `keel_create`'s description ends with *"confirm with the human"* and *"worse than useless"*; `keel_projects` says *"ask the human before creating anything"*. Tool descriptions are the only text re-read every session in every environment — unlike a skill (never loaded, proven) or a hook preamble (weak directive force). The anti-write instruction is inside the write tool, and `requires_confirmation` is tool *output* arriving at decision time, which is the strongest channel that exists.

**My file-based plan is rejected, on grounds I should have seen.** `product/STATUS.md` is one spec artifact — the whole forty-row tracker. An agent adding a task row via the PostToolUse hook writes revision N+1 of that blob, creates zero task artifacts, and `generate --check` passes. That is bit-for-bit the incident that lost 16 of 28 questions, promoted to the default write path and executed on every edit. It also reduces surface coverage (chat and Cowork have no filesystem, no hook, no localhost daemon) and turns a silent non-write on those surfaces into silent data loss.

**Adopted from it:** collapse the four model-facing write tools into one `keel_record`; rewrite the description with "call this without asking" in the first three lines; teach reversibility through the write's own output; auto-create the first project for a directory; a deterministic Stop hook with no model call; and a `class` column making the prose-blob failure unrepresentable rather than merely detectable.

**Process rule adopted unanimously, and it indicts the build order:** no phase may be sequenced ahead of a phase that tests an assumption it depends on. 305 tests and a nine-relation typed graph for a store holding 29 links is the signature of ordering work by what was buildable rather than by what was uncertain.

Full document at `product/WAY-FORWARD.md`. It is not yet in Keel — it arrived as a repo file and should be imported.


### B-28 — s2, s7 and s9 are genuine misses, not L0

`accepted` · decided 2026-08-10 · `dec_01KZMTF8PVC0AWYFPQVGXM69BB`

KB's judgement, 2026-08-10. The three silent sessions in Run A were all pure implementation prompts - cache a lookup, fix gc() wiping the store on an empty keep set, make put() atomic. Each is a genuine miss rather than a session with nothing worth recording.\n\nThat is the right call. A bug that would wipe a content-addressed store on an accidental empty keep set is exactly the kind of thing a project memory should hold, and 'we found and fixed a data-loss bug' is more valuable six months on than most of what did get recorded.\n\nConsequence: Run A is 7 of 10 against a bar of 9, and the residual is real. It also settles the open judgement in TQ-21 and re-frames Step 6.


### B-29 — Phase 2 closed on 18 of 20

`accepted` · decided 2026-08-10 · `dec_01KZN24NH42AW7XQB9GNNZ0NFY`

KB's call, 2026-08-10.

The exit criterion - across ten unprompted sessions Claude writes to Keel in at least nine, every write attributed, zero duplicate projects - was met on two consecutive independent draws. Runs B and C each scored 9 of 10. Pooled 18 of 20, point estimate 90%, 95% CI [69.9%, 97.2%].

What closes it is not the score alone but the mechanism being understood. Six runs decompose into three distinct failures with three distinct fixes: the SessionStart hook fixed noticing and intent (ceiling ~30% to 80%), the continuation turn fixed execution (offers 8 to 1, and it was an instrument artefact rather than a product fault), and the deterministic Stop hook fixed the residual by reaching sessions that never noticed Keel at all during heads-down implementation work. The Stop hook fired in exactly the three sessions that had missed in Run A and stayed silent for the seven that had not, which is stronger evidence than the aggregate.

What is knowingly carried rather than resolved:
- The pooled lower bound is 69.9%. Twenty sessions cannot establish a 90% rate, and 9-of-10-at-n=10 was retired as a statistical instrument for exactly that reason. Closing the phase is a judgement that the mechanism works, not a claim that the rate is 90%.
- The precision floor does not exist. Step 10's independent hand-judge of ~20 writes remains open as a p0. The criterion is pure recall, which is least trustworthy when recall is high.
- Chat and Cowork have neither hook and are entirely untested. Everything measured is Claude Code.
- One known failure mode survives: a session that receives the Stop nudge and answers the user's next question instead.

These are recorded as open work, not as reasons the phase stays open.


### B-30 — The p0 wedge was SIGKILL mid-write, not the FTS index

`accepted` · decided 2026-08-10 · `dec_01KZN2W5BPHM5DH3PRSHW5A600`

Fixed 2026-08-10. I diagnosed this wrong twice before getting it, and the wrong diagnosis is recorded in STATUS as of the previous commit.

What it actually was. An UPDATE raised a DuckDB FATAL: 'Failed to delete all rows from index. Only deleted 0 out of 1 rows.' That is an ART index disagreeing with its table. A FATAL poisons the DuckDB connection, so every subsequent query fails with whatever operation happened to be running - 'count matching rows' on a create, 'run a question lookup' on an update. Reads on a freshly started process worked because they never touched the damaged index, and fsck reported clean because it checks referential integrity, not index consistency. Every observation was true and the conclusion was still wrong.

My earlier claim that the FTS index was broken came from searching after a write had already poisoned the connection. Search on a genuinely fresh daemon returns hits. The FTS index was never involved.

The cause. The daemon's graceful shutdown waits for in-flight connections, and /api/events is a Server-Sent Events stream that by design never ends. So SIGTERM never completed, and every restart this session ended in SIGKILL - repeatedly, mid-write. That is how the index and its table stopped agreeing.

Three fixes:
1. Shutdown now runs on a five-second deadline and CHECKPOINTs before exiting. Verified: with an SSE stream open, SIGTERM stops the daemon in 5s and logs the checkpoint. It used to hang indefinitely.
2. Error chains are surfaced. Error::chain() walks to the root cause and the MCP boundary reports it. The source was attached the whole time; nothing printed it, which is why two hours went into guessing instead of reading.
3. Regression test: create, checkpoint, reopen, update - the exact cycle, asserting the connection is not poisoned afterwards.

Recovery used backup and restore, which rebuilt every table and index from Parquet. 536 rows, verified per table. The damaged store is kept at ~/.keel.corrupt-20260810T053513Z and the Parquet backup at /tmp/keel-backup-repair. No data was lost.

Worth noting for the panel's Step 8: their hypothesis was two write paths into one file where only the daemon maintains derived state. That was reasonable and it was not the cause. The cause was cruder - the daemon could not be stopped politely, so it was stopped rudely, many times.


### B-31 — restore now re-establishes the store's git repository

`accepted` · decided 2026-08-10 · `dec_01KZN3K1A6PBRFVJ9H9H6542HM`

Found one command before deleting the only copy that still had it. SPEC §11 names three recovery tiers: the store's own git history (full fidelity, every revision), the Parquet backup, and the markdown mirror. keel restore rebuilds from tier 2 into a fresh directory - and handed back a store with no .git, so a restore silently cost you tier 1, the tier with the most fidelity.

That is worse than it sounds because of when it fires: you only restore after something has already gone wrong, so the moment you use tier 2 is exactly the moment you lose tier 1. Nothing warned, and verify_restore passed because it checks rows, not recovery properties.

The fix lives in keel-cli rather than keel-core, mirroring plugin/install.sh: keel-core does not spawn processes, and 'a store should be a git repo' is policy rather than storage. After a verified restore the CLI runs git init, writes the models/ .gitignore, and commits the restored state - an empty repository restores nothing, so the state has to be in a commit. No remote, which is Q-2 and KB's call.

It never fails the restore. A missing git binary prints a warning naming the exact command to run, because the rows being back matters more than the tier being re-established this second.

Two tests: a restored store becomes a repo with its state committed and models/ ignored, and an existing repo is left alone - the same loss in the other direction.


### B-32 — KB confirmed: idempotency keys stay on all thirteen tables

`accepted` · decided 2026-08-10 · `dec_01KZN5H4EJ905TXJA2RTS0MNKY`

TQ-9 answered 2026-08-10. B-10 stands and is no longer provisional.

The spec disagreed with itself - REQ-7 and section 7.2 say every create is idempotent, section 3.2 gave the column only to tasks. Implemented on all thirteen because the alternative silently drops idempotency for twelve types including projects, the one type where duplicates are called out as ruining the cross-project view.

It has since earned it on organic traffic: across the gate runs, sessions called create twice with an identical title on nine occasions and the key deduplicated every one.


### B-33 — KB confirmed: BM25 stays in DuckDB, Lance does vectors only

`accepted` · decided 2026-08-10 · `dec_01KZN5H4FFR7VHD92Z1PWRTMRA`

TQ-10 answered 2026-08-10. B-12 stands and is no longer provisional. SPEC section 5 is now formally wrong about which engine ranks keywords and should be corrected.

The original design put both halves of hybrid search in lance_hybrid_search. Its keyword half could not be characterised: 'onboarding metering' returned a document containing only metering, 'onboarding slow' returned nothing despite a document containing onboarding, and a third query returned an unrelated document with a score identical to an unrelated query's.

DuckDB's fts extension is a real BM25 index with documented behaviour, and it covers every artifact type rather than prose alone, so a spec and a task compete in one ranking instead of two. Lance keeps the vector index and the blobs. Search has behaved correctly throughout.


### B-34 — The desktop app routes on the hash, and the router is hand-written

`accepted` · `dec_01KZNHQ0SMBXVKYF3SA85W9VZ7`

#### Context

Phase 6 needs every screen, project, document, search and task to have an address. The app had no router at all.

#### Decision

**Route on `location.hash`, with a hand-written route table in `apps/desktop/src/lib/router.ts`.** No routing dependency.

#### Reasoning

Two separate calls, both pointing the same way.

**The hash rather than the path.** A path-based router needs the server to fall back to `index.html` for any deep URL. Vite's dev server does that; Tauri's asset protocol does not. So `/projects/keel/board` would have 404'd on reload in the built app — precisely the failure routing was added to fix, and one that would only have shown up in the packaged build. The hash never reaches a server, so one bundle behaves identically in dev, in the Tauri webview, and in the future static web build SPEC §10 asks for.

**Hand-written rather than a library.** Eleven route patterns and a query string come to about 200 lines including the comments. A router library would need roughly as much configuration and would add a dependency to the surface. Same reasoning as B-14 for components.

Two properties are held by tests rather than by care: `parseHash(toHash(r)) === r` for every route the app can build, and an address matching no route falls back to Home while keeping its query. `App` then canonicalises — an address that means nothing, or names a project-scoped screen with no project, is rewritten with `replace` so it never becomes a Back destination.

#### Reversible?

Yes, cheaply. Every call site goes through `href`, `navigate` and `useRoute`; swapping to paths or to a library means rewriting one module.


### B-35 — Eight named type sizes in two scales, not eleven anonymous ones

`accepted` · `dec_01KZNHQ6BNEH54ZG8HQ7WRR2S5`

#### Context

The app used eleven ad hoc pixel sizes with no names — `text-[15px]`, `text-[12.5px]`, `text-[10px]` and so on — so no two screens agreed on what a label or a heading was.

#### Decision

**Six named steps for the interface: `display`, `title`, `heading`, `body`, `small`, `micro`. Two more for rendered document bodies only: `doc-title` and `doc-section`.** All eight are Tailwind theme tokens in `styles.css`. No raw pixel size survives anywhere in `src/`.

#### Reasoning

A name is what makes a size a decision rather than a guess: `text-micro` says "this is metadata", `text-[10px]` says nothing.

The two extra steps are an honest exception rather than a leak. A rendered spec has its own heading hierarchy and needs steps above `text-title`, which the interface never uses; forcing an article and a toolbar onto one scale would flatten the article. Naming them keeps the count truthful — eight sizes, every one a decision, in two clearly separated scales — instead of quietly reintroducing anonymous values inside the markdown renderer.

12px folded into 13 and 10px into 11. Nothing was lost that a reader can see.

#### Reversible?

Yes. One file.


### B-36 — The light scheme overrides :root, not a second @theme block

`accepted` · `dec_01KZNHQCRB7PYBKW0Q37P4VFVK`

#### Context

The light scheme was declared as a second `@theme` block nested inside `@media (prefers-color-scheme: light)`, and it overrode only the surfaces, the ink and the accent. `good`, `warn` and `bad` kept their dark-tuned values.

#### Decision

**Declare the palette once in a top-level `@theme`, and override the custom properties on `:root` inside the media query — including the three status colours.**

#### Reasoning

Tailwind v4 resolves every colour utility through `var(--color-…)`, so an override on `:root` reaches utilities that were generated once. A nested `@theme` is not the documented arrangement and depends on where the block lands in the cascade.

The status colours mattered more than the mechanism. A hue tuned to sit on a near-black surface at 0.74–0.80 lightness is close to invisible on a near-white one: in light mode "done" and "blocked" both read as pale smudges, and a colour system that cannot be told apart is not carrying information. Light values are 0.52–0.55 lightness at similar or higher chroma.

#### Reversible?

Yes. One file.


### B-37 — The desktop app gets Vitest and Testing Library

`accepted` · `dec_01KZNHQHCJ7D50QAY8738NNF3A`

#### Context

The definition of done requires tests, including at least one failure case. The desktop app had no test runner at all — every test in the repository was Rust.

#### Decision

**Vitest with jsdom and `@testing-library/react`, configured inside the existing `vite.config.ts`.** `npm test` in `apps/desktop`.

#### Reasoning

Vitest reuses the Vite config the app already has, so the transform pipeline under test is the one that ships — a separate Jest setup would mean a second TypeScript and JSX configuration that can drift from the real one.

jsdom rather than a real browser: what needs testing here is routing, ranking and keyboard handling, none of which needs a compositor. The parts that do need one — layout, the light theme — are not things a unit test would have caught anyway, and were checked in a browser instead.

One patch to the environment, in `src/test-setup.ts`: jsdom has no `Element.prototype.scrollIntoView`. Stubbing it there keeps the guard out of product code, where it would have been test scaffolding shipped to users.

#### Reversible?

Yes, though there is now a test suite that would have to move with it.


### B-38 — Graph traversal carries the neighbour's label

`accepted` · `dec_01KZNQ3BCRH4CM0CAVV3DYC7TQ`

#### Context

`Neighbour` was `{id, entity_type, rel, anchor, depth, path}`. Everything that rendered or reasoned about a traversal therefore had to go back and look up what each id was.

Two callers had already gone wrong in the same way. The document reader printed bare ULIDs under "Connected" — the id was all it had. And an agent walking the graph got a list of identifiers it had to follow with a `keel_get` per hop to learn what it had found.

#### Decision

**`neighbours()` joins `v_entities` and returns a `label`.**

#### Reasoning

`v_entities` exists for exactly this: SPEC §4 built it to resolve an id without knowing its type, and unifying the four different name columns — `name`, `title`, `term`, `summary` — is what its `label` column is for. The join is a `LEFT JOIN` on the walk's final select, which costs one lookup per returned row on a store of a few thousand.

`LEFT`, not inner, and this is the part worth keeping: an edge pointing at a row that no longer resolves comes back with an empty label rather than being dropped. Dropping it would turn a visible integrity problem into a silently shorter graph — hiding precisely what `fsck`'s dangling-link check exists to report, and in the direction that makes everything look fine.

#### Reversible?

Yes, but there is no reason to. It is one additive field on a read shape.


### B-39 — The Tauri shell is suspended; the web build is the surface

`accepted` · `dec_01KZNQR16ZJKQ5MGTSF8H0VW9C`

The desktop shell is not built for now. Work on the read surface happens in the browser: `npm run dev` in `apps/desktop` serves the React bundle on :1420 and proxies `/api` to the daemon on :7654.

**Why.** Nothing in `apps/desktop/src` imports a Tauri API — the frontend is already a plain web app, and SPEC §10's "same bundle, different base URL" holds today. The shell therefore buys nothing at this stage while costing a webview dependency tree and ~1.2 GB of build output. That cost was invisible until the disk filled: `target/` had reached 11 GB alongside `src-tauri/target/` at 1.2 GB, on a volume with 10 GB free.

**How it is enforced.** `apps/desktop/src-tauri/build.rs` exits with a message unless `KEEL_DESKTOP=1` is set. A loud refusal rather than deleting the crate, because the shell is coming back — this is a pause, not a reversal of the Phase 3 plan. The workspace already excluded `src-tauri`, so no ordinary `cargo` command was building it; the guard is what stops the next session from doing it by hand without noticing the cost.

**To un-suspend:** delete the guard block in `build.rs` and the note on the workspace `exclude`.


### B-40 — Readable identifiers are composed, never stored

`accepted` · `dec_01KZNW724SBG1NFAWDZ9CR66DN`

#### Context

Phase 6 adds `KEEL-42` alongside `tsk_01KZKW28CS4Q1WSB0D95B2A01G`. The obvious implementation is a `ref` column on `tasks` holding the composed string, written once at creation.

#### Decision

**Store the two halves — `projects.key` and `tasks.number` — and compose the label at every point of use. Nothing anywhere stores `KEEL-42`.**

#### Reasoning

A stored composite is a denormalisation whose invalidation nobody owns. Re-keying a project — which the key being editable makes a legitimate operation — would require rewriting every task row, and any that were missed would go on displaying the old prefix while resolving under the new one. That is the failure this project keeps meeting in other clothes: something that looks right and is quietly wrong.

Composing costs a project lookup at the surfaces that do not already hold the project, and every surface that renders more than one task already holds it: `ProjectLine` carries the key, so the digest, the board, the detail view and the tracker all have it to hand. The one place it is genuinely per-row is `keel_get`'s summary, where it is a point lookup on a table with a handful of rows.

Two consequences that took some deciding:

**A number is never reused, even after an archive.** `MAX(number) + 1` counts archived rows. If a number were handed on, `KEEL-1` would keep resolving and silently start meaning a different task — and every note, commit message and conversation that used it would be wrong with nothing to say so.

**The uniqueness index is on `upper(key)`.** References resolve case-insensitively, so `KEEL` and `keel` must be one identifier. A plain unique index would have permitted both as separate projects, leaving the lookup to pick one arbitrarily.

#### Reversible?

Adding a cached column later is easy. Removing one that has drifted is not, which is the asymmetry the decision turns on.


### B-41 — KB confirmed: a task holds a list of external links

`accepted` · `dec_01KZP1E78WZXXTJZK7YBHATJCZ`

#### Context

TQ-23. RESET-PLAN 6.2 asked for a task to be able to hold more than one external reference. `tasks.external_ref` was `Option<String>`, and changing it is a storage-format change, so it was raised rather than assumed.

#### Decision

**KB confirmed, 2026-08-10: a task can hold more than one.** `external_ref VARCHAR` becomes `external_refs VARCHAR[]`, backfilled from the single value and then dropped, in the same migration as rank and the parent link.

#### Reasoning

Option 1 of the three offered. The column type already exists on this table — `labels` is a `VARCHAR[]` — so it costs one migration step and no new machinery, and it is the only one of the three field additions that needs no new UI beyond rendering a list where a string was rendered.

The old column is dropped rather than kept alongside. Two columns meaning the same thing is drift with a schedule attached, and the one that stops being written is the one everything keeps reading.

Renaming rather than aliasing: a caller passing `external_ref` now gets serde's own "task has no field `external_ref`… any of: …", which names the replacement. Accepting both would be the undocumented-parameter problem RESET-PLAN 7.3 exists to remove, created deliberately.

#### Reversible?

Forward-only, like every migration here. The data survives either way — the backfill copies before the drop.


### B-42 — KB confirmed: blocked is derived from the links, not a status

`accepted` · `dec_01KZP5189J3N9R1BJESQ0PGJNZ`

#### Context

TQ-25. RESET-PLAN 6.5 settled that the links win for what "blocked" means. It did not settle whether `blocked` survived as a value a caller could set, and the two readings lead to materially different work.

#### Decision

**KB confirmed, 2026-08-10: derive it.** A task is blocked exactly when something links to it with `blocks`. `blocked` stops being a `TaskStatus`; the board's column becomes a computed grouping; the counts in the app, the digest and the generated tracker all come from the same derivation.

#### Reasoning

The same call this codebase has made everywhere else: make the contradiction unrepresentable rather than detectable. Option 1 would have kept two facts that must agree and added an integrity check to notice when they do not — which is a check that fires *after* someone has already read the wrong number.

The evidence was on screen while the question was being asked. The digest reported two tasks as "marked blocked, but nothing links to it with `blocks`" — KEEL-45 and KEEL-48. Under the rejected option those become findings a human clears, one at a time, forever. Under this one they simply stop being blocked, because nothing is blocking them and nothing ever was.

It costs a forward-only migration and a visible behaviour change, which is why it was KB's to make rather than mine.

#### Reversible?

The migration is forward-only. Re-adding an enum value later is easy; the rows that were moved out of `blocked` would not come back, and should not — they were wrong.


### B-43 — An accepted decision can be corrected; the revision chain is the guard

`proposed` · `dec_01KZPNN6TBH77592TR7VN6DD4K`

#### Context

`keel_update` refused any content change to a decision whose status was `accepted` — SPEC §3.2, enforced in `keel-core` because the schema cannot express it. The remedy it named was to create a new decision linked with `supersedes`.

`keel_write_doc` was never subject to it. It replaced an accepted decision's entire body without complaint, and did so twenty-five times on 2026-08-10 while the reasoning was migrated out of the prose table into the rows.

#### Decision

The guard is removed. An accepted decision can be edited, and the revision chain is what makes the edit safe.

#### Reasoning

It sat on the wrong door. A title is a label; the body is the argument, and the argument *is* the decision. Guarding the label while leaving the argument writable stopped the harmless edit and permitted the harmful one.

The concrete cost was visible: seven decision titles had been truncated at roughly eighty characters by whatever imported them — B-8 read `Surface carries five values, not four: chat \` — and could not be corrected. They were invisible while the prose table carried the real titles, and became the headings of the generated log the moment the register was unified. Correcting a transcription defect is not amending a decision, but a write guard cannot tell the two apart. The revision chain can, after the fact, which is when the question is actually asked.

What replaces it was already there: every change is an attributed revision with a diff and an event naming the field. A reworded decision is visible rather than prevented, and *visible* is the property that was wanted — "supersede instead of editing" is advice about how to think, and it survives as advice.

The old test asserted only that the error named the remedy. It never asserted that the body it was protecting was protected, which is part of how the gap lasted.

#### Consequences

Retitling a decision changes its mirror slug, and `generate` never deletes, so seven orphaned files under `.keel/decisions/` had to be removed by hand. A generated file that survives a rename reads as current, which is its own small instance of the disease this register unification was fixing. Recorded as TQ-28.

#### Reversible?

Yes — the guard was six lines. Re-adding it would re-break title correction, so anything that reinstates it should guard the body too or not at all.


### B-44 — A missing readable number is read as unassigned, not as an error

`proposed` · `dec_01KZPTGB10RJERCFAJR9C1R71B`

#### Context

Reported from another project: `keel_create` with `type: "decision"` failed reproducibly with `read column number of decisions: Invalid column type Null`. A decision saved earlier the same morning had worked.

#### Decision

A missing readable number is read as zero — "not yet assigned" — rather than as an error, and the write paths assign a real one. Migration 13 repairs rows already written.

#### Reasoning

Every schema change opens a window. Migration 10 added `decisions.number` at 18:43:57 and backfilled everything that existed; it could not reach forward. At 18:45:21 — **84 seconds later** — a daemon that had the column but not the struct field inserted a decision, and the row got a NULL.

Reading that as a hard error was catastrophically out of proportion. One row with a NULL made **every** decision in that project unreadable, and because the idempotency check is a read, `create` failed too. So a single unnumbered row presented as "this artifact type is broken" rather than "this one row has no label".

Proportion is the principle: an unnumbered row costs its own label and nothing more. Zero already meant "not yet assigned" everywhere else in the codebase.

Reading NULL as zero alone would trade one failure for a worse one — two rows written back at zero would collide on the unique index — so the update path now assigns a number to anything holding zero, matching what create already did.

`tasks` had the identical shape and survived only because migration 6 landed in a quieter minute. Migration 13 covers both.

#### Consequences

The general lesson, which is worth more than the fix: **a column added by a migration is NULL for every writer that has not been restarted yet.** Any read of a newly-added non-nullable-in-practice column has to tolerate that window, or the migration becomes an outage for whoever writes during it. The window here was 84 seconds and it still caught a real project.

#### Reversible?

Yes. The lenient read is one function; the migration is a backfill that is a no-op on a clean store.


### B-45 — Every milestone carries a plain-English explainer, required at creation

`accepted` · `dec_01KZR4KZQ8BXFXA1PRFTXEPE07`

#### Decision

A milestone cannot be created without a short plain-English summary of what the phase covers. `keel_create(type: "milestone")` requires it and refuses a create without one, naming what was missing and what would be valid.

KB's call, 2026-08-11, on seeing Phase 8 appear on the roadmap with no description.

#### What "plain English" means here

KB set the standard in the same conversation, and it is the harder half of this decision. The summary must read like a person wrote it:

- **One or two sentences.** The existing summaries are 8 to 15 words — "Deployable daemon, auth, mobile client." A paragraph is too long.
- **Say what the phase does for the reader**, in the words they would use. Not the section numbers, not the internal names.
- **No AI register.** No em-dash asides, no "genuinely" or "deliberately" or "rather than", no rule-of-three lists, no "not X but Y", no sentence that exists to sound considered.

The first attempt at Phase 8 failed this: five clauses, six section references, and the phrase "constitute doing work rather than describing it". It was replaced with "Make the everyday loop work: file a bug in seconds, see what's ready to start, and read the board without opening every card."

This is a house style rule, not only a milestone rule. It applies to any prose the product puts in front of a human.

#### Context

The trigger was a silent drop, not just an omission. `keel_create` takes a `body` argument for every type. `build_entity` (crates/keel-mcp/src/dispatch.rs:1070) routes it into `description` for a project and `body` for a task, and for a milestone it calls `Milestone::new(project, name)` and ignores `body` completely. `Milestone.summary` exists (crates/keel-core/src/types.rs:307) and nothing on the create path writes it.

So a session that supplies a description gets a success, no warning, and no description. An input that is accepted and thrown away is worse than one that is refused, because the caller has no way to find out.

The data shows it. Phases 0 to 5 were created by `keel bootstrap`, which builds the struct directly and sets the summary; all six have one. Phase 6 and Phase 7 were created through `keel_create` over MCP, and both are empty.

#### Reasoning

The roadmap answers "what is this project doing, and in what order". A phase whose row is a bare name answers that only for whoever wrote it. There are eight milestones and ninety-nine tasks, so the milestone is the unit a human actually reads and an unreadable one costs more per row.

Requiring it at the tool boundary follows what this project has already measured. Skills do not fire unprompted — thirty gate sessions, zero invocations. A rejection at the tool boundary was recovered from in the same turn by both sessions that hit one. A required property is confronted on every call, on every surface, whether or not anything else loaded.

#### Consequences

- `Milestone::new` takes the summary as a required argument, so the compiler finds every call site.
- `keel_create` declares it required for milestones and refuses a missing or empty one.
- Phase 6 and Phase 7 get summaries written from what they shipped.
- The style rule above goes in the tool description with a good and a bad example, the way §8G proposes for task summaries. A length ceiling is enforceable; the register is not, so the description has to carry it.
- Narrower than TQ-34, which asks the same of tasks and is still open. Eight milestones is a small, unambiguous set. Deciding milestones does not decide tasks.


### B-46 — The plain-English rule covers every prose field, not just milestone summaries

`accepted` · `dec_01KZRMFKQARKG1K6NEW1MD5222`

#### Decision

Everything Keel stores as prose — decision bodies, question bodies, specs, feedback, notes, task bodies and summaries, titles — must read as though a person wrote it. The rule from B-45 is not specific to milestones and is now applied wherever prose enters the store.

KB's call, 2026-08-11, extending B-45 on the same day it was taken.

#### What is actually enforceable, stated honestly

This is the part worth being straight about, because the request is larger than any validator can satisfy.

**Structure can be checked. Voice mostly cannot.** A rule can see that a field is empty, that it restates its own title, or that it cites `TQ-15` with nothing beside it. It cannot see that a sentence is limp, over-hedged or arranged for cadence rather than meaning. Anyone claiming otherwise is describing a check that will be wrong in both directions.

**A false rejection is worse than a mediocre sentence.** When a model is refused for a reason it does not accept, its recovery is to satisfy the letter of the rule — swapping a banned word for a synonym and keeping the same shape. That is worse than the original, because the prose is now both bad and rule-compliant, and the check reports success.

So the enforcement is deliberately split three ways.

#### Three layers, by how reliably each one works

**1. The tool descriptions, which are the real mechanism.** Every prose-bearing field says what good looks like, with a worked good/bad pair. A model reads this at the moment of writing, on every surface, whether or not any skill loaded. This is the layer that changes what gets written, and it is the one with no false-positive cost.

**2. Rejection, for what is objectively wrong.** Empty or whitespace. A body that only restates its title — reusing the containment rule from KEEL-65 rather than inventing a similarity measure. A bare `TQ-15`, `B-44`, `KEEL-96` or `REQ-7` with no gloss, using the parser `fsck` already has. A short list of phrases with essentially no legitimate use in a project tracker. Each rejection names the span and what to write instead.

**3. Warning, for what is a signal rather than a rule.** Softer tells are reported alongside a successful write rather than blocking it. The write lands, and the session is told what read as machine-written. This teaches without the write-around failure mode.

#### Quoted material is exempt

Fenced code, inline code and block quotes are stripped before any check. A note quoting an error message, a spec quoting a vendor's documentation, or a decision quoting what someone actually said is carrying someone else's words, and refusing those would make the store unable to record the world as it is.

#### Consequences

- One `style` module in `keel-core`, so the CLI, MCP and `keel import` cannot diverge on what is acceptable.
- The SessionStart hook states the house rule in one line, since that is the channel measured to reach the model — thirty gate sessions invoked the skill zero times.
- The existing rows are not rewritten. A machine inventing replacement prose would produce exactly the confident, plausible, wrong text this rule exists to prevent, which is the same reasoning that stops the mirror ever reading a file back. `keel lint` reports them.
- This does not settle TQ-34, which asks whether a task `summary` is required at all. This decides how prose is judged once written, not which fields must exist.


### B-47 — The close reason is a column, and closing is checked on the transition

`accepted` · `dec_01KZS0SFC4YAGPC58TGDG677T9`

KEEL-110's body left one thing open: `duplicate`, `superseded` and `no_change` are reasons, but only `done` and `wont_do` are statuses, so where the reason lives had to be settled when the task was picked up. It is a `close_reason` column on the task.

#### What was chosen

Five reasons over two statuses. `done` maps to `Done`; `wont_do`, `duplicate`, `superseded` and `no_change` all map to `WontDo`, and the column says which of the four it was. `close_message` and `evidence` sit beside it.

The alternative was mapping the last three onto `wont_do` and recording the reason nowhere, which loses the only thing that distinguishes them. Adding four statuses was the other alternative, and it would have put the same information somewhere every query filtering on `is_open` has to learn about.

#### Where the rule is enforced, and why it matters

In `DuckStore::update`, not in `work::close`. Any path into a terminal status is held to it, so a caller reaching for `keel_update(status: done)` to avoid answering the question is refused by the same check. That is the difference between an invariant and a second convention, and it is what makes the definition of done in this file more than a list somebody is asked to honour.

#### Checked on the transition only

A hundred and seven tasks closed before any of this existed and carry no reason, no message and no evidence. Two things follow.

Running the check on every write would freeze every one of them: moving an old row's priority would be refused for a message nobody was being asked to write. And backfilling would mean inventing a reason for work nobody remembers — a store that cannot tell an invented reason from a stated one is worse than one with holes, because the holes are at least visible.

So the rule sits on the transition, and `keel lint` reports what falls through. That is the same shape TQ-34 settled for task summaries, for the same reason.

#### The hole left open on purpose

`keel_create` with `status: done` still bypasses the rule. `bootstrap` and `import` both create already-closed rows, and enforcing on create would break both. Creating a task that is already finished is a migration shape rather than doing work, but it is a hole and it belongs on `keel lint`'s list.


### B-48 — A claim is optimistic concurrency, not a lock

`accepted` · `dec_01KZS0SZ6V2YCKKGX3ANTW777B`

`keel claim` had to be atomic, and the obvious reading of that is a lock. It is not one.

Claiming reads the task, checks whether anybody is holding it, and writes through the ordinary update with the version it read. Two sessions racing for the same task both read version 7; the first writes 8 and the second is rejected with a stale-version error carrying the current state and the events in between. That is already how every other write in Keel behaves, and it is exactly the property a lock would have been added to provide.

A lock would also have needed a release path of its own, and something to release it when the holder dies. Both of those already exist here and neither is new machinery: closing clears the claim in the store's update path, and a claim goes stale after three days.

#### The three days are fsck's number, not a second one

`fsck` already warns about a task parked in `in_progress` for three days. Choosing a different threshold here would have meant two answers to "this session is probably gone", and the disagreement would surface as work `fsck` calls abandoned and `keel claim` refuses to take.

#### The one write refused for want of a session

A claim with no `session_id` is refused outright. Everywhere else in Keel an anonymous write is merely less traceable — SPEC §6.5 says to fall back to the transport's identity rather than decline. Here the session is the content: a claim naming nobody excludes the task from `keel ready --unclaimed` while telling no one who to ask about it, which is worse than leaving it unclaimed.

#### Releasing lives in the store, not in the close path

Any transition into a terminal status clears `claimed_by` and `claimed_at`, wherever it came from. Putting that only in `keel_close` would have let a plain `keel_update(status: done)` leave a claim standing, and `ready --unclaimed` would have gone on skipping work nobody was doing.


### B-49 — Reading an image off the disk is a field, not a fourteenth tool

`accepted` · `dec_01KZS2VXYGZ35YVV56QZ4AYNC0`

TQ-33 approved the capability by name: `keel_attach(id, path)`, so the daemon can read a file that is already on the same machine. TQ-31, settled hours earlier the same day, set thirteen tools as the ceiling and said a fourteenth needs an argument at least as good as the one that earned the thirteenth.

Both are KB's. This resolves them in favour of the capability without spending the slot.

#### What was built

`image_path` on `keel_create`, beside the `image` field that already takes base64, and `image_path` in `keel_update`'s `changes` for attaching to something that already exists. Absolute paths only, up to 10 MB, sniffed from the magic bytes.

#### Why this is the same decision rather than a different one

The substance of TQ-33 is that the daemon may read a local file. The form — a tool called `keel_attach` or an argument called `image_path` — is naming and internal structure, which the standing rules say to decide and record rather than ask about.

And `product/CLAUDE.md` names the alternative as an anti-pattern in almost these words: reaching for a new type when the modelling is awkward, where it is almost always a field. A second tool for "the same thing, from a path" would have been a second door onto one capability, with the base64 form on `keel_create` and the file form somewhere else — so a model deciding how to attach an image would first have to decide which tool attaches images.

The one thing a tool would have bought is a shorter call for the attach-to-existing case, which is `keel_update(id, version, changes: {image_path})`. That is not worth a permanent slot on a surface where every extra tool makes selection worse.

#### The boundary that has to hold

A local path and a URL look similar and are not. One touches the machine Keel is already running on; the other gives a model the ability to make the daemon talk to the internet, which TQ-6 declined and TQ-33 confirmed. So anything URL-shaped is refused explicitly, with the reason in the message, and a test asserts it for `https:`, `http:` and `file:`. TQ-33 predicted the failure mode exactly: if the path argument ever quietly accepts something URL-shaped, that is this decision being reversed by accident.

Relative paths are refused too, for a duller reason: the daemon's working directory is its own, so a relative path resolves against something the caller cannot see.

#### What the base64 description used to promise

1 MB, which no session can reach. Base64 inflates by a third and the *model* emits every character, so 1 MB is 350,000 to 450,000 output tokens and the useful ceiling is nearer 100 KB. The description now says the reachable number, says why, and points at the path with no such cost. Verified against a live daemon: a 683 KB PNG went from disk to the store and back out of `/api/blob/{id}` intact, which through base64 would have cost roughly 240,000 output tokens.


### B-50 — A glossary term can declare which type it is a spelling of

`accepted` · `dec_01KZS6ZARDDED3P4GF3X8QF9E7`

KEEL-116 made `keel_create(type: "phase")` work by adding a fixed list of aliases in the source. That closed §8F's exit criterion and left the general problem open: every project's vocabulary had to be anticipated by whoever wrote the list, and a project saying "incident" for a task was out of luck until somebody shipped a binary.

`Term` gains a `means` column, holding one of the thirteen types. A term that declares one is consulted before the built-in list.

#### Why a column and not the definition

The task this came from proposed having the alias table "consult the glossary", and the obvious reading is to parse the type out of a term's definition — the prose is right there. It does not survive contact with real definitions: "a phase is a milestone with a demo at the end" and "a phase is not a milestone" mention the same word and mean opposite things, and a rule that read either would resolve them identically.

A declaration cannot be misread. And because it is an `EntityType` rather than a string, the type system enforces the thing that actually matters here.

#### The rule this lives under

**A term declares a spelling, never a concept.** This is the feature most able to break the thirteen-type ceiling, because for the first time a *stored row* can introduce vocabulary — and a row is written by whoever is using Keel rather than by whoever reviews a pull request. Making `means` an `EntityType` makes a fourteenth type unrepresentable, which is the same move TQ-31's alias table made and for the same reason. A test asserts it across every word the glossary knows.

#### The resolution order, and why each step is where it is

1. **The canonical name.** Nothing can shadow it — a project defining a term called "task" that means a decision gets a task, because `keel_create(type: "task")` has to mean the same thing in every project forever. Tested.
2. **This project's glossary, then the global one.** Project-first is Q-4's existing rule for terms, and it applies here for the same reason. Another project's term never applies: a word meaning one thing here and another there is precisely what project scoping is for.
3. **The project's own `milestone_noun`.** Not redundant with the glossary even though setting the noun seeds a term, because the noun is what the *interface* says: a project whose board reads "Phase 8" should accept "phase" on input whether or not anybody kept the term in step.
4. **Keel's built-in list**, which is where KEEL-116 stopped.

#### The display noun

`milestone_noun` on projects is a label and never changes what is stored. The tracker now writes "Active phase", "## Phases" and a "Phase" column header; the board's filter says "Phase" and "Any phase"; the digest's first paragraph says "active phase: Phase 8".

A noun that is another type's name is refused at the point somebody sets it. A project calling milestones "tasks" would make every `keel_create(type: "task")` ambiguous, and the resolution order *hides* that rather than surfacing it — the canonical name wins, so the noun would silently do nothing.

#### Narrating why, not just what

KEEL-116 established that resolution is narrated: a session told "you said 'sprint' — in Keel that is a milestone" learns the vocabulary in one round trip. This adds the reason. "Because this project's glossary says so" tells a session where the vocabulary lives; "because Keel recognises that word" tells it the word is universal. The difference is actionable.


### B-51 — Phase 9 runs before Phase 10, and DuckDB and Lance come out of the tree entirely

`accepted` · `dec_01KZS7XBT5GZXPG7CGYN75WWYZ`

#### Context

The Phase 9 spec ended with one thing needing KB: whether the SQLite move runs before Phase 10, or whether `DUCKDB_DOWNLOAD_LIB=1` is a good enough interim.

#### Decision

Phase 9 runs now, before Phase 10. And it does not stop at "SQLite is the store" — once the migration is verified, every DuckDB and Lance dependency comes out of the repository: the crate dependencies, the code paths, the install scripts, the CI workflow, the profile settings that only exist because a C++ database is vendored, and the two-format backup path.

KB's call, 2026-08-11, on the record in this session.

#### Reasoning

The interim removes the 22-minute build and leaves everything else: two formats, two backup paths, a keyword index rebuilt wholesale, and a release story that ships a 40–60 MB library beside every binary. It is a cost paid at every release rather than once.

Going further than the spec asked — full removal rather than a store swap — is what makes the phase worth its cost. A tree with both engines in it is a tree where either can come back by accident, and the second engine is not free even when nothing calls it: it is in the lockfile, in CI, in `cargo deny`, and in the dev profile settings written to stop DuckDB's debug info filling a disk.

#### What this commits to

The work happens on a branch, not on master. Master keeps a working store until the migration is verified by row count per table and hash per document, which is what the spec's step 5 already says.

A third thing joins the phase that the spec does not name: a measurement of what the app and the daemon actually cost to load, taken before the swap and repeated after. Without a before, "SQLite made it faster" is a thing nobody can check, and the intermittent board stall KB reports has never been measured at all.


### B-52 — Taking the payload out of a tool result is one named function, not two lines

`accepted` · `dec_01KZSKKGWMG73H09G4Q20XMDSZ`

#### Context

`dispatch` returns the MCP `tools/call` envelope — `{content, structuredContent, isError}`. Three surfaces are not speaking MCP and need what is inside it: the CLI's daemon call, the CLI's fall-back-to-the-store, and the daemon's own `/api` responses. Each had its own copy of the same two lines, and the CLI's fallback did not have them at all.

The result was KEEL-133. `keel ready` printed "nothing ready" whenever no daemon was listening, for as long as that path has existed.

#### Decision

`keel_mcp::structured` and `keel_mcp::summary_text`, used by all three.

#### Reasoning

Forgetting the unwrap is invisible. The envelope is a perfectly good JSON object, so `.get("ready")` on it returns `None` rather than failing, and every renderer here reads a missing field as an absent value — which for a list means an empty list, and an empty list has a sentence of its own that sounds like an answer. The failure mode is the one the standing instructions single out for graph direction: a plausible, calm, empty result.

Two lines copied three times is not worth naming for its own sake. Two lines whose absence is undetectable is.

The CLI now has one `run_tool` rather than a copy per command, and the unwrap sits inside `directly` — so it is not something a new caller has to remember, which is the part that failed.


### B-53 — The write-path atomicity fix: &Connection primitives, transaction-of-one, one typed composite on Store

`proposed` · `dec_01KZSQJ05N4TSXDETPAZKD685F`

*No reasoning recorded.*

### B-54 — The fixture corpus stays compiled in, ungated

`accepted` · `dec_01KZTFE58YF4AZATMPAHDQ8R87`

#### Decision

`keel-core::fixture` — about 2,200 lines of demo corpus — stays in every build. No `#[cfg(feature = "fixture")]`.

#### Why

KEEL-162 asked for it to be gated. Working through what that costs against what it buys, it does not pay.

**What it buys.** Roughly 60 KB of a binary, and only in a build that excludes it. Cargo unifies features across a workspace build, so `cargo build --release` at the repo root would compile it into the daemon anyway. The saving only lands if the daemon is built on its own, which is not how it is built.

**What it costs.** Three things, and the third is the one that matters:

1. `keel fixture` is a shipped CLI command, so keel-cli has to enable the feature. Gating it therefore does not remove it from the product, only from one crate's dependency graph.
2. `crates/keel-core/tests/fixture_backup.rs` uses the corpus. A crate cannot enable its own optional feature for its dev-dependencies, so plain `cargo test -p keel-core` would silently skip that file unless every invocation grows `--features fixture`.
3. DECISIONS B-11 dropped `--all-features` from the definition of done because feature combinations were a hazard rather than a help here, and the note added in Phase 9 records that no workspace crate declares a feature at all. Adding one puts back the machinery that was deliberately removed, in exchange for bytes nobody has measured a problem with.

#### What would change this

A measurement. If the daemon binary size or its compile time ever becomes a real complaint, the corpus is the obvious first thing to move — and the cleaner move is out of the binary entirely, into a data file `keel fixture` reads, rather than behind a feature flag.


### B-55 — Documents are embedded as passages, and the passage table is an index rather than a record

`accepted` · `dec_01KZX83HF50F7T90B2CD1P7EZ7`

KB decided, 2026-08-13, after the truncation measurement in KEEL-174.

`bge-small-en-v1.5` reads 512 tokens and a document goes to it whole, so 41% of current documents were never going to be embedded past their opening. Documents get split into passages instead: headings first, then a hard wrap around 1,400 characters with roughly 15% overlap, and the heading path prepended to each passage's text so a passage from §5 of the spec still carries what it is a section of.

A new `document_chunks` table holds them, keyed to `doc_id`, carrying `ordinal`, `heading_path`, the character span, the text, the vector and the source revision's `body_hash`. Query side groups by entity and takes the **best** matching passage per document — mean would punish a long document for having sections about other things, which is backwards. The passage doubles as the excerpt, which is better than the fixed window around the first matching term that it replaces.

**`documents.embedding` stops being written and is dropped in a later migration.** One place for vectors. The argument against a `vec0` table already in `store::search` — a second copy of every vector, and something has to keep it in step — applies just as well to a whole-document vector sitting beside per-passage ones. Nothing in Keel asks "what is this document broadly about", so the second copy would exist to drift.

**Passages are hard-deleted when the revision they came from is replaced, when the entity is archived, or when the model changes.** This is an explicit exception to hard constraint 3, and the distinction it rests on is one the codebase already relies on: `fts_source` is a derived index whose triggers already delete, and nobody has ever called that a violation, because the record is the revision in `documents` — immutable, append-only, and untouched. A passage is a derived artefact of a revision in the same way a BM25 posting is. Constraint 3 gains a carve-out naming derived indexes, and a test proves a passage can always be recomputed from its revision.

The alternative was an `archived_at` on every passage and a filter on every query, which is consistent with the constraint as written and means the passage table outgrows everything else in the store within a year while holding nothing a person can read.

The model stays full-precision `bge-small-en-v1.5` — 134 MB rather than the 67 MB compressed variant. It downloads once, in the background, while keyword search already answers, so nothing blocks on it, and the quality cost of the compressed one is not predictable on a corpus this shape. Reversible either way: same 384 dimensions, so switching is a re-embed pass and not a schema change, which is what `embedding_model` on the row is for.

Resolves QUE "May the chunk index be hard-deleted". Related: TQ-3, which asks the re-embedding question this makes cheaper — a model change is now a delete-and-recompute over a derived table rather than a rewrite of the document rows.


### B-56 — Superseded decisions stay in search results and carry a label saying what replaced them

`accepted` · `dec_01KZX83RKRDV71XF3ZWMBYY50N`

KB decided, 2026-08-13.

A decision whose thinking has been replaced — `decisions.status = 'superseded'`, or an inbound `supersedes` edge — stays in the index and stays returnable. `SearchHit` gains `superseded_by`, and the hit says which decision replaced it. Ranking is untouched.

The reason is the reason Keel exists. "Why did we stop passing `--all-features`" is answered by the old decision and the new one together; returning only the new one answers a different question. Hiding superseded rows would make the store good at describing the present and useless at explaining it.

Ranking them down was the obvious middle path and was rejected: the multiplier would be arbitrary, and the adjustment would be invisible to whoever read the results — the silent-correction shape this codebase keeps having to undo. Telling the caller what is true and letting them decide is what the close reasons and the digest already do everywhere else. It also composes: demotion can be added later on top of a label, and a label cannot be recovered from a demotion.

Not to be confused with a superseded *revision*, which is `documents.status = 'superseded'` and is already settled — search reads current revisions only, older ones stay readable by version through `keel_get`, and passages are never built for them.

Decided before chunking lands because the label has to be carried from the query through to the hit, and retrofitting means touching the same three layers twice.

Resolves QUE "Should superseded decisions still be findable by search".


### B-57 — A phase's state is derived; only shipped, cut and paused are declared

`accepted` · `dec_01KZX9ZJWEGGFSPXK1MH750G94`

#### Decision

`milestones.status` stops being a word somebody types. What a phase is *doing* is worked out from its tasks and its edges. What a phase has been *decided about* is stored, and there are only three such decisions.

**Derived, never stored, so they cannot disagree with anything:**

- **planned** — no task has moved off `todo`
- **active** — a task has started and something is still open
- **complete** — every task closed, nobody has said what that means yet
- **blocked** — something live links to the phase with `blocks`, exactly as it works for tasks

**Declared, stored, because no amount of looking at tasks can tell you:**

- **shipped** — a person says it shipped. `shipped_at` is written in the same operation, never separately.
- **cut** — dropped. Replaced rather than abandoned is `cut` plus a `supersedes` edge naming what replaced it, which is what tasks already do.
- **paused** — started, stopped, not abandoned.

The stored column holds `open` when nothing has been declared. `active` and `blocked` stop being storable at all.

#### Why

Five of twelve phases contradicted their own tasks, and nobody noticed for a week. The damage was not cosmetic: the tracker and the digest name the first phase marked `active`, so every session started this week was told the active phase was Phase 9 — which had finished. The orientation line at the top of every conversation was wrong.

This is the same failure TQ-25 removed from tasks. `blocked` was a task status that could disagree with the `blocks` edges, and the fix was to stop storing it. The argument transfers without modification: a status that can disagree with the rows underneath it is a colour, not information.

#### What made it certain rather than likely

Nothing derived it, nothing validated it, and nothing updated it when the last task in a phase closed. Compare what a task gets: `keel_claim` to start and record who, `keel_close` with one of five reasons, a message, and typed evidence for `done` — all enforced in the storage layer so the CLI and MCP cannot disagree. A milestone got five adjectives and an honour system.

#### `done` and `wont_do` are not the same thing

Writing this, a session set Phase 5 to `shipped` by the rule "no open tasks". Phase 5 has one task and it was closed `wont_do`. Nothing was ever built, and the roadmap said delivered.

Both reasons empty the column and mean opposite things. A phase whose work was abandoned is `cut`. No rule that counts open tasks can tell the difference, which is the sharpest argument for `shipped` and `cut` staying human declarations.

#### Why `paused` is the one new state

It is the only scenario here that is real, common, and underivable. A phase that has been set aside is not `active` — nobody is on it — and not `cut`, because it is coming back. Without it a shelved phase has to lie.

Everything else considered was rejected. **Superseded** is `cut` plus an edge, matching tasks; a sixth status would be a second way to say the same thing. **Ongoing** — for a phase like hardening that arguably never ends — is refused: a phase that cannot finish should be closed and a new one opened, not left running forever. **Designed but not scoped** — Phase 10 exists as a spec with no milestone and is invisible to the roadmap — is a workflow gap, not a status. It needs its own decision.

#### What this fixes that was not on the original list

`shipped_at` is a second field saying the same thing as `status`, maintained separately. Phases 7 and 9 were set to `shipped` during this work and left with an empty `shipped_at`, because `keel_update` sets the field it was given and nothing else. Writing both in one operation is part of this decision, not a follow-up.

`sort_order` has the same shape — null on four phases, which is why the roadmap prints them out of order — but ordering is a human preference rather than a fact about the work, so it stays typed. It gets a lint, not a derivation.

#### The cost

A migration, which is the second this project has had and the first exercise of the deliberate-migration path built for KEEL-154. Every surface that displays a phase — the digest, the tracker, the desktop roadmap — reads a derived value instead of a column, so the API returns the derived state alongside the row.


### B-58 — Leaked test stores get a working sweeper, not a redirected TMPDIR

`accepted` · `dec_01KZXA7K5NXDGTVTEG9G26JPBB`

KEEL-119 offered three ways to stop killed test runs leaving stores in TMPDIR. A fourth came up while working on it: point `TMPDIR` at a repo-local directory from `.cargo/config.toml`, which cargo applies to every process it spawns. That would have covered all 157 `tempfile::tempdir()` call sites without touching one of them, and confined the leak to a directory you can see.

Rejected, on the measurement. A test binary that runs to completion leaks nothing — `TempDir::drop` works. Only a killed process leaks, and the accumulation on disk traces to a single `cargo mutants` run whose mutants time out. So the leak is local, occasional, and 388 KB a time since Phase 9 dropped DuckDB.

Against that, redirecting TMPDIR globally changes where rustc and the linker put their scratch files too, and it fails in a confusing way if the directory is ever missing — every temp-using process in the build breaks at once. That is a lot of blast radius for housekeeping, and the scale-discipline rule says the measurement has to argue for the machinery. This one argues against it.

What was done instead: fix the sweeper in `scripts/sweep-build-artifacts.sh`, which was globbing `"$TMP".tmp*` and therefore only worked where TMPDIR ends in a slash, and make it report its count even when that count is zero.

Reversible. If mutation testing becomes routine rather than a weekly scheduled job, the redirect is still there to reach for.


### B-59 — A changed model is an ordinary re-embed, because search refuses to mix models at all

`accepted` · `dec_01KZXMFZH4V5TDJGPN96B1WBJ2`

Resolves TQ-3, 2026-08-13, which asked whether re-embedding after a model change should be a background full pass or lazy on access.

Neither, and the question turned out to be resting on a bug.

**The bug first.** The only guard on the vector scan was `length(embedding) = ?`, which catches a model that changed *dimension*. It cannot catch one that did not. Two 384-wide models produce vectors in unrelated spaces; the cosine between one model's document vector and another's query vector is a perfectly well-formed number that sorts into a plausible ranking and means nothing. Swapping `bge-small-en-v1.5` for any other 384-dimension model would have done this silently, and no strategy for *when* to re-embed would have helped, because the corpus is mixed for the whole duration of any strategy.

So the semantic query filters on `embedding_model` as well as width, and the embedder is now required even when the query vector was computed elsewhere — it is what names the model, and without the name there is no way to know which stored vectors this one may be compared against. No embedder means no semantic results rather than a guess.

**Then the strategy, which mostly dissolves.** "Missing" is redefined as *has no passages from the model now configured*. Changing the model makes every live document missing, and `keel reembed --missing` — the command that already exists, for the case that already existed — rebuilds them. One definition, one command.

A **background full pass** was rejected for the reasons scale discipline gives: it is a background worker, it writes on a schedule nobody asked for, it competes for the single write path, and it makes the first start after an upgrade slow and surprising. There is one user and a few hundred documents; the pass takes 29 seconds and a person can run it.

**Lazy on access** was rejected because it writes during a read, and because it leaves the corpus permanently half in one space and half in another — with the model filter now in place, that means recall silently depends on what happened to be searched recently, which is the least predictable failure of the three.

What makes the explicit choice safe is that nothing is silent. `passages_from_mixed_models` in `fsck` and `passage_index` in `doctor` both report a split corpus with the remedy, and the model filter means the stragglers are *absent* from results rather than wrong in them. The failure mode is missing rows that something is complaining about, not present rows nobody can question.

One consequence worth stating: between changing the model and finishing the pass, semantic search returns only what the new model has embedded, and the keyword half carries the rest. That is a visible, temporary degradation with a command that ends it, which is the trade this project makes every time.


### B-60 — Say what the write path actually protects, and put an advisory lock on the store

`accepted` · `dec_01KZXMVTV13AA2M09Y84NMG9HT`

Resolves TQ-36 and the untitled duplicate beside it, 2026-08-13. Both asked the same thing: hard constraint 1 says the daemon owns the single write path, DuckDB used to enforce that and SQLite does not, so is it a rule or a convention now.

**Both halves, and TQ-36 was right about the first.** The constraint's value was never the exclusivity — six of the seven steps in a Keel write have nothing to do with locking, and they are the reason one place has to know how to write. So the constraint is reworded to say what is actually protected: everything that writes goes through `keel-core`'s write path. A contract claiming an enforcement the engine no longer provides is worse than one describing what is true.

**But rewording alone would have permitted what happened today.** I started a second daemon against the live store by accident — `--bind` and `--embeddings` passed, `--home` forgotten — and it applied a schema migration while the first daemon was serving. That process was not a rogue writer skipping the five steps. It went through `keel-core` correctly, and the reworded constraint would have blessed it. It was a legitimate writer that should not have been a second one, and nothing in the system had anything to say about it. The migration guard could not: it refuses a binary *older* than the store, and this was newer.

So the second half: **opening the store for writing takes an advisory lock on it.** The daemon holds it for its lifetime; the CLI takes it for the length of a direct write. A second acquirer fails immediately with a message naming what holds it, instead of succeeding and being discovered later by a health field that happened to disagree.

**TQ-36's objection to a lock file no longer stands, and it is worth being precise about why.** It said "a stale lock after a crash is a store nobody can open, which is worse than the problem". That is exactly right for a PID file or a claimed row in a table, and exactly wrong for an OS advisory lock, because the kernel releases it when the file descriptor closes — including on `SIGKILL`, panic and power loss. Measured rather than assumed:

```
--- while the holder is alive ---
REFUSED — still held: "WouldBlock"
--- after SIGKILL of the holder ---
ACQUIRED — the lock was free
```

There is no stale-lock failure mode to weigh, so the option TQ-36 rejected on that ground comes back on the table having lost its only real cost.

`std::fs::File::try_lock` does this with no new dependency, which settles the "lock file or health probe" half of the duplicate: neither a hand-rolled file nor a probe. The probe stays where it is useful — it is what the CLI consults to decide whether to *ask the daemon instead of writing*, which is a different question from whether writing is safe, and it is advisory in the sense that a second daemon never thinks to ask.

Two costs, both accepted. `rust-version` moves from 1.85 to 1.89, which is when that API stabilised; this is a personal project on current stable. And advisory locks are unreliable on network filesystems — `doctor`'s location check already warns when the store sits in a synced or network folder, which is the same population.

Not attempting the third option TQ-36 listed, enforcement in the type system. The CLI legitimately writes when no daemon is running, so the capability cannot be daemon-only, and a runtime lock expresses "one at a time" directly where a type would have to express it by proxy.


### B-61 — Explicit writes only, and adopting an existing project is a two-layer backfill

`accepted` · `dec_01KZYASB3PX1BXA4Y37VP0D4XD`

Resolves Q-6, 2026-08-13. KB chose the hybrid, for people adopting Keel on a project that already exists.

**The question's own exception has gone.** Q-6 recorded the working assumption as "explicit writes only, except the GitHub webhooks in SPEC §9". Those webhooks were dropped on 2026-08-11 (KEEL-45, `wont_do`) because there is no git remote — the integration was specified for a world this project does not live in. So Keel has no automatic ingestion at all, and the only path ever planned was deliberately removed. Explicit-only is not a choice being made here so much as a fact being written down.

**Backfill does not reopen it.** What Q-6 is about is *unattended* ingestion: something watching files, receiving webhooks, scraping commits, writing without anyone asking. A backfill is the opposite — a person runs it, once, on purpose. The property that matters is who initiates, not how many rows land. Three hundred artifacts written because somebody typed a command are more explicit than one row written by a webhook nobody remembered configuring.

**Two layers, because the two halves have different failure modes.**

*Mechanical.* `keel import` already does this: markdown files become versioned documents, idempotent, recording `mirror_path` so `keel generate` puts them back where they came from. Nothing is inferred — a file *is* a document. This is most of the volume and none of the risk. Its one gap is that it cannot be previewed, which matters more than it sounds: soft-delete-only means a bad backfill leaves permanent sediment, archived but present forever.

*Judged.* Tasks, decisions, glossary terms and milestones are not sitting in files waiting to be parsed. Deciding that an ADR is a decision, that a heading is a spec, or that a `TODO` is a task is a judgement, and a parser making it at scale gets it wrong at scale. That half is Claude reading the repository and writing through the MCP surface that already exists — no new code, and better at the only part that is hard.

**The hybrid's provenance is honest by construction, which is the strongest argument for it.** A parser writing hundreds of rows would need a way to mark them as derived rather than asserted — `Actor::System` exists for that and is currently unused — because Keel's whole value is that "we decided X" comes with who and when, and unattributed rows that look attributed dilute it by exactly their volume. The hybrid needs none of that machinery: imported documents carry `Actor::Human` truthfully, because a person wrote the file, and judged rows carry a real `session_id` because a real session made the call. Nothing is fabricated, so nothing needs flagging as fabricated.

**The risk that remains is volume, and it is the one Q-6 named.** `write-amplification` in its own body. A naive backfill turns every `TODO` into a task and every heading into a spec, and the digest — budgeted at 3–4k tokens and already over budget on this project — is the first thing every session reads. A backfilled project can be worse than an empty one. The standing instructions already say a project with forty trivial tasks that should be eight is worse than useless; the backfill workflow has to say consolidate, not transcribe, and be judged on what it left out.

Not doing: a `keel backfill <repo>` command that infers everything. It is the version that sounds most finished and is worst at the part that matters, and it would need the provenance machinery the hybrid avoids.


### B-62 — Spec decisions that outlive their reasoning are annotated, not rewritten

`accepted` · `dec_01KZYC1V6NV3H9EVNPVAHEECRJ`

Resolves TQ-37, 2026-08-13. KB chose annotation, which was the recorded recommendation.

Six rows of SPEC §13 — D-1a, D-2, D-2b, D-4, D-5 and D-6 — argued from DuckDB and Lance, which left the tree in Phase 9. Every one of them still reaches the right conclusion; what expired was the reasoning underneath. Each row now keeps the rationale it was decided on, with the dead clause struck through and what replaced it named beside it, in the pattern D-1 already set.

Not rewritten, and the reason is D-4. It chose recursive CTEs over DuckPGQ because DuckPGQ could not run on DuckDB 1.5.x alongside Lance — a constraint that no longer exists in a tree containing neither. Then the Phase 9 survey ruled out Turso for not supporting recursive CTEs at all. The conclusion did not merely survive its rationale being replaced, it got stronger. Rewrite the row to argue from SQLite and that disappears; a reader learns what is true and not that the decision was load-bearing for a reason nobody anticipated.

D-6 is the one worth being blunt in. "Storage engines are Rust-native" is false — SQLite is a C amalgamation compiled into the binary and the embedding path reaches ONNX Runtime, which is C++. The property actually wanted was nothing to install and nothing running beside the binary, which §2 now argues directly rather than through the language. Saying that plainly is better than deleting the sentence and leaving the conclusion looking unexamined.

The standing note under the table changed with them. It used to say the rows were left alone pending KB's agreement, which stopped being true the moment the agreement arrived.

Rewriting the rationales was rejected for the reason KEEL-132 was told not to: it would make the spec read as though it had always said SQLite. Leaving them was rejected because a reader who reads only the table is misinformed, and the blockquote that was carrying the correction is easy to miss.


### B-63 — The keel-github stub comes out of the tree; SPEC §1.1 stays as the intended layout

`accepted` · `dec_01KZYFS0PJY5RPXCMN15GC2AS7`

### The keel-github stub comes out of the tree

`crates/keel-github` was 24 lines: a `main.rs` that printed `keel-github: not yet implemented — Phase 4`, a Cargo.toml, no lib target, no dependents, and a `tempfile` dev-dependency for tests that were never written. Its own doc comment said it existed "so §1.1's layout is real and so nothing drifts into the daemon that belongs here".

It is removed, along with its workspace member entry.

#### Why the stated reason did not hold

The first half — making §1.1's layout real — is the part that looked like it should block this. It does not, because the layout in §1.1 is not a description of the tree. That same diagram lists `apps/web/`, which has never existed as a directory in this repository. So the diagram was already the intended shape rather than the current one, and `keel-github` was the odd member: the only planned component with an empty crate standing in for it. Removing it makes the two consistent, and §1.1 goes on meaning what it already meant.

The second half — stopping webhook logic drifting into the daemon — was doing nothing a compiler enforces. An empty crate does not repel code; §9 saying the receiver is a separate binary that calls the daemon's API is what does that, and it still says it.

#### What it cost to keep

It compiled on every `cargo build --workspace` and `cargo test --workspace`, and produced a test binary that ran zero tests. Small, but it was pure overhead against a component nobody has started.

#### When Phase 4 arrives

`cargo new` costs nothing. The spec still names the crate, its job and its deployment shape in §1.1 and §9, which is the whole of what the placeholder was preserving.


### B-64 — The write-ahead log stays on SQLite's defaults, unwatched

`accepted` · `dec_01KZZGS6S1DC5T05KFQY6KQCFT`

### The write-ahead log stays on SQLite's defaults, unwatched

Nothing is added to monitor or manage the WAL. No check in `doctor`, no `journal_size_limit`, no background checkpoint loop. `wal_autocheckpoint = 1000` and the TRUNCATE on daemon shutdown are the whole of it, as before.

#### What prompted the question

`Store::wal_pages()` is documented as the number that says whether checkpointing is keeping up, and only tests call it. The failure it describes is real and genuinely nasty: SQLite cannot checkpoint past the oldest open read snapshot, the daemon runs for days holding a server-sent-events connection, and a reader that never releases would pin the log so it grows without bound while every query keeps answering correctly out of it. No error, no failed request.

#### Why nothing is being built

The hazard is real but has not been observed, and three things already stand between it and us. `wal_autocheckpoint = 1000` handles the ordinary case. `await_holding_lock = "deny"` in the workspace lints forbids the coding mistake most likely to cause it — a guard held across an await. The daemon checkpoints with TRUNCATE on clean shutdown.

The evidence says it is working: at the time of asking, the live store carried a 3 MB log against a 7.2 MB database. That is the mechanism doing its job, not a symptom.

Adding a background timer to watch for something that has never happened is exactly what the scale-discipline rule exists to stop. One user, a few thousand rows, and a measurement that says the current arrangement is fine.

#### What was learned along the way, and where it lives

Four measurements sit on KEEL-191, and they are the reason closing this costs nothing:

- `PRAGMA wal_checkpoint(PASSIVE)` is not a read. It cost 7 ms on a 606-page log against 1.32 µs for a `stat` of the `-wal`, so anything calling it on a cadence puts real work on the write path and quietly replaces the checkpoint policy it meant to observe.
- A checkpoint never truncates the file. The next write after one does, and only when `journal_size_limit` is set. So file size is a high-water mark and not a current reading.
- A pinned snapshot shows as `checkpointed == 0` while `busy` stays `0`. The obvious column to check is the wrong one, and a monitor written against `busy` would never fire while reading as success.
- Retrying PASSIVE does not help. Three consecutive attempts moved zero pages, then all 304 the moment the reader released.

If a `-wal` is ever found larger than the store beside it, the diagnosis is already written down and reopening this is cheap.

#### Reversible

Nothing was built, so there is nothing to unwind. The argument for revisiting would be an actual observation — a log that does not come back down — rather than the theoretical possibility, which is what was on the table this time.


### B-65 — Keel ships as a product: Apache-2.0, 0.x, and the Claude Code plugin as the front door

`accepted` · `dec_01KZZJAD9639CB9V0Q4PJY8ZGT`

*No reasoning recorded.*

### B-66 — Updates apply themselves when compatible, and stop and ask across a schema change

`accepted` · `dec_01KZZJATJZSJWEZ6ASKN0BFWQR`

*No reasoning recorded.*

### B-67 — Phase 10 runs after Phase 11, drops Windows, and plans for a release cadence that starts fast and slows down

`accepted` · `dec_01KZZPATJ8RNYQ573W4701AK63`

*No reasoning recorded.*

### B-68 — Mutation testing comes out of CI until there is traction worth protecting

`accepted` · `dec_01M00V2TMZR4HY008QJMCXE8YG`

*No reasoning recorded.*

### B-69 — Serving a read-only page does not touch hard constraint 7, the repo stays private for now, and the package becomes keel

`accepted` · `dec_01M00Y20C7Z39KK7QESF7VM0F9`

*No reasoning recorded.*

### B-70 — One package owns both binaries, because dist builds one installer per package

`accepted` · `dec_01M010PFT9N71ZDB2EZ1BWV5Z4`

#### Decision

`keel-cli` is renamed `keel` (this is B-69's half of it) and now declares **both** shipped binaries. `crates/keel/src/bin/keel-daemon.rs` is a three-line shim over `keel_daemon::run`, and `keel-daemon` sets `[package.metadata.dist] dist = false`.

#### Why

`dist` names an installer after the package that owns the binaries, and treats every package with binaries as a separate app. Run against the workspace as it was, `dist plan` announced two apps and two installers — `keel-cli-installer.sh` and `keel-daemon-installer.sh`.

Two things in the tree say that is wrong. PHASE-10 §1 advertises one URL ending `keel-installer.sh`, and `scripts/verify-release-tier1.sh` checks that running **one** installer leaves both `keel` and `keel-daemon` on disk. Two installers satisfies neither, and a user who ran only the advertised one would get a CLI with no daemon behind it.

There is no setting for this. `binaries` in the dist config is a per-platform override, not a way to pull another package's binaries into an archive. So the package boundary had to move.

#### What moved, and what did not

Only the entry point. `crates/keel-daemon/src/main.rs` became `crates/keel-daemon/src/run.rs` with `pub fn run()`, so the argument parsing, the bind refusal and its three unit tests all stay in the crate they are about. What crossed the boundary is a `fn main` calling one function.

The two integration tests that drive the real process — `end_to_end.rs` and `wont_restart_loop.rs` — did have to move to `crates/keel/tests/`, because `CARGO_BIN_EXE_keel-daemon` only resolves in the package that declares the binary. Nothing in them changed.

#### The cost

`cargo build -p keel` now builds axum and tokio. A workspace build was doing that anyway, and the `keel` binary references none of it so the linker drops it, but a single-package build of the CLI is slower than it was.

#### Rejected

Publishing under the names `dist` picks and correcting §1's URL. It reads as the smaller change and is not: it leaves the user running two installers to get one product, and it would have meant rewriting the tier-1 check to expect that rather than fixing what the check was right about.


### B-71 — The installer refuses a download it cannot verify, rather than skipping the check

`accepted` · `dec_01M010PZ4GM1Q2NS41KPJJEZAS`

#### Decision

`scripts/patch-installer.sh` rewrites the sha256 block in the installer `dist` generates. It tries `sha256sum`, falls back to `shasum -a 256`, and **errors** when neither is on the path. Upstream returns success in that last case.

The release workflow runs it after building the installer and before attesting, so the provenance statement covers the bytes people download.

#### The correction PHASE-10 needs

§10 says stock macOS has no `sha256sum`. Measured on 2026-08-14 that is half right, and the half that is wrong changes where the bug bites.

This machine ships `/sbin/sha256sum` — an Apple binary, universal with an arm64e slice, dated June 2026. So on a current macOS with the default path the check does run.

It skips everywhere else on macOS:

- Older macOS, which has it nowhere.
- Any restricted path. `scripts/verify-release-tier1.sh` runs the installer under `env -i PATH=/usr/bin:/bin` precisely to prove a machine with no toolchain can install — and `/sbin` is not on it.

So the tier the release gate leans on is the tier where integrity checking does nothing. Demonstrated directly: the unpatched installer, on that path, accepted a file whose contents had been changed and exited 0.

`/usr/bin/shasum` is present in all of those cases.

#### Why error rather than skip

PHASE-10 §13 makes "the installer refuses a corrupted archive" an exit criterion. A check that could not run has established nothing about the bytes, so reporting success is a claim it has no basis for. Both target platforms carry one of the two commands, so the refusal only fires on a machine that has neither — where declining to install something unverified is the right answer.

#### Why a patch script and not configuration

The text is in `dist`'s own installer template. The choice was vendoring the whole template or a targeted rewrite; this is the second, with the fix to be sent upstream.

It fails loudly on text it does not recognise, and that is the part worth defending. A patch that silently does not apply is the same failure as the bug it fixes. If `dist` fixes this upstream, the release fails and somebody deletes the script — which is the outcome we want, arrived at by being told rather than by noticing.

`crates/keel/tests/installer_checksum.rs` covers it, including one test that pins the *unpatched* behaviour so the patch can be retired with evidence.


### B-72 — The repository stays private and macOS builds run on a self-hosted runner

`accepted` · `dec_01M01F8R621R79SSKGCV4D4G34`

#### Decision

The repository stays private. macOS builds — both targets — run on a self-hosted runner on KB's own Mac. Linux stays on GitHub's hosted `ubuntu-22.04`. This supersedes the answer given to the visibility question earlier the same day, which was to go public.

#### What overturned it

Going public was never wanted for its own sake. It was wanted because §2 requires macOS runners for the ad-hoc signature and those are free only on public repositories. The cost of getting them was never priced properly.

It was priced when the change was made, and it is higher than it looked:

- **Making a repository public emits a `PublicEvent` to every follower.** There is no setting that suppresses it — GitHub's controls are over what an account receives, not what it broadcasts. The account has sixteen followers and the event is recorded at `2026-08-14T14:20:47Z`. Reverting to private does not retract it, and the public event stream is mirrored off GitHub.
- **There is no unlisted tier.** Private or public, with `internal` for organisations only. Public means the profile listing, GitHub search, code search and crawlers.
- **Nothing written into Keel can be taken back out of the mirror.** Found while trying to remove a machine path before publishing: retracting a note works, but editing a task body reprints the old value in the changelog, because events are immutable and the changelog derives from them. That is KEEL-215, and until it is fixed "publish the mirror" and "redact anything" cannot both be true.

#### Why a self-hosted runner answers it rather than working around it

The requirement in §2 is Apple's linker, not GitHub's hardware. An Apple Silicon Mac satisfies it for both targets: `--target=x86_64-apple-darwin` on an arm64 host still links with `cc` and still gets the ad-hoc signature. §10 had already worked this out for the August 2027 Intel-runner retirement — the plan for then is the plan for now, arrived at early.

Self-hosted runners are free and unmetered. Linux stays hosted because it is billed at the base rate and because a second machine testing Linux is worth more than a saved minute — CI has never once tested Linux on anything but a hosted runner.

And the usual objection does not apply. A self-hosted runner on a *public* repository is dangerous, because a pull request from a stranger executes on the machine. On a private repository it is the ordinary supported pattern. Private is what makes this safe, so the two halves of this decision hold each other up rather than trading off.

#### What it costs, plainly

**The Mac has to be awake for a release, and for the macOS half of CI.** A job with no runner queues rather than failing, so the symptom of a sleeping laptop is a run that sits there — not an error. Worth knowing before it happens.

**The build machine is now the development machine.** This is the real cost and it lands on exactly the thing §12 tier 1 exists to protect: tier 1 runs the installer under `env -i HOME=<scratch> PATH=/usr/bin:/bin` precisely because the build machine has cargo, a real store and a running daemon, all of which make a broken release look fine. Building on that machine does not weaken the trick — the stripped environment is still stripped — but it removes the last accidental independence there was.

So tier 2, the Linux VM, matters more under this decision than it did before, not less. It is now the only verification that happens anywhere other than one Mac.

#### Rejected

Staying private and paying for hosted macOS minutes at ten times the Linux rate, at a cadence of "every few days". It is the option that changes nothing and bills for it.


