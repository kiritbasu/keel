<!-- specline:generated spec spc_01KZKMPVNTZAZHC9HY1TSNZNGM
     Specline is the source of truth for this file. Edit it there — in the app, or by asking Claude — and regenerate.
     An edit made here is overwritten on the next `specline generate`. -->

# Keel — Technical Specification

> **Status:** Draft v1
> **Companion to:** PRD.md
> **Date:** 2026-08-09
> **Storage sections revised:** 2026-08-12, after Phase 9 replaced DuckDB and Lance with one SQLite file. Where a claim changed, the old one is marked rather than deleted — see §13 D-1, which is the same treatment.

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
                                  │  the single write path
                  ┌───────────────▼──────────────────────┐
                  │  keel-core                           │
                  │  domain · validation · provenance    │
                  │  embeddings · git mirror (gix)       │
                  └───────────────┬──────────────────────┘
                                  │
                  ┌───────────────▼──────────────────────┐
                  │   SQLite   ~/.keel/keel.sqlite       │
                  │   entities · links · events · notes  │
                  │   documents (+ embeddings) · blobs   │
                  │   fts_entities (FTS5) · v_entities   │
                  └──────────────────────────────────────┘
                        one file · one connection · WAL
```

A join from a task to the spec revision that motivated it, ranked by vector distance, is one statement against one database. There is no attach step and no boundary at which the two halves of a write can half-land.

> **Replaced in Phase 9, 2026-08-12.** This diagram had two boxes: a DuckDB box holding entities, links, events and metrics, and a Lance box holding documents, blobs and embeddings, with the Lance datasets `ATTACH`ed into DuckDB as table namespaces so that one SQL statement could reach across both. That worked, and the sentence under the diagram used to say so. What it cost is recorded in §13 D-1 and measured in `PHASE-9.md`: a cold release build in tens of minutes, a keyword index rebuilt in full on every write because that engine's full-text index did not track inserts, and a backup in two formats that could be taken moments apart and be internally inconsistent with nothing able to detect it.

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

- **The storage layer needs nothing installed and nothing running.** `rusqlite` compiles the bundled SQLite amalgamation as part of the workspace build and links it statically, so there is no sidecar database process, no ORM impedance, and no cross-language marshalling on the hot path. This was the strongest argument for Rust here and it survives the change of engine. Two honest caveats: SQLite is C rather than Rust, and the embedding path in §5 pulls in ONNX Runtime through `ort`, so the stack is not FFI-free. Neither costs a process or a deployment step.
- **The single write path is now a rule rather than a constraint.** DuckDB refused a second connection outright, so the design and the engine happened to agree. SQLite in WAL mode will let a second process open this file and write to it, and nothing will stop it. The rule stands anyway, for the reason in §7 — six of the seven steps in a Keel write have nothing to do with locking. Whether to enforce it in code is TQ-36, open.
- **The Tauri app can't serve Claude chat.** Building daemon-first prevents logic getting trapped in the desktop app.
- **`gix` (gitoxide)** for git operations — pure Rust, no libgit2 linkage.

---

## 2. Storage split

| Lives in | What | Why |
|---|---|---|
| **Entity tables** — `projects`, `tasks`, … plus `links`, `events`, `notes` | Entity headers, edges, the event log, commentary, metrics, environments | Mutable rows, frequent status flips, relational joins |
| **`documents`** | Every prose body, every revision, each with its embedding | Append-only, needs keyword and vector search over the same text |
| **`blobs`** | Images, screenshots, attachments | Large, read lazily, written in the same transaction as the row they belong to |

**Rule:** anything whose access pattern is *update in place* is a row you update; anything whose pattern is *append a new version* is a row you insert. That rule is unchanged. What changed is that it is no longer a choice of engine — it is a choice of table, and both sides of it can be written in one transaction.

> **Superseded in Phase 9.** This section used to argue the split as a property of the two engines, and the argument was careful: that Lance *can* mutate through deletion vectors so the claim was never "Lance can't", but that frequent small status flips on a columnar layout are pessimal for reads, that the documents corpus is append-only by nature so nothing was given up, and that Lance was the only store carrying the vector index and the multimodal blobs, so putting the searchable text beside them avoided a sync. The middle clause is still true and is why `documents` is still append-only. The other two described a boundary that no longer exists. The sync they were meant to avoid was between two engines that could each accept a write the other never saw, which is why `fsck` had to audit the pairing across it — that was the only way to find it broken.

Revision semantics are implemented in **user columns**: a revision is a row in `documents` carrying a `version`, not a storage-level snapshot. D-2b said that about Lance dataset versions; the rule outlived the reason for it, and now keeps document revisions independent of `VACUUM INTO` snapshots (§11) and of any vector index that might later be added (§5).

### 2.1 The unified documents table

This is the decision in the spec with the widest reach. Rather than a table per prose-bearing entity, there is **one** `documents` table that every entity's body writes into:

```sql
CREATE TABLE documents (
  doc_id            TEXT PRIMARY KEY,     -- ULID
  entity_type       TEXT NOT NULL,        -- 'spec'|'decision'|'feedback'|'design'|'question'
  entity_id         TEXT NOT NULL,        -- → the header row in one of the entity tables
  project_id        TEXT,                 -- denormalised for filtering
  version           INTEGER NOT NULL,
  parent_version    INTEGER,
  title             TEXT NOT NULL,
  body              TEXT NOT NULL,        -- markdown
  body_hash         TEXT NOT NULL,        -- content-addressed; identical revisions dedupe
  media_ref         TEXT,                 -- → blobs.blob_id
  status            TEXT NOT NULL,        -- 'draft'|'current'|'superseded'|'archived'
  author            TEXT NOT NULL,        -- 'human'|'claude'
  session_id        TEXT,
  surface           TEXT,                 -- 'chat'|'cowork'|'code'|'ui'|'cli'
  created_at        TEXT NOT NULL,
  embedding         BLOB,                 -- raw little-endian f32, 384 wide
  embedding_model   TEXT,                 -- e.g. 'bge-small-en-v1.5'
  embedding_version INTEGER               -- bump to trigger re-embed
) STRICT;
CREATE UNIQUE INDEX documents_version ON documents(entity_id, version);
CREATE INDEX documents_current ON documents(entity_id, status);
```

`embedding` is a plain blob rather than a column in a vector index, and that is deliberate. `sqlite-vec` is 0.1.9 and its author says to expect breaking changes; keeping the vectors as bytes Keel owns means replacing the vector search is a new query over the same column rather than a re-embedding run over the whole corpus. §5 has the rest of that reasoning.

`entity_id` points at a header row in one of the thirteen entity tables. It is still not a declared foreign key — the reference is polymorphic, so there is no single table to name — but the reason has changed. It used to be unenforceable because the two rows lived in engines that could not see each other; now they live in one file and are written in one transaction, so the pairing cannot half-land in the first place. `keel-core` validates on write and `keel fsck` audits.

Consequences:

- **One hybrid search covers everything.** "What do we know about onboarding?" is a single search that returns spec sections, decisions, customer feedback and design captions ranked together. With a table per type that is a five-way union and a manual re-rank — and, since BM25 scores are only comparable within one index, a re-rank over numbers that cannot honestly be compared (§5).
- **Versioning is uniform.** One code path for revisions, diffing, and provenance regardless of artifact type.
- **Adding a prose-bearing type costs nothing** — no new table, no new index, no new search path.

Entity headers carry `current_doc_version`; the pointer is on the row, the body is in `documents`. Every prose-bearing type — including `questions`, whose body is a document like any other — has that column.

---

## 3. Data model

### 3.1 Conventions

- **IDs:** ULID, prefixed by type — `prj_01H8…`, `tsk_01H8…`, `spc_01H8…`. Sortable by creation, unambiguous in agent output.
- **Timestamps:** UTC, ISO 8601, stored as text. Once the column is text every `ORDER BY created_at` is a string comparison, and UTC ISO 8601 is the rendering that sorts lexicographically in the same order it sorts chronologically. `store/rows.rs` pins the format down to the width of the fraction, which is not cosmetic.
- **Soft delete only.** `archived_at`. Agents make mistakes; hard deletes make them permanent. This applies to links too — `keel_link`'s remove operation sets `archived_at`, it does not `DELETE`.
- **Referential integrity is application-level.** `links` is polymorphic across thirteen tables, and so is `documents.entity_id`, so neither can be expressed as a declared constraint. `keel-core` validates on write; `keel fsck` audits. Archive cascade is explicit: archiving a parent archives its links but never its children, and orphans surface in `fsck`. *(This bullet used to give a second reason — that `documents` lived in Lance where DuckDB could not see it. That reason is gone; the polymorphism is the whole of it now, and the class of orphan that spanned two engines cannot occur.)*
- **Provenance vocabulary.** One concept, two shapes: entity tables record state (`created_by`, `updated_by`), the event log records the act (`actor`). They draw from the same value set — `human | claude | github | system` — and an entity's `updated_by` always equals the `actor` of the event that produced it.
- **Every table carries the audit block below**, written as `<audit>` to avoid repeating it thirteen times. The one deliberate exception is `events` (§3.4), which is append-only and immutable: it has no `updated_at`, no `version`, and no `archived_at`, because none of them can ever change.

```sql
-- <audit> expands to:
  created_at   TEXT    NOT NULL,
  updated_at   TEXT    NOT NULL,
  version      INTEGER NOT NULL DEFAULT 1,   -- optimistic concurrency
  created_by   TEXT    NOT NULL,             -- 'human'|'claude'|'github'|'system'
  updated_by   TEXT    NOT NULL,
  session_id   TEXT,                         -- provenance unit, see §6.5
  surface      TEXT,                         -- 'chat'|'cowork'|'code'|'ui'|'cli'
  archived_at  TEXT
