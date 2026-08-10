<!-- keel:generated spec spc_01KZKMPVNTZAZHC9HY1TSNZNGM
     Keel is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `keel generate`. -->

# Keel — Technical Specification

> **Status:** Draft v1
> **Companion to:** PRD.md
> **Date:** 2026-08-09

---

## 1. Architecture

One Rust workspace. One daemon that owns the data. Everything else is a client.

```
                  ┌──────────────────────────────────────┐
   Claude chat ──►│                                      │
   Cowork      ──►│   MCP  (stateless HTTP, 2026-07-28)  │
   Claude Code ──►│                                      │
                  │            keel-daemon               │
   Tauri UI    ──►│   Local REST + SSE                   │
   CLI         ──►│                                      │
                  └───────────────┬──────────────────────┘
                                  │  sole write handle
                  ┌───────────────▼──────────────────────┐
                  │  keel-core                           │
                  │  domain · validation · provenance    │
                  │  embeddings · git mirror (gix)       │
                  └───────┬──────────────────┬───────────┘
                          │                  │
                  ┌───────▼──────┐   ┌───────▼──────────┐
                  │   DuckDB     │   │   Lance          │
                  │ entities     │   │ documents        │
                  │ links        │   │ blobs            │
                  │ events       │   │ (+ embeddings)   │
                  │ metrics      │   │                  │
                  └──────────────┘   └──────────────────┘
                        ▲ ATTACH ─────────────┘
                  one SQL surface over both
```

The Lance datasets are `ATTACH`ed into DuckDB as table namespaces, so a single SQL query can join a task to the spec revision that motivated it and rank by vector similarity in the same statement.

### 1.1 Workspace layout

```
keel/
├── crates/
│   ├── keel-core/      domain types, validation, storage, git mirror
│   ├── keel-daemon/    axum: MCP + local REST/SSE. Owns the write handle.
│   ├── keel-mcp/       MCP protocol layer (could fold into daemon)
│   ├── keel-cli/       thin client — scripting, backup, migration
│   └── keel-github/    GitHub App webhook receiver + PR linkage
├── apps/
│   ├── desktop/        Tauri v2 shell; daemon as sidecar
│   └── web/            same React bundle, served by daemon when remote
└── plugin/             Claude Code plugin: skill, hooks, MCP config
```

`keel-core` never opens a network socket and never knows about MCP. That boundary is what makes the CLI, the daemon, and any future surface cheap to add.

### 1.2 Why this shape

- **DuckDB and Lance are native Rust crates.** No sidecar database process, no ORM impedance, no cross-language marshalling on the hot path. This is the strongest argument for Rust here and it falls directly out of the storage choice. (Caveat: the embedding path in §5 does pull in ONNX Runtime through `ort`, so the stack is not literally FFI-free — but the *storage* layer is.)
- **DuckDB is single-process for writes.** One daemon owning the write handle turns that constraint into a design rule rather than a bug. See §7.
- **The Tauri app can't serve Claude chat.** Building daemon-first prevents logic getting trapped in the desktop app.
- **`gix` (gitoxide)** for git operations — pure Rust, no libgit2 linkage.

---

## 2. Storage split

| Lives in | What | Why |
|---|---|---|
| **DuckDB native tables** | Entity headers, links, events, metrics, environments | Mutable rows, frequent status flips, relational joins |
| **Lance `documents`** | Every prose body, every revision, embeddings | Append-only, needs hybrid search, needs format-level versioning |
| **Lance `blobs`** | Images, screenshots, attachments | Multimodal, large, lazily loaded |

**Rule:** anything whose access pattern is *update in place* goes in DuckDB. Anything whose pattern is *append a new version* goes in Lance.

To be precise about why, since it's easy to overstate: Lance *can* do row-level update and delete via deletion vectors, and its commits are optimistic — concurrent appends can collide at commit time and require a retry. The argument isn't that Lance can't mutate. It's that (a) frequent small status flips on a columnar layout are pessimal for read performance, (b) the documents corpus is genuinely append-only by nature so there's nothing to give up, and (c) Lance is the only store here that carries the vector index and multimodal blobs, so co-locating the searchable text with them avoids a sync.

Note also that revision semantics below are implemented in **user columns**, not Lance's dataset-level versioning. Lance dataset versions are a storage-level concern (they'll be used for snapshot/restore in §11); document revisions are a domain concept that needs to survive compaction and re-embedding. Don't conflate them.

### 2.1 The unified documents dataset

This is the highest-leverage decision in the spec. Rather than a Lance dataset per prose-bearing entity, there is **one** `documents` dataset that every entity's body writes into:

```
documents (Lance)
  doc_id          string   ULID, primary
  entity_type     string   'spec' | 'decision' | 'feedback' | 'design' | 'question'
  entity_id       string   FK to the DuckDB header row
  project_id      string   denormalised for filtering
  version         int32
  parent_version  int32?
  title           string
  body            string   markdown
  body_hash       string   content-addressed; identical revisions dedupe
  media_ref       string?  → blobs.blob_id
  status          string   'draft' | 'current' | 'superseded' | 'archived'
  author            string   'human' | 'claude'
  session_id        string?
  surface           string?  'chat' | 'cowork' | 'code' | 'ui' | 'cli'
  created_at        timestamp
  embedding         vector(384)
  embedding_model   string   e.g. 'bge-small-en-v1.5'
  embedding_version int32    bump to trigger re-embed
```

`entity_id` is a logical reference to a DuckDB header row. It is **not** an enforced foreign key — Lance cannot enforce it and DuckDB cannot see it. Referential integrity across the two engines is an application-level invariant, checked on write in `keel-core` and audited by `keel-cli fsck`.

Consequences:

- **One hybrid search covers everything.** "What do we know about onboarding?" is a single `lance_hybrid_search()` that returns spec sections, decisions, customer feedback, and design captions ranked together. With per-type datasets that's a five-way union and a manual re-rank.
- **Versioning is uniform.** One code path for revisions, diffing, and provenance regardless of artifact type.
- **Adding a prose-bearing type costs nothing** — no new dataset, no new index, no new search path.

