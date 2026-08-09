# Keel — Decision log

> Maintained by Claude Code. Every non-obvious choice made during development gets a row.
> Architectural decisions made *before* development are in `product/SPEC.md` §13 (D-1 … D-11, including D-2b — twelve rows) — those are settled and are not repeated here.

**Why this exists:** in six months neither KB nor a fresh Claude session will remember why a library was chosen or why an approach was abandoned. One line written now saves an hour of archaeology later. It's also the seed data for Keel's own `decisions` table at the Phase 1 dogfooding switch.

---

## Format

| ID | Date | Decision | Reasoning | Reversible? |
|---|---|---|---|---|

- **ID**: `B-1`, `B-2`, … (B for build-time, to avoid colliding with the spec's D-series)
- **Reasoning**: one or two sentences on *why*, including what was rejected. "Chose X" without "over Y because Z" is not useful.
- **Reversible?**: `yes` / `no` / `expensive`. If `no` or `expensive`, it should probably have been a question for KB first.

---

## Decisions

| ID | Date | Decision | Reasoning | Reversible? |
|---|---|---|---|---|
| B-1 | 2026-08-09 | **`chrono` for time, not `jiff`.** | `duckdb-rs` ships a first-class `chrono` feature with `ToSql`/`FromSql` for `TIMESTAMP`; there is no `jiff` feature. Choosing `jiff` would mean a hand-written conversion shim at every storage boundary — the exact place a timezone bug would hide — for no domain benefit. Recorded here because `product/CLAUDE.md` requires picking one and never mixing. | yes, painfully |
| B-2 | 2026-08-09 | **All Lance access goes through the DuckDB `lance` extension. The `lance` and `lancedb` Rust crates are not dependencies.** | Verified empirically (see P0-2 table): `ATTACH … (TYPE lance)` gives full `SELECT`/`INSERT`/`UPDATE` over Lance datasets, and the three search functions work. Using the extension means one connection, one SQL surface, one transaction story — and it drops `lance` v10 + `arrow` v59 from the build entirely. Rejected: the native crate, which would have meant marshalling Arrow record batches by hand and keeping two Lance versions in step. | yes — `DocumentStore` is a trait precisely so this can be swapped |
| B-3 | 2026-08-09 | **`duckdb` with the `bundled` feature.** | Compiles DuckDB from source, so the binary can never disagree with the extension versions it loads. Costs ~10 min on a cold build; CI caches it. Rejected: linking a system libduckdb, which makes the build depend on whatever Homebrew last installed. | yes |
| B-4 | 2026-08-09 | **No vector or FTS index on the Lance dataset initially — brute-force scan.** | Verified that `lance_fts`, `lance_vector_search` and `lance_hybrid_search` all return correct results with no index present. At a few thousand documents an index is pure cost. Per the scale-discipline rule, a measurement comes before an index. | yes |
| B-5 | 2026-08-09 | **`unwrap`/`expect`/`panic`/`todo`/`unimplemented` are workspace clippy lints at `warn`, promoted to errors by CI's `-D warnings`.** | The definition of done forbids these in library code. Encoding it as a lint means CI catches it; leaving it to review discipline means it lands. Tests and binaries opt out locally with `#[allow]` where genuinely warranted. | yes |
| B-6 | 2026-08-09 | **`missing_docs` is a workspace lint, not just a `keel-core` convention.** | The contract only requires doc comments on `keel-core` public items, but scoping the lint per-crate is more machinery than it saves, and documenting the daemon's public surface costs little. | yes |
| B-8 | 2026-08-09 | **`Surface` carries five values, not four: `chat \| cowork \| code \| ui \| cli`.** | SPEC §3.1's audit-block comment lists four; §6.5 separately names `cli` as a fixed sentinel for the command line. The two passages disagree and something had to give. Five is the reconciliation — `keel-cli` writes fixtures and restores backups, and those writes need an honest surface rather than a borrowed `ui`. The column is a bare `VARCHAR` with no check constraint, so this costs nothing at the storage layer. Raised with KB as TQ-8 rather than treated as settled. | yes |
| B-9 | 2026-08-09 | **All ULIDs are minted from a single process-wide *monotonic* generator, never `Ulid::new()`.** | Found by a test, not by reading: `Ulid::new()` re-randomises its low 80 bits every call, so two ids created in the same millisecond sort arbitrarily. SPEC §3.4 rests on ULID order *being* chronological order so that "changed since T" is a range scan over `events.id` — and a burst of writes inside one millisecond is an agent's normal behaviour, not an edge case. Non-monotonic ids would make an event-cursor query silently skip or repeat rows, which is the same class of quiet-wrong-answer bug as an inverted graph traversal. Rejected: ordering every query by `(created_at, id)`, which pushes the problem to every call site instead of solving it once. | yes |
| B-10 | 2026-08-09 | **Every table gets `idempotency_key`, not just `tasks`.** | SPEC §7.2 and PRD REQ-7 say *every* create is idempotent, but §3.2 only gives the column to `tasks`. Honouring the requirement means honouring it everywhere; the alternative silently drops idempotency for twelve of thirteen types, including `projects` — the one type where duplicates are called out as the failure that ruins the aggregate view (UC-8, REQ-8). Marked `PROVISIONAL` and raised as TQ-9, because adding a column is a storage-format change and those are KB's call. | expensive — it is a schema column |
| B-11 | 2026-08-09 | **Dev builds use `debug = "line-tables-only"` and `debug = false` for dependencies; the clippy gate drops `--all-features`.** | The vendored DuckDB's full debug info is enormous: `target/` reached **19 GB** and filled KB's disk mid-session (`ranlib: errno=28`). `--all-features` made it worse by building DuckDB a second time under a different feature set, while changing nothing — no workspace crate declares a feature. Line tables keep file-and-line in every backtrace, which is the part that matters; what is lost is stepping through DuckDB's C++ internals, which this project does not do. `product/CLAUDE.md`'s definition of done was amended to match. **Worth KB knowing separately: the machine is at 95% disk (327 GB of 373 GB used) independent of this repo.** | yes |
| B-7 | 2026-08-09 | **Build scope this stretch is Phases 0–3; git stays local with no remote; `session_id` is a skill-minted per-conversation ULID; unverifiable human phase gates get an automated proxy and an honest note in `product/STATUS.md`.** | All four confirmed directly by KB on 2026-08-09 before he went away. Q-8 in `product/QUESTIONS.md` moves from `open` to answered on the strength of the third. | n/a — KB's call |

---

## Findings from dependency verification (task P0-2)

Record what was actually true when checked, with the date and the source. `product/SPEC.md` was written in August 2026 from documentation, not from running code, and several claims sit on fast-moving ground.

| Claim in SPEC | Verified? | What's actually true | Date checked | Source |
|---|---|---|---|---|
| Lance extension available for current DuckDB | ✅ **yes** | DuckDB is at **1.5.5 (Variegata)**. `INSTALL lance; LOAD lance;` succeeds on `osx_arm64`. It is a core extension, not a community one. | 2026-08-09 | Ran it |
| `ATTACH … (TYPE lance)` syntax as written in §5 | ⚠️ **syntax wrong, capability right** | ATTACH takes the **directory that contains the datasets**, not a single `.lance` path. `ATTACH '~/.keel/lance' AS lancedb (TYPE lance)` exposes `lancedb.documents` and `lancedb.blobs`. §5 attached `…/lance/documents.lance`, which would resolve to `documents.lance/documents.lance` and fail. **§5 corrected in place.** Bonus finding: the attached tables support `INSERT` and `UPDATE`, not just `SELECT` — hence B-2. | 2026-08-09 | Ran it |
| `lance_hybrid_search()` signature and behaviour | ⚠️ **signature wrong, capability right** | Actual: `lance_hybrid_search(dataset_path, vector_column, query_vector, text_column, query_text, k := …, alpha := …, prefilter := …, nprobs := …, refine_factor := …, use_index := …, oversample_factor := …)`. §5 called it with three positional arguments. It also **returns the full source row** plus `_distance`, `_score` and `_hybrid_score`, so §5's join back to `lancedb.documents` on `doc_id` is unnecessary. `lance_fts(path, text_column, query)` and `lance_vector_search(path, vector_column, query_vector)` follow the same shape. **§5 corrected in place.** | 2026-08-09 | Ran it |
| DuckPGQ still incompatible with the Lance-supporting DuckDB line | ✅ **yes — confirmed harder than the spec claimed** | `INSTALL duckpgq FROM community` on 1.5.5/osx_arm64 returns **HTTP 404** — no build exists. D-4 stands, now on evidence rather than documentation. The contingency in §4 remains hypothetical; nothing to reconsider. | 2026-08-09 | Ran it |
| Quack status and timeline (§7.1) | ✅ **yes** | `quack` is a real, installable community extension for 1.5.5 ("The DuckDB 'Quack' Client/Server Protocol"), not installed here. D-5 explicitly does not depend on it and that remains correct — §7.1's argument is about the six non-locking write steps, which Quack does not touch. | 2026-08-09 | `duckdb_extensions()` |
| Current MCP spec version and transport model | ✅ **yes** | **2026-07-28 is current** and final (shipped on that date). Stateless confirmed: the `initialize`/`notifications/initialized` handshake and `Mcp-Session-Id` are both **removed**. Several details §6 does not mention and Phase 1 must implement — see "MCP deltas" below. | 2026-08-09 | [changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog) |
| MCP `Mcp-Method` / `Mcp-Name` header names (§6) | ✅ **yes** | Both REQUIRED on Streamable HTTP POST. `Mcp-Method` mirrors `method`; `Mcp-Name` mirrors `params.name`/`params.uri` and is required for `tools/call`. `MCP-Protocol-Version` is also required. Mismatch between header and body ⇒ HTTP 400 + JSON-RPC `-32020` (`HeaderMismatch`). | 2026-08-09 | [transport](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http) |
| DCR deprecated in favour of CIMD (§11) | ✅ **yes** | RFC 7591 Dynamic Client Registration is deprecated in favour of Client ID Metadata Documents, retained only for backwards compatibility. §11's wording is accurate. Phase 5 only, so no action now. | 2026-08-09 | changelog, "Deprecated" §4 |
| `fastembed-rs` version, model availability, ONNX dependency | ✅ **yes** | `fastembed` **5.17.4**, published 2026-07-28. Still ONNX Runtime via `ort`; still downloads the model on first run. `bge-small-en-v1.5` (384-dim) remains available. §5's two honest caveats against G8 both still hold. | 2026-08-09 | crates.io |

**Verdict: nothing invalidates the storage design.** SPEC §2–§5's architecture is sound as specified — one unified Lance `documents` dataset, attached into DuckDB, hybrid-searched in one SQL statement, all confirmed working. The two errors found were both *call syntax* in §5, which the handoff predicted, and both were fixed in place as editorial corrections. No escalation to KB was needed.

### MCP deltas — things §6 predates and Phase 1 must handle

§6 was written against the 2026-07-28 announcement rather than the finished specification. None of these change the tool surface; all of them change the daemon's wire handling.

| Delta | What Phase 1 must do |
|---|---|
| `server/discover` is a **required** RPC | Implement it. It advertises supported protocol versions, capabilities and identity. Clients may call it before anything else. |
| Every result carries a required `resultType` field | Emit `"complete"` on all nine tools. `"input_required"` is the MRTR path and Keel never needs it. |
| `tools/list` results require `ttlMs` and `cacheScope` | Keel's tool list is static, so a long `ttlMs` and `cacheScope: "public"` are correct and let clients stop polling. |
| Protocol metadata moved into `_meta` | Read `io.modelcontextprotocol/protocolVersion`, `/clientInfo`, `/clientCapabilities` from `params._meta`, and validate the version against the `MCP-Protocol-Version` header. |
| Error codes renumbered | `HeaderMismatch` `-32020`, `MissingRequiredClientCapability` `-32021`, `UnsupportedProtocolVersion` `-32022`. The old `-3200{1,3,4}` numbers are wrong. |
| HTTP GET/DELETE on the MCP endpoint | Return `405 Method Not Allowed`. Ignore any `Mcp-Session-Id` or `Last-Event-ID` that arrives. |
| `Origin` validation is a MUST | Reject invalid `Origin` with HTTP 403. Cheap, and the daemon is localhost-bound anyway. |
| Sampling, Roots and Logging are deprecated | Do not implement them. Keel needs none. |
| SSE resumability removed | The local REST/SSE surface for the desktop app is Keel's own API, not MCP, so this constrains only the MCP endpoint. |

The nine-tool surface in §6.2 is unaffected. So is `keel_context`.

---

## Reversals

When a decision turns out to be wrong, add a row here rather than editing the original. Knowing something was tried and abandoned is as useful as knowing what was chosen.

| Original | Date reversed | What replaced it | What we learned |
|---|---|---|---|
| — | — | — | — |
