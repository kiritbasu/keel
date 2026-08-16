<!-- specline:generated spec spc_01KZPJXC5RG006KJANQ6G4TBQS
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# Dependency and protocol verification — a snapshot, not a description

*Checked 2026-08-09. Nothing here has been re-verified since, and some of it is
already out of date by design — see the note at the end. Read it as "this is
what was true on the day the storage design was committed to", which is exactly
what it was written to answer.*

Moved out of `product/DECISIONS.md` on 2026-08-10, when that file became a
generated view of the decision rows. These tables are measurements rather than
decisions, so they could not live there — but they are the evidence behind
B-2, B-3, B-4, B-12 and B-17, and deleting them would have left five decisions
citing a verification nobody could read.

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

## What has changed since

Kept deliberately, because a dated snapshot that is quietly corrected stops
being a snapshot:

- **The tool surface is ten, not nine.** `keel_note` earned the tenth slot after
  the tracker moved to rows. Every "all nine tools" above should be read as "all
  tools".
- **`lance_hybrid_search` is no longer used at all.** B-12 moved BM25 into
  DuckDB's `fts` extension after the keyword half could not be characterised.
  The row above records that the function worked, which was true and is not the
  reason it was dropped.
- **Every MCP delta listed below was implemented**, and the daemon additionally
  serves 2025-11-25 (B-17), which this table did not anticipate because the
  finished specification made no mention of a client that would need it.