Entity headers in DuckDB carry `current_doc_version`; the pointer is relational, the content is columnar. Every prose-bearing type — including `questions`, whose body is a document like any other — has that column.

---

## 3. Data model

### 3.1 Conventions

- **IDs:** ULID, prefixed by type — `prj_01H8…`, `tsk_01H8…`, `spc_01H8…`. Sortable by creation, unambiguous in agent output.
- **Timestamps:** UTC, microsecond precision.
- **Soft delete only.** `archived_at`. Agents make mistakes; hard deletes make them permanent. This applies to links too — `keel_link`'s remove operation sets `archived_at`, it does not `DELETE`.
- **Referential integrity is application-level.** DuckDB supports `FOREIGN KEY`, but `links` is polymorphic across thirteen tables and `documents` lives in Lance, so neither can be expressed as a constraint. `keel-core` validates on write; `keel-cli fsck` audits. Archive cascade is explicit: archiving a parent archives its links but never its children, and orphans surface in `fsck`.
- **Provenance vocabulary.** One concept, two shapes: entity tables record state (`created_by`, `updated_by`), the event log records the act (`actor`). They draw from the same value set — `human | claude | github | system` — and an entity's `updated_by` always equals the `actor` of the event that produced it.
- **Every table carries the audit block below**, written as `<audit>` to avoid repeating it thirteen times. The one deliberate exception is `events` (§3.4), which is append-only and immutable: it has no `updated_at`, no `version`, and no `archived_at`, because none of them can ever change.

```sql
-- <audit> expands to:
  created_at   TIMESTAMP NOT NULL,
  updated_at   TIMESTAMP NOT NULL,
  version      INTEGER NOT NULL DEFAULT 1,   -- optimistic concurrency
  created_by   VARCHAR NOT NULL,             -- 'human' | 'claude' | 'github' | 'system'
  updated_by   VARCHAR NOT NULL,
  session_id   VARCHAR,                      -- provenance unit, see §6.5
  surface      VARCHAR,                      -- 'chat' | 'cowork' | 'code' | 'ui' | 'cli'
  archived_at  TIMESTAMP
```

`created_by` and `session_id` are not optional garnish — G3 and REQ-2 are the whole provenance guarantee, and they live here.

### 3.2 DuckDB schema

```sql
CREATE TABLE projects (
  id             VARCHAR PRIMARY KEY,
  slug           VARCHAR UNIQUE NOT NULL,
  name           VARCHAR NOT NULL,
  description    VARCHAR,
  status         VARCHAR NOT NULL DEFAULT 'active',   -- active|paused|shipped|abandoned
  repo_urls      VARCHAR[],
  root_path      VARCHAR,                              -- local checkout, for the mirror
  aliases        VARCHAR[],                            -- disambiguation aid, see §6.4
  <audit>
);

CREATE TABLE milestones (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  kind           VARCHAR NOT NULL DEFAULT 'milestone', -- milestone|release
  name           VARCHAR NOT NULL,
  summary        VARCHAR,
  status         VARCHAR NOT NULL,                     -- planned|active|blocked|shipped|cut
  target_date    DATE,
  shipped_at     TIMESTAMP,
  version_string VARCHAR,                              -- releases only
  sort_order     INTEGER,
  <audit>
);

CREATE TABLE tasks (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  milestone_id   VARCHAR,
  kind           VARCHAR NOT NULL DEFAULT 'task',      -- task|bug|chore|spike
  title          VARCHAR NOT NULL,
  body           VARCHAR,                              -- short; long-form goes to a spec
  status         VARCHAR NOT NULL DEFAULT 'todo',      -- todo|in_progress|blocked|review|done|wont_do
  priority       VARCHAR DEFAULT 'p2',                 -- p0|p1|p2|p3
  labels         VARCHAR[],
  external_ref   VARCHAR,                              -- PR/issue URL
  closed_at      TIMESTAMP,
  idempotency_key VARCHAR NOT NULL,                    -- always derived if not supplied, §7.2
  <audit>
);
CREATE UNIQUE INDEX tasks_idem ON tasks(project_id, idempotency_key);

CREATE TABLE specs (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  kind           VARCHAR NOT NULL,                     -- prd|spec|rfc|design-doc|note
  title          VARCHAR NOT NULL,
  status         VARCHAR NOT NULL DEFAULT 'draft',     -- draft|review|approved|superseded
  current_doc_version INTEGER NOT NULL DEFAULT 0,      -- → documents.version
  mirror_path    VARCHAR,
  <audit>
);

CREATE TABLE decisions (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  title          VARCHAR NOT NULL,
  status         VARCHAR NOT NULL DEFAULT 'proposed',  -- proposed|accepted|superseded|rejected
  decided_at     TIMESTAMP,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path    VARCHAR,
  <audit>
);
-- Immutability of accepted decisions is enforced in keel-core, not by the schema:
-- keel_update rejects content changes where status='accepted'. Supersede instead.

CREATE TABLE questions (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  kind           VARCHAR NOT NULL DEFAULT 'question',  -- question|risk|assumption
  title          VARCHAR NOT NULL,
  status         VARCHAR NOT NULL DEFAULT 'open',      -- open|answered|accepted|mitigated|moot
  severity       VARCHAR,                              -- risks: low|medium|high
  resolved_at    TIMESTAMP,
  current_doc_version INTEGER NOT NULL DEFAULT 0,      -- body lives in documents
  mirror_path    VARCHAR,
  <audit>
);

CREATE TABLE terms (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR,                              -- NULL = global
  term           VARCHAR NOT NULL,
  definition     VARCHAR NOT NULL,
  aliases        VARCHAR[],
  mirror_path    VARCHAR,
  <audit>
);
CREATE UNIQUE INDEX terms_uniq ON terms(COALESCE(project_id, ''), term);
-- project_id NULL = global term. Per-project rows override a global of the same
-- name; resolution is project-first (PRD Q-4, provisionally resolved this way).
-- COALESCE in the index because a nullable column would let duplicate globals
-- through and make "the override" ambiguous.

CREATE TABLE feedback (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  kind           VARCHAR NOT NULL,                     -- interview|support|sales|idea|competitor|observation
  source         VARCHAR,                              -- who/where
  contact        VARCHAR,
  sentiment      VARCHAR,                              -- positive|neutral|negative|mixed
  occurred_at    TIMESTAMP,
  triaged        BOOLEAN DEFAULT FALSE,
  current_doc_version INTEGER NOT NULL DEFAULT 0,      -- verbatim body → documents
  <audit>
);

CREATE TABLE design_artifacts (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  name           VARCHAR NOT NULL,
  state          VARCHAR NOT NULL DEFAULT 'proposed',  -- proposed|approved|built
  figma_ref      VARCHAR,
  blob_id        VARCHAR,                              -- → Lance blobs
  current_doc_version INTEGER NOT NULL DEFAULT 0,      -- caption/rationale → documents
  <audit>
);

CREATE TABLE environments (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  name           VARCHAR NOT NULL,                     -- production|staging|preview
  url            VARCHAR,
  deployed_version VARCHAR,                            -- NOT current_doc_version — the
  deployed_commit  VARCHAR,                            -- shipped app version. Named
                                                       -- distinctly on purpose.
  status         VARCHAR DEFAULT 'unknown',            -- healthy|degraded|down|unknown
  last_deployed_at TIMESTAMP,
  <audit>
);

CREATE TABLE metrics (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  name           VARCHAR NOT NULL,
  unit           VARCHAR,
  target_value   DOUBLE,
  direction      VARCHAR DEFAULT 'up',                 -- up|down
  <audit>
);

CREATE TABLE metric_observations (
  id             VARCHAR PRIMARY KEY,
  metric_id      VARCHAR NOT NULL,
  project_id     VARCHAR NOT NULL,                     -- denormalised for filtering
  value          DOUBLE NOT NULL,
  observed_at    TIMESTAMP NOT NULL,
  note           VARCHAR,
  <audit>
);

CREATE TABLE artifacts (
  id             VARCHAR PRIMARY KEY,
  project_id     VARCHAR NOT NULL,
  name           VARCHAR NOT NULL,
  kind           VARCHAR,                              -- link|file|image|other
  url            VARCHAR,
  blob_id        VARCHAR,
  <audit>
);
```