```

`created_by` and `session_id` are not optional garnish — G3 and REQ-2 are the whole provenance guarantee, and they live here.

**Types.** SQLite has five storage classes, so most of the mapping is forced. Three choices are not: list columns are `TEXT` holding JSON, because there is no array type and the conversion is better tested in Rust than in a database; timestamps and dates are `TEXT`, for the reason above; booleans are `INTEGER`, because 0 and 1 is what the engine's own `true` and `false` compile to. Every table is `STRICT` — without it SQLite accepts a string into an integer column and stores it as one, and the read that eventually fails is nowhere near the write that caused it.

### 3.2 Schema

Reproduced here at spec fidelity. `crates/keel-core/src/store/schema.rs` is the authority for the exact text, including the expansion of `<audit>` and the indexes not shown.

```sql
CREATE TABLE projects (
  id              TEXT PRIMARY KEY,
  slug            TEXT UNIQUE NOT NULL,
  key             TEXT,                                 -- 'KEEL', for task references
  name            TEXT NOT NULL,
  description     TEXT,
  status          TEXT NOT NULL DEFAULT 'active',       -- active|paused|shipped|abandoned
  repo_urls       TEXT,                                 -- JSON array
  root_path       TEXT,                                 -- local checkout, for the mirror
  status_path     TEXT,                                 -- where the tracker is rendered
  decisions_path  TEXT,
  milestone_noun  TEXT,                                 -- 'Phase', for this project
  aliases         TEXT,                                 -- JSON array; see §6.4
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE milestones (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  kind            TEXT NOT NULL DEFAULT 'milestone',    -- milestone|release
  name            TEXT NOT NULL,
  summary         TEXT,
  status          TEXT NOT NULL DEFAULT 'planned',      -- planned|active|blocked|shipped|cut
  target_date     TEXT,
  shipped_at      TEXT,
  version_string  TEXT,                                 -- releases only
  sort_order      INTEGER,
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE tasks (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  milestone_id    TEXT,
  parent_id       TEXT,
  number          INTEGER,                              -- the stable KEEL-42
  rank            REAL,
  kind            TEXT NOT NULL DEFAULT 'task',         -- task|bug|chore|spike
  title           TEXT NOT NULL,
  summary         TEXT,                                 -- readable cold, six weeks later
  body            TEXT,                                 -- short; long-form goes to a spec
  status          TEXT NOT NULL DEFAULT 'todo',         -- todo|in_progress|review|done|wont_do
  priority        TEXT DEFAULT 'p2',                    -- p0|p1|p2|p3
  labels          TEXT,                                 -- JSON array
  external_refs   TEXT,                                 -- JSON array of PR/issue URLs
  claimed_by      TEXT,                                 -- the session holding it
  claimed_at      TEXT,
  close_reason    TEXT,                                 -- done|wont_do|duplicate|superseded|no_change
  close_message   TEXT,
  evidence        TEXT,                                 -- JSON array; required for 'done'
  closed_at       TEXT,
  idempotency_key TEXT NOT NULL,                        -- always derived if not supplied, §7.2
  <audit>
) STRICT;
CREATE UNIQUE INDEX tasks_idem ON tasks(project_id, idempotency_key);
CREATE UNIQUE INDEX tasks_number ON tasks(project_id, number);

CREATE TABLE specs (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'spec',     -- prd|spec|rfc|design-doc|note
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'draft',    -- draft|review|approved|superseded
  current_doc_version INTEGER NOT NULL DEFAULT 0,       -- → documents.version
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE decisions (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  number              INTEGER,                          -- the stable B-11
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'proposed', -- proposed|accepted|superseded|rejected
  decided_at          TEXT,
  current_doc_version INTEGER NOT NULL DEFAULT 0,
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  <audit>
) STRICT;
-- Immutability of accepted decisions is enforced in keel-core, not by the schema:
-- keel_update rejects content changes where status='accepted'. Supersede instead.

CREATE TABLE questions (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'question', -- question|risk|assumption
  title               TEXT NOT NULL,
  status              TEXT NOT NULL DEFAULT 'open',     -- open|answered|accepted|mitigated|moot
  severity            TEXT,                             -- risks: low|medium|high
  resolved_at         TEXT,
  current_doc_version INTEGER NOT NULL DEFAULT 0,       -- body lives in documents
  mirror_path         TEXT,
  idempotency_key     TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE terms (
  id              TEXT PRIMARY KEY,
  project_id      TEXT,                                 -- NULL = global
  term            TEXT NOT NULL,
  definition      TEXT NOT NULL,
  means           TEXT,
  aliases         TEXT,                                 -- JSON array
  mirror_path     TEXT,
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;
CREATE UNIQUE INDEX terms_uniq ON terms(COALESCE(project_id, ''), term);
-- project_id NULL = global term. Per-project rows override a global of the same
-- name; resolution is project-first (PRD Q-4, provisionally resolved this way).
-- COALESCE in the index because SQL says NULL is distinct from NULL, so a bare
-- nullable column would let duplicate globals through and make "the override"
-- ambiguous.

CREATE TABLE feedback (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  kind                TEXT NOT NULL DEFAULT 'observation',
                      -- interview|support|sales|idea|competitor|observation
  summary             TEXT NOT NULL,                    -- not `title`: a piece of
                                                        -- feedback is what someone
                                                        -- said, and titling it would
                                                        -- mean inventing one
  source              TEXT,                             -- who/where
  contact             TEXT,
  sentiment           TEXT,                             -- positive|neutral|negative|mixed
  occurred_at         TEXT,
  triaged             INTEGER DEFAULT 0,
  current_doc_version INTEGER NOT NULL DEFAULT 0,       -- verbatim body → documents
  idempotency_key     TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE design_artifacts (
  id                  TEXT PRIMARY KEY,
  project_id          TEXT NOT NULL,
  name                TEXT NOT NULL,
  state               TEXT NOT NULL DEFAULT 'concept',  -- concept|proposed|approved|built
  figma_ref           TEXT,
  blob_id             TEXT,                             -- → blobs.blob_id
  current_doc_version INTEGER NOT NULL DEFAULT 0,       -- caption/rationale → documents
  idempotency_key     TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE environments (
  id               TEXT PRIMARY KEY,
  project_id       TEXT NOT NULL,
  name             TEXT NOT NULL,                       -- production|staging|preview
  url              TEXT,
  deployed_version TEXT,                                -- NOT current_doc_version — the
  deployed_commit  TEXT,                                -- shipped app version. Named
                                                        -- distinctly on purpose.
  status           TEXT NOT NULL DEFAULT 'unknown',     -- healthy|degraded|down|unknown
  last_deployed_at TEXT,
  idempotency_key  TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE metrics (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  name            TEXT NOT NULL,
  unit            TEXT,
  direction       TEXT,                                 -- up|down
  target_value    REAL,
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE metric_observations (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,                        -- denormalised for filtering
  metric_id       TEXT NOT NULL,
  value           REAL NOT NULL,
  observed_at     TEXT NOT NULL,
  note            TEXT,
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;

CREATE TABLE artifacts (
  id              TEXT PRIMARY KEY,
  project_id      TEXT NOT NULL,
  kind            TEXT NOT NULL DEFAULT 'link',         -- link|file|image|other
  name            TEXT NOT NULL,
  url             TEXT,
  blob_id         TEXT,
  idempotency_key TEXT NOT NULL,
  <audit>
) STRICT;
```

Two tables sit beside the thirteen. `notes` is the running commentary on any row — append-only, attributed to the session that wrote it, and where findings live now that the tracker is rows rather than a markdown table. `blobs` holds image bytes with their media type, length and SHA-256, keyed by `blob_id` and written in the same transaction as the row that refers to them.

**`v_entities`** is a view: `UNION ALL` over every entity table, projecting `id`, `entity_type`, `project_id`, `label` and `archived_at`. It is what lets a query resolve an id without knowing its type — `label` unifies the four different column names (`name`, `title`, `term`, `summary`) that all mean "what this is called". It was proposed in §4 as groundwork for a graph extension that never arrived; it exists anyway, because resolving an id to a label is a thing every surface needs. That settles TQ-4.

### 3.3 Links — the graph

```sql
CREATE TABLE links (
  id          TEXT PRIMARY KEY,
  project_id  TEXT,
  from_type   TEXT NOT NULL,
  from_id     TEXT NOT NULL,
  rel         TEXT NOT NULL,
  to_type     TEXT NOT NULL,
  to_id       TEXT NOT NULL,
  anchor      TEXT NOT NULL DEFAULT '',  -- e.g. 'REQ-4'; '' means whole-entity.
                                         -- NOT NULL so the unique index actually
                                         -- fires — a nullable column would make
                                         -- every ordinary edge distinct.
  note        TEXT,
  <audit>
) STRICT;
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

`blocks` and `depends_on` are inverses. `keel-core` normalises on write: everything is stored as `blocks`, and a `depends_on` request is written with the endpoints swapped. Storing both directions is the single easiest way to make the graph queries silently wrong. Nothing in the schema enforces this and nothing can — both are legal values — so it is enforced on write and audited by `fsck`.

### 3.4 Events

```sql
CREATE TABLE events (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,  -- the cursor keel_activity pages from
  id          TEXT NOT NULL UNIQUE,               -- ULID
  project_id  TEXT,
  entity_type TEXT NOT NULL,
  entity_id   TEXT NOT NULL,
  field       TEXT,
  op          TEXT NOT NULL,     -- created|updated|status_changed|linked|revised|archived
  before      TEXT,              -- JSON
  after       TEXT,              -- JSON
  summary     TEXT NOT NULL DEFAULT '',  -- the sentence a person reads
  meta        TEXT,              -- JSON, e.g. {"confirmed_by":"human"} for §6.4
  actor       TEXT NOT NULL,     -- human|claude|github|system
  session_id  TEXT,
  surface     TEXT,
  at          TEXT NOT NULL
) STRICT;
CREATE INDEX events_project ON events(project_id, seq);
```

Append-only, never updated. "What changed since T" is a range scan on `seq`.

> **Changed in Phase 9.** This used to read "because ULIDs sort chronologically, 'what changed since T' is a range scan", followed by a paragraph on why that only holds if the ULIDs are minted monotonically — a plain ULID re-randomises its low 80 bits on every call, so two ids minted inside the same millisecond sort arbitrarily, and a burst of writes inside one millisecond is what an agent doing normal work looks like. That paragraph is still true of ids in general and `keel-core` still mints every id from one process-wide monotonic generator (D-B-9). The cursor no longer rests on it: `seq` is an `AUTOINCREMENT` integer, assigned by the engine in commit order, so "catch me up" cannot skip or repeat a row even if two ids sort against each other unexpectedly.

`AUTOINCREMENT` rather than a bare `INTEGER PRIMARY KEY`, and the difference matters here. A bare rowid reuses the highest deleted value; `AUTOINCREMENT` never reuses one. Nothing is ever deleted from this table, so in principle they behave identically — but "in principle nothing deletes from here" is exactly the kind of assumption a cursor should not rest on.

`summary` is written at the point the write happens and is not derivable from the columns beside it. The changelog and the activity feed both render it, so an events table without it makes both go blank — plausibly, and only for history written after the omission.

---

## 4. Graph layer

**Decision: recursive CTEs. Not a graph extension, not a graph server.**

FalkorDB is the wrong shape. There *is* a Rust rewrite of the engine underway (`falkordb-rs-next-gen`), but the shipping product is a server with Redis-module lineage, and `falkordb-rs` is a *client*, not an embeddable library. That is a second datastore and a second process to run, back up, and keep consistent, for a graph that will have a few thousand edges. GraphBLAS sparse-matrix algebra is built for problems several orders of magnitude larger than this.

Recursive CTEs over `links` handle every query the PRD asks for, in microseconds at this scale.

> **Changed in Phase 9.** The blocking fact this section opened with was that **DuckPGQ was not available for DuckDB 1.5.x** — it required pinning to 1.4.4, while the Lance extension lived in 1.5.x, so you could not have both and Lance was load-bearing. That comparison is moot: neither engine is in the tree. What replaces it is not a constraint but an absence — SQLite has no property-graph extension to want, and recursive CTEs are the whole story rather than the interim one. The contingency paragraph that used to sit at the end of this section, budgeting the work to adopt SQL/PGQ if it ever shipped, is gone with it. Turso, which would have been the closest SQLite-compatible alternative, was ruled out during the survey for exactly this: `Parse error: Recursive CTEs are not yet supported`. Keel's graph cannot be expressed without them.

**Direction matters more than depth.** `implements` runs *task → spec*, so the traceability query for "what implements this spec" traverses **inbound** edges (`to_id → from_id`), not outbound. Getting this backwards returns an empty set that looks like a legitimate "nothing links here."

```sql
-- UC-7: what implements this spec, and what do those things depend on.
-- Inbound on `implements`/`references`, then outbound from whatever we find.
-- `path` is a delimited string rather than a list, because SQLite has no array
-- type; `instr` over it is the cycle guard.
WITH RECURSIVE trace(id, entity_type, rel, anchor, depth, path) AS (
    SELECT l.from_id, l.from_type, l.rel, l.anchor,
           1,
           '|' || :root || '|' || l.from_id || '|'
    FROM links l
    WHERE l.to_id = :root
      AND l.archived_at IS NULL
      AND l.rel IN ('implements','references','derived_from')
  UNION ALL
    SELECT l.from_id, l.from_type, l.rel, l.anchor,
           t.depth + 1, t.path || l.from_id || '|'
    FROM links l
    JOIN trace t ON l.to_id = t.id
    WHERE l.archived_at IS NULL
      AND t.depth < :max_depth                       -- default 6, hard cap 16
      AND instr(t.path, '|' || l.from_id || '|') = 0 -- cycle guard
)
SELECT t.*, v.label
FROM trace t LEFT JOIN v_entities v ON v.id = t.id
ORDER BY t.depth, t.id;

-- What is transitively blocking this task.
-- `blocks` is stored from=blocker → to=blocked, so blockers are found on to_id.
WITH RECURSIVE blockers(id, depth, path) AS (
    SELECT l.from_id, 1, '|' || :root || '|' || l.from_id || '|'
    FROM links l
    WHERE l.to_id = :root AND l.rel = 'blocks' AND l.archived_at IS NULL
  UNION ALL
    SELECT l.from_id, b.depth + 1, b.path || l.from_id || '|'
    FROM links l
    JOIN blockers b ON l.to_id = b.id
    WHERE l.rel = 'blocks' AND l.archived_at IS NULL
      AND b.depth < :max_depth
      AND instr(b.path, '|' || l.from_id || '|') = 0
)
SELECT t.*, MIN(b.depth) AS depth
FROM tasks t JOIN blockers b ON t.id = b.id
WHERE t.status NOT IN ('done','wont_do') AND t.archived_at IS NULL
GROUP BY t.id      -- not DISTINCT: a task reachable by two paths of different
ORDER BY depth;    -- length would otherwise return twice
```

Note `depends_on` never appears in a traversal — §3.3 normalises it to `blocks` on write, so there is exactly one direction to reason about.

`keel-core` exposes three storage traits — `EntityStore` (entities, links, events), `DocumentStore` (revisions, blobs, embeddings, search) and `GraphStore`, the last with `neighbours(id, direction, rels, depth)` so callers never hand-write traversal direction. Every one of these queries is wrong in a way that returns plausible empty results, which is the worst failure mode available; centralising them means getting it right once. The traits are named for what they hold and never for what holds it, which is why they came through the change of engine unchanged — that is the whole return on having drawn them in Phase 0.

---

## 5. Search

Hybrid: **BM25 over an FTS5 index, vectors through `sqlite-vec`, fused by reciprocal rank in `keel-core`.** Both halves are queries against the same database as the rows.

> **Corrected 2026-08-09 against running code, and kept.** This section originally delegated both halves to Lance's `lance_hybrid_search()`. That function's keyword half did not behave predictably on multi-term queries — `"onboarding metering"` matched a document containing only *metering*, while `"onboarding slow"` matched nothing despite a document containing *onboarding*. The extension documented only single-word examples and no way to build the index that would presumably fix it. The mechanism is gone, but the conclusion it reached is why this section still looks the way it does: retrieval is not built on a function whose semantics cannot be stated. See DECISIONS B-12 and TQ-10.

### 5.1 The keyword half

`fts_entities` is an FTS5 index over a plain `fts_source` table, maintained by triggers.

> **Reversed in Phase 9.** This section used to say: "The DuckDB FTS index is a **snapshot**: it does not track inserts. An entity created after the last build is silently unfindable, which is the same shape of failure as an inverted graph traversal. The index is therefore rebuilt whenever the event log's high-water mark has moved." That was accurate and it was the single worst property of the old store — measured at 217 ms against a 13 ms mean, on the first search after *any* write. The opposite is now true. Triggers keep the index in step with the rows underneath it, so a row written in one transaction is findable in the next call, measured at 68 µs. There is no watermark, no rebuild, and nothing to invalidate.

Three details that bite at runtime rather than at compile time:

- **`fts_source` exists because FTS5 indexes by integer rowid and Keel's ids are ULID text.** Something has to hold the mapping. Making it a real table rather than hiding it inside the index means the index is external-content — it stores no second copy of every body — and "what is in the index" is answerable with an ordinary `SELECT` rather than only through a `MATCH`. It is also where archiving is handled: an archived row leaves `fts_source`, so it leaves the index, and no query has to remember to filter.
- **`MATCH` takes a query language, not a string.** A caller searching for `local-first` gets `no such column: first`, because the hyphen makes FTS5 read `first` as a column filter — an error naming a word from the user's own text and reading like a schema bug. Caller input is turned into quoted terms so that nothing a person types is ever parsed as syntax.
- **`bm25()` returns a negative number where lower is better.** The fusion wants higher-is-better, so the score is negated on the way out. Get the sign wrong and the worst match ranks first, which looks like a plausible ordering and is completely wrong — hence a test asserting that an obviously-best row comes first, rather than one asserting a score.

**One index, not thirteen.** BM25 scores are only comparable within a single index, because the term statistics that produce the score are computed per index. Thirteen indexes would give thirteen incomparable numbers and the fusion would be ranking noise.

### 5.2 The vector half

`sqlite-vec`'s `vec_distance_cosine` over `documents.embedding`, scanned in full and sorted by distance. There is deliberately no `vec0` virtual table:

- A `vec0` table is a second copy of every vector, and something has to keep it in step with `documents`. That something would be another trigger, and until it exists the alternative is repopulating the table at search time — which is the rebuild §5.1 exists to have deleted, wearing a different hat.
- At this scale it buys nothing. A few thousand 384-float vectors is 1–3 ms brute force, measured, against a corpus that is one person's project memory. Scale discipline says do not add the index until a measurement asks for it.

`sqlite-vec` is 0.1.9 and its author says to expect breaking changes. That is survivable precisely because the vectors are ordinary little-endian f32 blobs Keel owns rather than rows inside a proprietary index: replacing this half is a new query over the same column, not a re-embedding run over the whole corpus. If `vec_distance_cosine` disappears, the same loop in Rust over the same bytes is about fifty lines and needs no schema change.

One sharp edge is guarded explicitly: `vec_distance_cosine` raises an error when the two vectors differ in length, and that error fails the *whole* query. A single document embedded by an older model with a different width would take out search for everything. A `length(embedding) = ?` predicate skips those rows instead, so a model change degrades recall rather than breaking search.

### 5.3 Fusion and coverage

Reciprocal-rank fusion in `keel-core`, unchanged across the move — BM25 scores and cosine distances are not on comparable scales, and are not even in comparable units, so fusing on *rank* is the only defensible merge. A hit found independently by both halves is the strongest signal available.

Each half retrieves `inner_limit` rows rather than `limit`, set to four times the caller's limit. Retrieving exactly `k` from the index and *then* filtering by project and date is a classic way to return three results when forty exist.

**Coverage.** Everything that carries text is in one index, reached two ways:

| Fed from | Covers | Fields |
|---|---|---|
| `documents` (current revision) | spec, decision, feedback, design, question | title + body, plus the embedding for the vector half |
| The row itself | task, milestone, term, environment, artifact, project | title/name + short body + definition |

The prose types are indexed from `documents` because that is where their text is; the other six are indexed from their own rows because that is where theirs is. Nothing is indexed twice — `fts_source` is keyed by entity id, so a spec with forty revisions occupies one slot rather than forty, which also stops a heavily-edited document outranking everything by sheer repetition.

Together these satisfy REQ-4's "every artifact type that carries text." `metric` and `metric_observation` are deliberately excluded — they are numeric, and searching them is a filter, not a query.

**Embeddings.** A local model via `fastembed-rs` (`bge-small-en-v1.5`, 384-dim). Two honest caveats against G8's "runs entirely locally": the model is downloaded from the Hub on first run, and it executes through ONNX Runtime (`ort`), a C++ dependency. After first-run setup it is fully offline, ~50ms per embed, and good enough for a corpus this size. `embedding_model` and `embedding_version` are stored on each document so upgrading is a background re-embed of stale rows rather than a rewrite. Resolves PRD Q-7 in favour of local, reversibly.

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

The daemon owns the single write path. Everything else goes through the daemon's API.

> **Corrected 2026-08-10, then changed again in Phase 9.** The 2026-08-10 correction said that a second process could not even *read* the store while the daemon ran: DuckDB refused a read-only connection while any process held the write lock, which was found by implementing `open_read_only` and watching it fail with the same conflicting-lock error a writer gets. That is why generation moved inside the daemon and the CLI became a client (DECISIONS B-21, QUESTIONS TQ-15), and that part of the design stands on its own merits. But the constraint behind it is gone. SQLite in WAL mode lets a second process open this file, read a consistent snapshot while a write is in flight — 12 µs against an open ten-thousand-row transaction, measured — and write to it as well. **The single write path is now a convention, not something the engine enforces.** Whether to enforce it in code is TQ-36, open.

What the engine does still guarantee is that two writers cannot interleave: SQLite takes one write lock at a time, readers do not block behind it, and a writer that finds the lock held waits up to `busy_timeout` — five seconds — rather than failing. The daemon should never reach that timeout, because it should be the only writer. If it does, waiting beats failing: the alternative is a tool call that errors for a store that was merely busy.

The reason for the rule is unchanged, and it was never about locking:

```
validate → resolve links → generate embedding → write entity
        → append revision → append event → regenerate mirror → notify SSE
```

Six of those seven steps have nothing to do with the engine's concurrency model. A second process holding the file open can write a row; it cannot write the event, the revision, the embedding or the index entry that make the row mean anything. That is what the rule protects, and it is why removing the constraint did not remove the design.

*(The paragraph that used to sit here on DuckDB's Quack remote protocol — beta at 1.5.2, maturity anticipated around v2.0 — is gone with the engine. Its conclusion was that Quack would remove a constraint without removing the reason for the design. That turned out to be the right shape of argument for the wrong feature: WAL removed the constraint instead.)*

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
running a command, not a background mechanism. One thing to know before running
it: SQLite will let it open the store alongside a running daemon quite happily,
where the old engine would have refused. Stop the daemon first — the reason is
§7.1, not the lock.

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

**Mobile (Phase 5).** Tauri v2 targets iOS/Android from the same codebase. Mobile stays a thin client against a remote daemon — not because the store could not be embedded (SQLite is on every phone already; that argument was about DuckDB and Lance, both large native dependencies), but because the data lives on one machine and a second copy would need sync. §11 lists the three ways that could be answered when it is time to answer it.

---

## 11. Deployment, backup, security

**Local (v1).** Everything under `~/.keel/`:

```
~/.keel/
├── keel.sqlite         rows, documents, blobs, embeddings, the keyword index
├── keel.sqlite-wal     the write-ahead log; checkpointed on shutdown
├── config.toml
├── models/             local embedding model
└── backups/<timestamp>/{keel.sqlite, manifest.json}
```

**Backup.** `~/.keel` is itself a git repo. `keel backup` runs `VACUUM INTO`, which takes a consistent snapshot of the whole database — rows, documents, blobs, vectors and the keyword index — at a single point in time, without stopping the daemon, and writes it as an ordinary SQLite file. Measured at 64 ms for a 6.9 MB database. A manifest with row counts per table is written beside it. Nightly, plus before every migration. Restore is a file copy into an empty directory, and it refuses to write over a store that already exists.

> **Replaced in Phase 9.** This used to describe two dumps: `EXPORT DATABASE` to Parquet for DuckDB, plus an explicit Lance-to-Parquet dump of the documents dataset with the embeddings included, and `restore` had to refuse a backup missing its second half — a backup that covers the rows and skips the documents is not a backup. The check worked. The flaw nobody could design away was the one no check could catch: **a write landing between the two dumps produced a backup that was internally inconsistent and passed everything**, with the rows from one instant and the documents from another. One file taken in one operation is what makes that failure mode stop existing rather than merely become rarer. The restore also stopped converting: the Parquet path had to cast embeddings back to `FLOAT[384]` on the way in, and a restore that converts is a restore that can convert wrongly.

Recovery tiers, stated precisely because the third is easy to overclaim:

1. **Restore from `~/.keel` git history.** Full fidelity — everything, including revision history.
2. **Restore the last snapshot from `backups/`.** A complete, valid SQLite database; putting it where the store lives is the whole operation. This tier used to depend on a separate Parquet export as an escape hatch from the storage format. It no longer does, and the reason is worth being explicit about rather than assuming: the file *is* the portable format. SQLite's on-disk format is documented and its maintainers commit to reading it indefinitely, so "the format stops being readable" is not the risk it was against two engines on their own release cadences.
3. **Last resort: the committed `.keel/` mirrors in each repo.** These contain *only* current specs, decisions, open questions and the glossary — as readable markdown. **Tier 3 does not recover tasks, feedback, metrics, observations, design artifacts, environments, artifacts, links, notes, or the event log**, and it recovers only current revisions, not history. It is a legibility guarantee, not a backup. Tier 3 also depends on PRD Q-3 resolving in favour of committing the mirror; if that flips, this tier disappears entirely.

Tiers 1 and 2 are the actual backup story. Tier 3 exists so that a catastrophe leaves you with readable prose rather than nothing.

**Remote (Phase 5).** Single container, daemon + web bundle, persistent volume. Auth: single bearer token to start. If it ever leaves your machine, move to OAuth 2.1 with **Client ID Metadata Documents** for client identification — MCP 2026-07-28 deprecates Dynamic Client Registration in favour of CIMD, but note these are client *identification* mechanisms; the authorization flow itself is standard OAuth on top.

There is no built-in sync, and Phase 5 wants a phone to read status. Three answers exist and none has to be chosen now: a read-only `VACUUM INTO` snapshot served to the client, Litestream-style replication of the same file, or opening it with libSQL later — a matter of opening it, since it is a standard SQLite file.

**Security posture v1.** Daemon binds `127.0.0.1` only. No auth locally. Tunnel (Cloudflare/Tailscale) for the GitHub webhook, token-gated. Do not expose the daemon publicly before Phase 5.

---

## 12. Build phases with exit criteria

| Phase | Scope | Exit criteria |
|---|---|---|
| **0 — Spine** | `keel-core` (schema, ULIDs, events, migrations, revisions, links, embeddings, hybrid search) plus the minimum of `keel-cli` needed for backup and `fsck`. Dependency verification (TQ-7) happens here, first. | All 13 entity types round-trip; event log correct; 200-entity fixture loads; graph-direction tests pass in **both** directions for every relation in §3.3; backup round-trips (back up → wipe → restore → diff clean) |
| **1 — Daemon** | axum, 9 MCP tools, `keel_context`, concurrency safety, wiring the Phase 0 search into the tool surface | A live Claude session completes PRD UC-1 → UC-4; two concurrent sessions writing produce zero duplicates and zero lost updates |
| **2 — Plugin** | Skill, session-ID threading (§6.5), project-confirmation, mirror hooks, install script | Across 10 unprompted sessions, Claude writes to Keel in ≥9, threads `session_id` on every write, and creates 0 duplicate projects |
| **3 — Desktop** | Tauri shell, sidecar, screens 1–6 **and 9 (Activity)** — REQ-10 lists the activity feed as v1 | Sunday-review use case (UC-6) completes in under 30s |
| **4 — Integrations** | GitHub App, design artifacts, metrics, screens 7–8 | PR merge proposes closure; design proposed-vs-built renders |
| **5 — Remote** | Deployable daemon, auth, mobile client | Project status readable from phone |

*(Phase 0's exit criterion used to end "…including the Lance→Parquet dump". There is one backup format now, so the clause has nothing left to name. Phases 6 onwards are planned in their own documents — `PHASE-8.md`, `PHASE-9.md`, `PHASE-10.md` — and are not folded back into this table.)*

**Phase 2 is the real test.** If Keel isn't useful after Phase 2 with no UI at all, the premise is wrong and the UI won't rescue it. Build 0–2 before writing a line of the desktop app.

---

## 13. Decisions embedded in this spec

| # | Decision | Rationale |
|---|---|---|
| D-1 | ~~DuckDB + Lance, no SQLite~~ — **superseded by D-1a, Phase 9** | Native Rust crates; Lance extension unifies the SQL surface; write volume is trivially low. Every clause of that was true when it was written, and it is kept rather than deleted because what overturned it is only legible beside it. What overturned it was measured, not argued: a cold release build of 22m11s against 23s; a keyword index rebuilt wholesale on every write, because that engine's full-text index does not track inserts; two backup formats that had to be kept in step, with a restore that had to refuse a backup missing half of itself; and a release pipeline that would have had to ship a 40-60 MB library beside every binary. See `PHASE-9.md` for the survey. |
| D-1a | One SQLite file, nothing beside it | Replaces D-1. `rusqlite` with the bundled amalgamation, FTS5 for keyword search, `sqlite-vec` for vectors, blobs and document revisions as ordinary tables. The three storage traits came through the move unchanged, which is what made it affordable; ~~§3 onwards still describes the old shape and is being brought up to date separately~~. **That last clause expired the day after it was written**: KEEL-132 brought §1–§12 and §14 up to date on 2026-08-12, and this table was left alone because §13 is KB's. |
| D-2 | Single unified `documents` ~~dataset~~ **table** | One hybrid search across all prose; one versioning code path; new types cost nothing. **The word was Lance's**; the thing it names is now an ordinary SQLite table, and every clause of the rationale is truer of a table than it was of a dataset. |
| D-2b | Revisions in user columns, ~~not Lance dataset versions~~ | Domain revisions must survive compaction and re-embedding; dataset versions serve snapshot/restore instead. **The alternative it was arguing against no longer exists**, which leaves the conclusion standing without a rival: revisions are rows, and `VACUUM INTO` serves snapshot and restore. The instinct was right twice — B-55's passages are derived and deleted rather than versioned, for the same reason. |
| D-3 | Database canonical, git mirror generated | Split-brain is worse than the loss of PR review, which doesn't apply to a solo user |
| D-4 | Recursive CTEs, not DuckPGQ or FalkorDB | ~~DuckPGQ can't run on 1.5.x alongside Lance~~; FalkorDB is a separate server for a tiny graph. **The whole rationale was replaced and the conclusion got stronger**, which is the most interesting thing in this table: recursive CTEs were chosen to dodge a version conflict between two engines that are both gone, and the Phase 9 survey then ruled out Turso for not supporting them at all. Right answer, retired reason. |
| D-5 | Daemon owns the single write path | Six of seven write steps are unrelated to locking; ~~Quack changes convenience, not architecture~~. **Quack was a DuckDB feature and the clause is moot**, but the first half turned out to be the entire argument — see B-60, which reworded hard constraint 1 around it after SQLite stopped enforcing exclusivity and a second daemon migrated the store under a running one. Exclusivity is now an advisory lock; the write path is why it matters. |
| D-6 | Rust + Tauri | ~~Storage engines are Rust-native~~; one codebase reaches desktop and mobile. **This one is simply no longer true and is worth saying so plainly**: SQLite is a C amalgamation compiled into the binary, and the embedding path reaches ONNX Runtime, which is C++. What survives is the property that was actually wanted — nothing to install, nothing running beside the binary — which §2 now argues directly rather than through the language. |
| D-7 | Local embeddings via fastembed | Preserves the offline goal; model version stored so upgrading is a background migration |
| D-8 | Propose task closure, don't auto-close | A merged PR isn't always done; silent wrong status destroys trust in the field |
| D-9 | Soft delete only, links included | Agents make mistakes; hard deletes make them permanent |
| D-10 | `session_id` is caller-supplied, never daemon-invented | Stateless transport has no session to borrow; cooperative attribution beats refused writes |
| D-11 | `blocks`/`depends_on` normalised to one stored direction | Storing both is the easiest way to make graph traversals silently wrong |

> **Six rows in this table argued from an engine that is gone, and are now annotated rather than rewritten** (TQ-37, 2026-08-13). D-1a, D-2, D-2b, D-4, D-5 and D-6 each keep the rationale they were decided on, with the expired clause struck and what replaced it named beside it. Nothing was quietly reworded to read as though the spec always said SQLite, because every one of those decisions reached a conclusion that survived the change of engine — and in D-4's case a conclusion that got *stronger* while its entire reasoning was replaced. That is the part worth being able to see, and it is only visible if the original argument is still on the page.

---

## 14. Open technical questions

The live register is question rows in Keel, rendered to `.keel/questions.md`. What follows is the original list; two of them were settled by Phase 9 and are marked here so the numbering stays readable.

- **TQ-1** — Do requirement anchors (`REQ-4`) get parsed out of markdown by convention, or explicitly declared in frontmatter? Parsing is friendlier to agents; declaration is more stable across revisions.
- **TQ-2** — Should `keel_context` be cached and invalidated by event, or computed per call? Start with per-call; measure. *(Event-log retention is PRD Q-5 and lives only there.)*
- **TQ-3** — Re-embedding strategy when the model changes: background full pass, or lazy on access?
- **TQ-4** — ~~Does `v_entities` get built now for the DuckPGQ contingency, or deferred?~~ **Settled.** The contingency it was hedging against is gone with DuckDB, but the view was built anyway and is in §3.2: resolving an id to a label without knowing its type is something every surface needs.
- **TQ-5** — Does the mirror include tasks, or only prose? Leaning prose-only — tasks churn too much and would make repo diffs noisy. Note this constrains the dogfooding plan: if the mirror is prose-only, `product/STATUS.md` after the Phase 1 switch is produced by a dedicated `keel-cli render-status` command, not by the §8 mirror.
- **TQ-6** — How does a design artifact's image get *into* Keel from a Claude session? Cowork can send files; Claude Code can read them; Claude chat is harder.
- **TQ-7** — ~~Re-verify the fast-moving claims in this document before building the storage layer: the Lance DuckDB extension's availability and syntax, the current DuckDB version, Quack's status, the MCP spec version and its transport and headers, the DCR→CIMD deprecation, and `fastembed-rs`.~~ **Closed, and it earned its place twice.** The MCP half caught a real one (§6). The storage half was verified as asked and the design was built on it — and then, four phases later, replaced anyway, not because a claim had been wrong but because living with the consequences said more than verifying them could. The habit it encodes is the durable part: check the fast-moving thing against running code, not against its documentation.

*Note on IDs: TQ-2 and TQ-3 were renumbered between drafts when event-log retention moved to PRD Q-5. From here on, retired TQ numbers are not reused.*

---

**Sources for the technical claims in this document:**
[SQLite file format](https://sqlite.org/fileformat2.html) ·
[SQLite FTS5](https://sqlite.org/fts5.html) ·
[SQLite WAL mode](https://sqlite.org/wal.html) ·
[`VACUUM INTO`](https://sqlite.org/lang_vacuum.html#vacuuminto) ·
[sqlite-vec](https://github.com/asg017/sqlite-vec) ·
[rusqlite](https://github.com/rusqlite/rusqlite) ·
[MCP 2026-07-28 specification](https://blog.modelcontextprotocol.io/posts/2026-07-28/) ·
[Tauri 2](https://v2.tauri.app/)

Kept for the decisions they explain rather than for anything current:
[DuckDB Lance extension](https://duckdb.org/docs/lts/core_extensions/lance) ·
[Test-driving Lance in DuckDB](https://duckdb.org/2026/05/21/test-driving-lance) ·
[DuckDB concurrency](https://duckdb.org/docs/current/connect/concurrency) ·
[DuckPGQ community extension](https://duckdb.org/community_extensions/extensions/duckpgq) ·
[DuckDB graph queries guide](https://duckdb.org/docs/current/guides/sql_features/graph_queries) ·
[FalkorDB Rust client](https://github.com/falkordb/falkordb-rs) ·
[FalkorDB next-gen Rust engine](https://github.com/FalkorDB/falkordb-rs-next-gen)