### 3.3 Links — the graph

```sql
CREATE TABLE links (
  id          VARCHAR PRIMARY KEY,
  project_id  VARCHAR,
  from_type   VARCHAR NOT NULL,
  from_id     VARCHAR NOT NULL,
  rel         VARCHAR NOT NULL,
  to_type     VARCHAR NOT NULL,
  to_id       VARCHAR NOT NULL,
  anchor      VARCHAR NOT NULL DEFAULT '',  -- e.g. 'REQ-4'; '' means whole-entity.
                                            -- NOT NULL so the unique index actually
                                            -- fires — a nullable column would make
                                            -- every ordinary edge distinct.
  note        VARCHAR,
  <audit>
);
CREATE UNIQUE INDEX links_uniq ON links(from_id, rel, to_id, anchor);
CREATE INDEX links_from ON links(from_id);
CREATE INDEX links_to   ON links(to_id);
```

**Relations and their canonical direction.** Direction is normative — every traversal in §4 depends on it:

| Relation | Reads as | Example |
|---|---|---|
| `implements` | from **implements** to | task → spec `REQ-4` |
| `blocks` | from **blocks** to | task A → task B (A must finish first) |
| `depends_on` | from **depends on** to | inverse of `blocks`; stored, never both |
| `supersedes` | from **supersedes** to | decision v2 → decision v1 |
| `derived_from` | from **derives from** to | spec → feedback |
| `resolves` | from **resolves** to | decision → question, PR → task |
| `references` | from **references** to | anything → anything |
| `duplicates` | from **duplicates** to | task → task |
| `informs` | from **informs** to | feedback → spec |

`blocks` and `depends_on` are inverses. `keel-core` normalises on write: everything is stored as `blocks`, and a `depends_on` request is written with the endpoints swapped. Storing both directions is the single easiest way to make the graph queries silently wrong.

### 3.4 Events

```sql
CREATE TABLE events (
  id           VARCHAR PRIMARY KEY,     -- ULID = chronological
  project_id   VARCHAR,
  entity_type  VARCHAR NOT NULL,
  entity_id    VARCHAR NOT NULL,
  action       VARCHAR NOT NULL,        -- created|updated|status_changed|linked|revised|archived
  field        VARCHAR,
  before       JSON,
  after        JSON,
  actor        VARCHAR NOT NULL,        -- human|claude|github|system
  session_id   VARCHAR,
  surface      VARCHAR,
  summary      VARCHAR,                 -- one-line, human-readable
  meta         JSON,                    -- e.g. {"confirmed_by":"human"} for §6.4
  created_at   TIMESTAMP NOT NULL
);
CREATE INDEX events_project_time ON events(project_id, created_at);
```

Append-only, never updated. Because ULIDs sort chronologically, "what changed since T" is a range scan.

**This is only true if the ULIDs are generated monotonically.** A plain ULID re-randomises its low 80 bits on every call, so two ids minted inside the same millisecond sort arbitrarily against each other — and a burst of writes inside one millisecond is what an agent doing normal work looks like. `keel-core` therefore mints every id from one process-wide monotonic generator (DECISIONS B-9). Without it, a cursor-based `keel_activity` query silently skips or repeats rows.

---

## 4. Graph layer

**Decision: recursive CTEs now. Not DuckPGQ, not FalkorDB.**

The blocking fact: **DuckPGQ is not available for DuckDB 1.5.x** — it currently requires pinning to 1.4.4. The Lance extension lives in 1.5.x. You cannot have both today, and Lance is load-bearing while DuckPGQ is convenience. DuckPGQ is also explicitly a CWI research project with features under development.

FalkorDB is the wrong shape for a different reason. There *is* a Rust rewrite of the engine underway (`falkordb-rs-next-gen`), but the shipping product is still a server with Redis-module lineage, and `falkordb-rs` is a *client*, not an embeddable library. That's a third datastore and a third process to run, back up, and keep consistent, for a graph that will have a few thousand edges. GraphBLAS sparse-matrix algebra is built for problems several orders of magnitude larger than this.

Recursive CTEs over `links` handle every query the PRD asks for, in microseconds at this scale.

**Direction matters more than depth.** `implements` runs *task → spec*, so the traceability query for "what implements this spec" traverses **inbound** edges (`to_id → from_id`), not outbound. Getting this backwards returns an empty set that looks like a legitimate "nothing links here."

```sql
-- UC-7: what implements this spec, and what do those things depend on.
-- Inbound on `implements`/`references`, then outbound from whatever we find.
WITH RECURSIVE trace AS (
    SELECT l.from_id AS id, l.from_type AS type, l.rel, l.anchor,
           1 AS depth, [$root, l.from_id] AS path
    FROM links l
    WHERE l.to_id = $root
      AND l.archived_at IS NULL
      AND l.rel IN ('implements','references','derived_from')
  UNION ALL
    SELECT l.from_id, l.from_type, l.rel, l.anchor,
           t.depth + 1, list_append(t.path, l.from_id)
    FROM links l
    JOIN trace t ON l.to_id = t.id
    WHERE l.archived_at IS NULL
      AND t.depth < $max_depth                    -- default 6, hard cap 16
      AND NOT list_contains(t.path, l.from_id)    -- cycle guard
)
SELECT * FROM trace;

-- What is transitively blocking this task.
-- `blocks` is stored from=blocker → to=blocked, so blockers are found on to_id.
WITH RECURSIVE blockers AS (
    SELECT l.from_id AS id, 1 AS depth, [$root, l.from_id] AS path
    FROM links l
    WHERE l.to_id = $root AND l.rel = 'blocks' AND l.archived_at IS NULL
  UNION ALL
    SELECT l.from_id, b.depth + 1, list_append(b.path, l.from_id)
    FROM links l
    JOIN blockers b ON l.to_id = b.id
    WHERE l.rel = 'blocks' AND l.archived_at IS NULL
      AND b.depth < $max_depth
      AND NOT list_contains(b.path, l.from_id)
)
SELECT t.*, MIN(b.depth) AS depth
FROM tasks t JOIN blockers b ON t.id = b.id
WHERE t.status NOT IN ('done','wont_do') AND t.archived_at IS NULL
GROUP BY t.*          -- not DISTINCT: a task reachable by two paths of different
ORDER BY depth;       -- length would otherwise return twice
```

Note `depends_on` never appears in a traversal — §3.3 normalises it to `blocks` on write, so there is exactly one direction to reason about.

`keel-core` exposes three storage traits — `EntityStore` (DuckDB entities, links, events), `DocumentStore` (Lance revisions, blobs, embeddings, search) and `GraphStore`, the last with `neighbours(id, direction, rels, depth)` so callers never hand-write traversal direction. Every one of these queries is wrong in a way that returns plausible empty results, which is the worst failure mode available; centralising them means getting it right once.

**Contingency, honestly scoped.** If DuckPGQ ships for 1.5.x and coexists with Lance, adopting it is *not* purely additive. SQL/PGQ edge tables bind to vertex tables by key, and `links` is polymorphic across thirteen tables with no vertex table and no declared FKs. Adoption would require either a unified vertex view (`UNION ALL` over all thirteen with a discriminator — workable, and worth defining up front as `v_entities` regardless) or one edge-table definition per type pair (not workable). The `GraphStore` trait keeps the swap contained to one implementation, but budget it as real work, not a config change.

---

## 5. Search

Hybrid: **BM25 in DuckDB, vectors in Lance, fused by reciprocal rank in `keel-core`.**

> **Corrected 2026-08-09 against running code.** This section originally delegated both halves to `lance_hybrid_search()`. That function's keyword half does not behave predictably on multi-term queries — `"onboarding metering"` matched a document containing only *metering*, while `"onboarding slow"` matched nothing despite a document containing *onboarding*. The extension documents only single-word examples and no way to build the index that would presumably fix it. Rather than build retrieval on a function whose semantics cannot be stated, BM25 moved to DuckDB's `fts` extension, where they can. See DECISIONS B-12 and QUESTIONS TQ-10.

- `fts_entities` (DuckDB, BM25) — **every** searchable artifact. Prose titles and bodies are joined in from the Lance dataset when the index is rebuilt, so a spec and a task compete in one ranking rather than in two result sets that then have to be reconciled.
- `lance_vector_search()` — the semantic half, over the embeddings.
- Reciprocal-rank fusion in `keel-core` — because BM25 scores and vector distances are not on comparable scales, so fusing on *rank* is the only defensible merge. A hit found independently by both is the strongest signal available.

The DuckDB FTS index is a **snapshot**: it does not track inserts. An entity created after the last build is silently unfindable, which is the same shape of failure as an inverted graph traversal. The index is therefore rebuilt whenever the event log's high-water mark has moved.

The Lance datasets are attached under a namespace. **ATTACH takes the directory that holds the datasets, not an individual dataset path** — `ATTACH '${KEEL_HOME}/lance' AS lancedb (TYPE lance)` exposes both `lancedb.documents` and `lancedb.blobs` as ordinary joinable relations, from the one attach. The search *functions*, by contrast, take the individual dataset path. Both facts were verified against DuckDB 1.5.5; an earlier draft of this section attached `…/lance/documents.lance`, which resolves to `documents.lance/documents.lance` and finds nothing.

The attached relations are writable — `INSERT` and `UPDATE` both work — so `keel-core` needs no separate Lance client library. See DECISIONS B-2.

DuckDB's `?` placeholders are strictly positional; named parameters use `$name`, which is what `keel-core` binds:

```sql
-- lance_hybrid_search(dataset_path, vector_column, query_vector,
--                     text_column, query_text, k := …, alpha := …, …)
-- It returns every column of the source row, plus _distance, _score and
-- _hybrid_score — so there is nothing to join back to. An earlier draft
-- called it with three positional arguments and joined on doc_id; both
-- were wrong.
SELECT s.entity_type, s.entity_id, s.title, s.body,
       s._hybrid_score AS score, p.name AS project
FROM lance_hybrid_search(
       '${KEEL_HOME}/lance/documents.lance',
       'embedding', $embedding,
       'body',      $query,
       k := $k_inner
     ) s
JOIN projects p ON p.id = s.project_id
WHERE s.status = 'current'
  AND ($project_id IS NULL OR s.project_id = $project_id)
  AND ($types      IS NULL OR list_contains($types, s.entity_type))
  AND ($since      IS NULL OR s.created_at >= $since)
  AND ($until      IS NULL OR s.created_at <  $until)
ORDER BY score DESC
LIMIT $k_outer;
```

`lance_fts(path, text_column, query)` and `lance_vector_search(path, vector_column, query_vector)` take the same shape and return the same score columns. None of the three requires an index to be built first — they fall back to a scan, which at Keel's scale is the right default (DECISIONS B-4).

`k_inner` is set to `k_outer * 4` by `keel-core` so post-filtering doesn't starve the result set — retrieving exactly `k` from the index and then filtering by project and date is a classic way to return three results when forty exist.

**Coverage.** The `documents` dataset covers `spec | decision | feedback | design | question`. Types without prose bodies get a DuckDB FTS index over their text columns:

| Index | Covers | Fields |
|---|---|---|
| `documents` (Lance, hybrid) | spec, decision, feedback, design, question | title + body + embedding |
| `fts_entities` (DuckDB, BM25) | task, milestone, term, environment, artifact, project | title/name + short body + definition |

Together these satisfy REQ-4's "every artifact type that carries text." `metric` and `metric_observation` are deliberately excluded — they're numeric, and searching them is a filter, not a query. `keel-core` fuses the two result sets with reciprocal-rank fusion and re-ranks.

**Embeddings.** Start with a local model via `fastembed-rs` (`bge-small-en-v1.5`, 384-dim). Two honest caveats against G8's "runs entirely locally": the model is downloaded from the Hub on first run, and it executes through ONNX Runtime (`ort`), a C++ dependency. After first-run setup it is fully offline, ~50ms per embed, and good enough for a corpus this size. Store `embedding_model` and `embedding_version` on each document so upgrading is a background re-embed of stale rows rather than a rewrite. Resolves PRD Q-7 in favour of local, reversibly.

---

## 6. MCP surface

Built on the **2026-07-28 spec**: stateless request/response, no sessions or handshakes, `Mcp-Method`/`Mcp-Name` headers — **and, in practice, 2025-11-25 as well.**

> **Corrected 2026-08-09 against a real client.** Claude Code 2.1.185 opens with the legacy `initialize` handshake and declares `2025-11-25`. A daemon serving only the current revision reports "Failed to connect" and is unusable with the client this product exists for. Both revisions are served: the handshake is answered, the mirrored headers are required only of a 2026-07-28 caller, and `resultType` is sent only to clients whose revision defines it. See DECISIONS B-17 and QUESTIONS TQ-11. This is genuinely simpler to implement than the older stateful transport, and it means the daemon can be put behind a plain load balancer later with no changes.

### 6.1 Design principles

- **Few tools, rich arguments.** Nine tools, not forty CRUD endpoints. Models choose correctly among nine.
- **Every write returns the resulting entity.** No confirmation read.
- **Idempotency keys on every create.** Agents retry; you do not want three copies.
- **Optimistic concurrency on every update.** Pass the `version` you read; stale writes are rejected with the current state attached so the agent can merge.
- **Never truncate silently.** Every list is paginated and carries an explicit `truncated` flag with a total count. A small number of fields are declared *unbounded* (§6.3) — those are never cut, and the budget is absorbed elsewhere. The invariant is that the response always tells the truth about what it left out, not that everything is capped.

### 6.2 Tools

Every tool accepts three ambient arguments in addition to those listed: `session_id` (optional, §6.5), `surface` (optional), and — on writes — `idempotency_key` (optional, derived if absent, §7.2). They're documented once here rather than repeated per tool.

| Tool | Purpose |
|---|---|
| `keel_context` | **The entry point.** Compact digest for a project (or all projects). |
| `keel_search` | Hybrid search across types and projects. |
| `keel_get` | Fetch entities by ID, at a specific `version` if given, optionally with linked neighbours to depth N and an optional `diff_against` version. Satisfies REQ-2's diff requirement at the API layer, not only in the UI. |
| `keel_create` | Create any entity type. Typed argument union. |
| `keel_update` | Update with optimistic concurrency. Includes status transitions. |
| `keel_write_doc` | Append a new revision to a prose document. |
| `keel_link` | Create or remove typed edges. |
| `keel_activity` | Events since a timestamp or ULID cursor. |
| `keel_projects` | List and resolve projects — the disambiguation surface. |

### 6.3 `keel_context` — the most important tool

```
keel_context(project?: string, depth?: 'brief'|'standard'|'full', since?: timestamp)
```

Returns, budgeted to roughly 3–4k tokens at `standard`:

```
project        name, slug, status, one-line description
active         current milestone(s), target date, % tasks done
attention      open P0/P1 tasks, blocked tasks, overdue milestones
recent         last N events, summarised
decisions      last 5 accepted decisions, title + one-line
questions      all open questions and unmitigated risks   ← never truncated
specs          current specs: title, kind, status, last revised
terms          glossary for this project                  ← never truncated
environments   what is live, at what version
next           suggested next actions derived from state
meta           echoed session_id, budget_exceeded flag, truncation report
```

Questions and terms are the declared-unbounded fields from §6.1. They are never truncated because they are the two things whose absence causes an agent to *actively do the wrong thing* — re-litigating a settled question, or using the wrong word for a domain concept. A truncated task list makes an agent less informed; a truncated glossary makes it confidently wrong. Everything else degrades gracefully, and the response reports what it dropped.

If questions and terms alone would exceed the budget, the digest returns them in full and sets `budget_exceeded: true` rather than trimming — a signal that the project's open-question register needs pruning, which is real information.

With no `project`, returns a cross-project roll-up: one line per project plus anything at risk.

### 6.4 Project creation and disambiguation

Per PRD REQ-8, `keel_create(type: 'project')` is permitted, but safety lives in the **skill**, not the API:

1. Before creating, the agent must call `keel_projects(query: …)`, which does fuzzy matching on name, slug, aliases, and repo URL.
2. If any candidate scores above threshold, the tool response includes `requires_confirmation: true` with the candidates, and the skill instructs the agent to ask the human.
3. New projects are created with `status: 'active'` and an event whose `meta` records `{"confirmed_by": "human"}`.
4. The UI surfaces projects created in the last 7 days with fewer than 3 artifacts as "possibly accidental."

Belt and braces, because this is the failure mode that quietly ruins the aggregate view.

### 6.5 Session identity under a stateless transport

This needs saying explicitly because the two halves of the design pull against each other: MCP 2026-07-28 is deliberately **stateless** — no sessions, no handshakes, every request self-describing — while `session_id` is the provenance unit that G3 and REQ-2 rest on. There is no protocol-level session to borrow.

Resolution: **`session_id` is a domain concept, supplied by the caller, and the daemon never invents one.**

- Every write tool accepts an optional `session_id` and `surface`.
- The **skill** is responsible for generating a stable identifier once per conversation and passing it on every call. A ULID minted at first use, held in the conversation, is sufficient — it needs to be stable and unique, not meaningful.
- If no `session_id` arrives, the daemon records `NULL` and falls back for `actor`: the authenticated client identity where auth exists (Phase 5), otherwise the transport the request arrived on — MCP defaults to `claude`, the local REST API to `human`. Crude, but it degrades provenance to "some Claude session" rather than failing the write. Losing attribution is bad; refusing the write is worse.
- `keel_context` returns the caller's `session_id` back if one was supplied, so a long conversation can self-check that it's still threading correctly.
- The desktop app and CLI use fixed sentinels (`ui`, `cli`).

The consequence to accept: attribution is *cooperative*, not enforced. An agent that doesn't pass a session ID produces weaker provenance, and no protocol mechanism prevents that. This is the correct trade for a stateless transport, but it makes the skill (Phase 2) load-bearing for a v1 must-have — which is another reason Phase 2 isn't optional.

---

## 7. Concurrency and the write path

### 7.1 Single writer

The daemon holds the only read-write DuckDB handle. Everything else goes through the daemon's API.

> **Corrected 2026-08-10.** This sentence used to offer a second option — "connects read-only or goes through the daemon's API" — and the read-only half does not exist. DuckDB refuses a read-only connection while any process holds the write lock, so no second process can read the store while the daemon runs, which is always. Found by implementing `open_read_only` and watching it fail with the same conflicting-lock error a writer gets. The API is the only path; generation moved inside the daemon and the CLI became a client (DECISIONS B-21, QUESTIONS TQ-15). The remaining read commands that still open the store directly are tracked as KEEL-57.

Inside the daemon, DuckDB permits multiple writer *threads* via MVCC plus optimistic concurrency — appends never conflict, and different tables or row ranges proceed independently. Only same-row simultaneous edits conflict, which surfaces as a transaction error and is retried.

**On Quack:** per DuckDB's own concurrency documentation, the Quack remote protocol turns DuckDB into a client-server system to allow multi-process writes; it is beta as of 1.5.2 with maturity anticipated around v2.0 in autumn 2026. (DuckLake with Postgres coordination is the production-ready alternative today, and is overkill here.) Adopt Quack when it helps — specifically to let the Tauri UI and CLI attach read-write directly instead of proxying. But keep the single write *path* regardless, because a write is never just a write:

```
validate → resolve links → generate embedding → write entity
        → append revision → append event → regenerate mirror → notify SSE
```

Six of those seven steps have nothing to do with DuckDB's locking. Quack removes a constraint; it doesn't remove the reason for the design.

### 7.2 Idempotency

Every create accepts `idempotency_key`. The daemon derives a default from `hash(project_id, type, normalised_title)` when one isn't supplied. Repeat calls return the existing entity with `created: false` rather than erroring — a retrying agent gets a sane result instead of a duplicate or a failure it has to reason about.

### 7.3 Optimistic concurrency

```
keel_update(id, version: 7, changes: {...})
  → 409 with { latest_version: 9, current_state: {...}, events_since: [...] }
    // `latest_version`, not `current_version` — the latter names the audit-block
    // concurrency counter and would collide with `current_doc_version`.
```

Returning the events since the agent's read is deliberate: it can usually resolve the conflict itself rather than clobbering or giving up.

---

## 8. Git mirror

One-directional export. **The mirror is never a source of truth.** Reconciliation between two authorities is the failure this whole design exists to avoid, and every "just sync it back" instinct leads there.

Note the precise formulation, because §8.1 below bends it: the mirror is never *read as truth*. It can be read as *evidence that an edit was attempted*.

```
<repo>/.keel/
├── README.md              "generated — do not edit"
├── specs/<slug>.md
├── decisions/<slug>.md
├── questions.md           open questions and risks, one file
├── glossary.md
└── manifest.json          path → [{entity_id, doc_id, version, hash}] → many-to-one
```

`questions.md` and `glossary.md` aggregate many rows into one file, so the manifest is keyed by path with a list of contributors, not `doc_id → path`. The per-row `mirror_path` column on `questions` and `terms` therefore points at a shared file — it answers "where does this appear," not "which file is this."

Each file gets a header:

```markdown
<!-- keel:generated spec spc_01H8ABC v7 2026-08-09T14:22:01Z
     source of truth is Keel — edits here are not saved -->
```

Regenerated on every relevant write, debounced ~2s. Committed or gitignored per PRD Q-3 — recommendation: **commit it**, because it doubles as a legible offline backup and puts specs into repo grep and agent context for free.

### 8.1 There is no path back — and that is the whole design

**Superseded 2026-08-10.** This section used to specify a Claude Code
`PostToolUse` hook that watched for edits to generated files, read the edited
file, wrote its contents back as a new revision, and then regenerated the file
from the database. It argued at length that this was safe because it was
event-triggered rather than reconciliation-triggered, and it promised that "the
database wins unconditionally afterwards, so if the write was rejected the edit
is discarded and the file reverts."

**None of that was ever true, because the hook never ran.** It called
`keel mirror`, a command that had been renamed to `keel generate` underneath it,
and swallowed the failure with `|| true`. It read `KEEL_SESSION_ID`, which
nothing anywhere sets. On the machine this project is developed on it was not
even installed — it was configured only in `plugin/hooks/hooks.json`, which
applies when Keel is loaded as a plugin, and it never was. So every edit it
claimed to capture was lost in silence, and the guarantee written here and in
the plugin README was a guarantee about nothing.

The hook was **deleted rather than repaired**. A mechanism that quietly does not
work is worse than no mechanism, because it is relied upon: the one-directional
rule was being softened here to make room for an exception that did not exist.

What replaces it does less and says more. `scripts/pre-commit` runs
`keel generate --check` when a commit carries a generated file, and refuses the
commit if the file differs from what Keel would produce. It does not write to
the store, does not rewrite your files, and does not try to guess what you
meant — it tells you the edit will be reverted and where to make it instead.
It also distinguishes "the check could not run" from "the files are wrong",
because reporting one as the other is how a green check comes to mean nothing.

For a deliberate migration there is `keel import <file>`, which is a person
running a command, not a background mechanism.

**The rule is now absolute: nothing reads a generated file back into the store
on its own.** D-3 no longer has an exception, and the "one permitted read" that
this section used to define does not exist.

One consequence worth stating plainly, since it is now the failure mode rather
than a footnote: an edit to a generated file outside a commit is simply gone.
`product/STATUS.md` is the sharpest case — it is rendered from task rows and has
no stored body at all, so there is nothing an edit there could even become a
revision *of*.

---

## 9. GitHub integration

A GitHub App with webhooks, not polling.

- `Closes KEEL-<id>` or `keel:tsk_01H8…` in a PR body creates a `resolves` link.
- On merge: per PRD Q-1, **propose** rather than auto-close. The daemon sets `status: review` and writes an event; the next `keel_context` surfaces "3 tasks look done, confirm?" A merged PR is not always a finished task, and silent auto-close erodes trust in the status field faster than anything else.
- `push` events attach commits to the linked task's timeline.
- `deployment_status` updates the matching `environments` row.

Webhook receiver is `keel-github`, a separate binary that calls the daemon's API — so it can be deployed publicly while the daemon stays local, over a tunnel.

---

## 10. Desktop app

**Tauri v2.** React + TypeScript + Tailwind 4 + shadcn/ui in the webview. The daemon runs as a Tauri sidecar; the UI talks to it over local HTTP + SSE — identical to how it would talk to a remote daemon, so the web build is the same bundle with a different base URL.

Screens:

1. **Home** — all projects, health at a glance, what shipped this week, what's at risk.
2. **Project dashboard** — active milestone, task counts by status, open questions, recent activity, live environments.
3. **Roadmap** — timeline of milestones and releases across one or all projects.
4. **Board** — tasks by status, filterable, keyboard-driven.
5. **Documents** — reader with version history, side-by-side diff between revisions, and the link graph for the current doc.
6. **Search** — hybrid, cross-project, faceted by type.
7. **Feedback** — inbox view, triaged/untriaged, semantic clustering.
8. **Design** — proposed vs approved vs built, side by side.
9. **Activity** — the event feed, filterable by actor. "What did Claude do today."

Read and search first, per the PRD. Writing is possible but never the fast path — the fast path is talking to Claude.

**Mobile (Phase 5).** Tauri v2 targets iOS/Android from the same codebase. Do **not** embed DuckDB and Lance in the mobile binary — both are large native dependencies and Lance especially will bloat it. Mobile is a thin client against a remote daemon.

---

## 11. Deployment, backup, security

**Local (v1).** Everything under `~/.keel/`:

```
~/.keel/
├── keel.duckdb
├── lance/{documents,blobs}.lance
├── config.toml
├── models/            local embedding model
└── backups/
```

**Backup.** `~/.keel` is itself a git repo. `keel-cli backup` runs DuckDB `EXPORT DATABASE` to Parquet, **dumps the Lance datasets to Parquet as well** (embeddings included), snapshots the Lance datasets in place, commits, and optionally pushes to a private remote. Nightly, plus before every migration. The Parquet exports are the point: they mean a restore never depends on a specific DuckDB *or* Lance version being readable.

Recovery tiers, stated precisely because the third is easy to overclaim:

1. **Restore from `~/.keel` git history.** Full fidelity — everything, including revision history.
2. **Rebuild from the Parquet export.** `EXPORT DATABASE` covers DuckDB; the documents dataset needs its own Lance→Parquet dump, which `keel-cli backup` must perform explicitly. Do not skip this — a Lance *snapshot* is not an escape hatch from Lance, and without the Parquet dump, tier 2 depends on the Lance format staying readable. Build it in Phase 0.
3. **Last resort: the committed `.keel/` mirrors in each repo.** These contain *only* current specs, decisions, open questions and the glossary — as readable markdown. **Tier 3 does not recover tasks, feedback, metrics, observations, design artifacts, environments, artifacts, links, or the event log**, and it recovers only current revisions, not history. It is a legibility guarantee, not a backup. Tier 3 also depends on PRD Q-3 resolving in favour of committing the mirror; if that flips, this tier disappears entirely.

Tiers 1 and 2 are the actual backup story. Tier 3 exists so that a catastrophe leaves you with readable prose rather than nothing.

**Remote (Phase 5).** Single container, daemon + web bundle, persistent volume. Auth: single bearer token to start. If it ever leaves your machine, move to OAuth 2.1 with **Client ID Metadata Documents** for client identification — MCP 2026-07-28 deprecates Dynamic Client Registration in favour of CIMD, but note these are client *identification* mechanisms; the authorization flow itself is standard OAuth on top.

**Security posture v1.** Daemon binds `127.0.0.1` only. No auth locally. Tunnel (Cloudflare/Tailscale) for the GitHub webhook, token-gated. Do not expose the daemon publicly before Phase 5.

---

## 12. Build phases with exit criteria

| Phase | Scope | Exit criteria |
|---|---|---|
| **0 — Spine** | `keel-core` (schema, ULIDs, events, migrations, revisions, links, embeddings, hybrid search) plus the minimum of `keel-cli` needed for backup and `fsck`. Dependency verification (TQ-7) happens here, first. | All 13 entity types round-trip; event log correct; 200-entity fixture loads; graph-direction tests pass in **both** directions for every relation in §3.3; backup round-trips (back up → wipe → restore → diff clean), including the Lance→Parquet dump |
| **1 — Daemon** | axum, 9 MCP tools, `keel_context`, concurrency safety, wiring the Phase 0 search into the tool surface | A live Claude session completes PRD UC-1 → UC-4; two concurrent sessions writing produce zero duplicates and zero lost updates |
| **2 — Plugin** | Skill, session-ID threading (§6.5), project-confirmation, mirror hooks, install script | Across 10 unprompted sessions, Claude writes to Keel in ≥9, threads `session_id` on every write, and creates 0 duplicate projects |
| **3 — Desktop** | Tauri shell, sidecar, screens 1–6 **and 9 (Activity)** — REQ-10 lists the activity feed as v1 | Sunday-review use case (UC-6) completes in under 30s |
| **4 — Integrations** | GitHub App, design artifacts, metrics, screens 7–8 | PR merge proposes closure; design proposed-vs-built renders |
| **5 — Remote** | Deployable daemon, auth, mobile client | Project status readable from phone |

**Phase 2 is the real test.** If Keel isn't useful after Phase 2 with no UI at all, the premise is wrong and the UI won't rescue it. Build 0–2 before writing a line of the desktop app.

---

## 13. Decisions embedded in this spec

| # | Decision | Rationale |
|---|---|---|
| D-1 | DuckDB + Lance, no SQLite | Native Rust crates; Lance extension unifies the SQL surface; write volume is trivially low |
| D-2 | Single unified `documents` dataset | One hybrid search across all prose; one versioning code path; new types cost nothing |
| D-2b | Revisions in user columns, not Lance dataset versions | Domain revisions must survive compaction and re-embedding; dataset versions serve snapshot/restore instead |
| D-3 | Database canonical, git mirror generated | Split-brain is worse than the loss of PR review, which doesn't apply to a solo user |
| D-4 | Recursive CTEs, not DuckPGQ or FalkorDB | DuckPGQ can't run on 1.5.x alongside Lance; FalkorDB is a separate server for a tiny graph |
| D-5 | Daemon owns the single write path | Six of seven write steps are unrelated to locking; Quack changes convenience, not architecture |
| D-6 | Rust + Tauri | Storage engines are Rust-native; one codebase reaches desktop and mobile |
| D-7 | Local embeddings via fastembed | Preserves the offline goal; model version stored so upgrading is a background migration |
| D-8 | Propose task closure, don't auto-close | A merged PR isn't always done; silent wrong status destroys trust in the field |
| D-9 | Soft delete only, links included | Agents make mistakes; hard deletes make them permanent |
| D-10 | `session_id` is caller-supplied, never daemon-invented | Stateless transport has no session to borrow; cooperative attribution beats refused writes |
| D-11 | `blocks`/`depends_on` normalised to one stored direction | Storing both is the easiest way to make graph traversals silently wrong |

---

## 14. Open technical questions

- **TQ-1** — Do requirement anchors (`REQ-4`) get parsed out of markdown by convention, or explicitly declared in frontmatter? Parsing is friendlier to agents; declaration is more stable across revisions.
- **TQ-2** — Should `keel_context` be cached and invalidated by event, or computed per call? Start with per-call; measure. *(Event-log retention is PRD Q-5 and lives only there.)*
- **TQ-3** — Re-embedding strategy when the model changes: background full pass, or lazy on access?
- **TQ-4** — Does `v_entities` (the unified vertex view from §4) get built now for the DuckPGQ contingency and general convenience, or deferred? Cheap now, annoying to retrofit.
- **TQ-5** — Does the mirror include tasks, or only prose? Leaning prose-only — tasks churn too much and would make repo diffs noisy. Note this constrains the dogfooding plan: if the mirror is prose-only, `product/STATUS.md` after the Phase 1 switch is produced by a dedicated `keel-cli render-status` command, not by the §8 mirror.
- **TQ-6** — How does a design artifact's image get *into* Keel from a Claude session? Cowork can send files; Claude Code can read them; Claude chat is harder.
- **TQ-7** — Several claims in this document rest on fast-moving sources and were written from documentation rather than running code. Re-verify **before building the storage layer** — this is Phase 0 task P0-2, not a Phase 1 activity, because the highest-stakes items can invalidate §2–§5. Scope: the Lance DuckDB extension's availability and syntax, the current DuckDB version, Quack's status, the current MCP spec version and its transport and header names, the DCR→CIMD deprecation, and `fastembed-rs`. Escalate to KB rather than working around anything that invalidates the storage design.

*Note on IDs: TQ-2 and TQ-3 were renumbered between drafts when event-log retention moved to PRD Q-5. From here on, retired TQ numbers are not reused.*

---

**Sources for the technical claims in this document:**
[DuckDB Lance extension](https://duckdb.org/docs/lts/core_extensions/lance) ·
[Test-driving Lance in DuckDB](https://duckdb.org/2026/05/21/test-driving-lance) ·
[DuckDB concurrency](https://duckdb.org/docs/current/connect/concurrency) ·
[DuckPGQ community extension](https://duckdb.org/community_extensions/extensions/duckpgq) ·
[DuckDB graph queries guide](https://duckdb.org/docs/current/guides/sql_features/graph_queries) ·
[FalkorDB Rust client](https://github.com/falkordb/falkordb-rs) ·
[FalkorDB next-gen Rust engine](https://github.com/FalkorDB/falkordb-rs-next-gen) ·
[MCP 2026-07-28 specification](https://blog.modelcontextprotocol.io/posts/2026-07-28/) ·
[Tauri 2](https://v2.tauri.app/)
