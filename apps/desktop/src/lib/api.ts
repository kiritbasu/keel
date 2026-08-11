/**
 * Typed client for the daemon's local API.
 *
 * Every path is relative. In development Vite proxies `/api` to the daemon; in
 * the Tauri build the daemon runs as a sidecar on the same origin. That is
 * deliberate — SPEC §10 says the web build should be the same bundle with a
 * different base URL, and hard-coding `http://127.0.0.1:7654` here would make
 * that a rewrite rather than a config change.
 */

const BASE = import.meta.env.VITE_KEEL_BASE ?? "";

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
    this.name = "ApiError";
  }
}

async function get<T>(path: string, params?: Record<string, string | number | undefined>): Promise<T> {
  const url = new URL(`${BASE}${path}`, window.location.origin);
  for (const [key, value] of Object.entries(params ?? {})) {
    if (value !== undefined && value !== "") url.searchParams.set(key, String(value));
  }

  let response: Response;
  try {
    response = await fetch(url.toString());
  } catch {
    // The daemon being down is the single most likely failure, and "Failed to
    // fetch" tells a human nothing about what to do. Say what is wrong.
    throw new ApiError(
      "Cannot reach the Keel daemon. Start it with `keel-daemon` and try again.",
      0,
    );
  }

  const body = await response.json().catch(() => null);
  if (!response.ok) {
    throw new ApiError(body?.error?.message ?? `Request failed (${response.status})`, response.status);
  }
  return (body?.data ?? body) as T;
}

// --- Shapes the daemon returns ------------------------------------------

export interface Audit {
  created_at: string;
  updated_at: string;
  version: number;
  created_by: Actor;
  updated_by: Actor;
  session_id: string | null;
  surface: string | null;
  archived_at: string | null;
}

export type Actor = "human" | "claude" | "github" | "system";

export interface Entity {
  type: string;
  id: string;
  audit: Audit;
  [key: string]: unknown;
}

export interface ProjectLine {
  id: string;
  name: string;
  slug: string;
  /** The prefix of this project's readable identifiers — the `KEEL` of `KEEL-42`. */
  key: string;
  status: string;
  open_tasks: number;
  urgent_tasks: number;
  blocked_tasks: number;
  open_questions: number;
  active_milestone: string | null;
}

export interface DigestItem {
  id: string;
  entity_type: string;
  label: string;
  /** `KEEL-42`, for the types that have one. Tasks only, today. */
  reference?: string;
  status: string | null;
  detail?: string;
}

export interface TermEntry {
  term: string;
  definition: string;
  global: boolean;
}

export interface Truncation {
  section: string;
  shown: number;
  total: number;
}

export interface Digest {
  project: ProjectLine | null;
  projects: ProjectLine[];
  active: DigestItem[];
  attention: DigestItem[];
  recent: string[];
  decisions: DigestItem[];
  questions: DigestItem[];
  specs: DigestItem[];
  terms: TermEntry[];
  environments: DigestItem[];
  next: string[];
  next_up: NextUp | null;
  truncated: Truncation[];
  budget_exceeded: boolean;
  estimated_tokens: number;
}

/** The ranked answer to "what do I do next". Same ranking the digest gives an agent. */
export interface NextUp {
  ready: NextItem[];
  waiting_on_you: NextItem[];
  blocked: NextItem[];
}

export interface NextItem {
  id: string;
  /** `KEEL-42` — what a person will type back at Claude. */
  reference: string;
  title: string;
  priority: string;
  unblocks: number;
  why: string;
}

export interface SearchHit {
  entity_id: string;
  entity_type: string;
  project_id: string | null;
  title: string;
  excerpt: string;
  score: number;
  source: "keyword" | "semantic" | "both";
}

export interface EventRow {
  id: string;
  project_id: string | null;
  entity_type: string;
  entity_id: string;
  action: string;
  field: string | null;
  before: unknown;
  after: unknown;
  actor: Actor;
  session_id: string | null;
  surface: string | null;
  summary: string;
  created_at: string;
}

export interface Revision {
  version: number;
  title: string;
  author: Actor;
  session_id: string | null;
  surface: string | null;
  created_at: string;
  status: string;
}

export interface DocumentBody {
  version: number;
  title: string;
  body: string;
  created_at: string;
  author: Actor;
}

export interface Diff {
  from_version: number;
  to_version: number;
  unified: string;
  added: number;
  removed: number;
}

export interface Neighbour {
  id: string;
  entity_type: string;
  rel: string;
  /** What it is called. Carried by the traversal so a caller need not re-fetch. */
  label: string;
  anchor: string;
  depth: number;
  path: string[];
}

/** Every list the daemon returns says whether it was cut, and by how much. */
/** One entry in a row's running commentary. */
export interface Note {
  id: string;
  project_id: string | null;
  entity_type: string;
  entity_id: string;
  body: string;
  author: Actor;
  session_id: string | null;
  surface: string | null;
  created_at: string;
  archived_at: string | null;
}

export interface Page<T> {
  items: T[];
  total: number;
  truncated: boolean;
}

// --- Calls ---------------------------------------------------------------

export const api = {
  health: () =>
    get<{ status: string; protocol: string; version: string; projects: number }>("/api/health"),

  /** The digest. No `project` gives the cross-project roll-up. */
  context: (project?: string) => get<Digest>("/api/context", { project, depth: "full" }),

  projects: () => get<{ projects: Entity[] }>("/api/projects"),

  /**
   * What can be worked on right now, ranked.
   *
   * The same `keel_ready` a session calls, not a second ranking computed here.
   * That is the point of the endpoint existing at all — an app that ordered the
   * work differently from the tool would make "what next" a question with two
   * answers.
   */
  ready: (params: {
    project: string;
    unclaimed?: string;
    milestone?: string;
    limit?: number;
  }) => get<{ ready: NextItem[]; total: number; truncated: boolean }>("/api/ready", params),

  /**
   * A project's notes, in one call.
   *
   * Fetched for the whole project rather than per card: a board showing
   * seventy tasks would otherwise open seventy requests to render a count.
   */
  notes: (project?: string) => get<{ notes: Note[]; total: number }>("/api/notes", { project }),

  /**
   * One row's notes, retracted ones included.
   *
   * A detail view shows a retracted note struck through rather than hiding it:
   * what a session once believed is part of how the row got here, and silently
   * dropping it rewrites the record.
   */
  notesFor: (entity: string) =>
    get<{ notes: Note[]; total: number }>("/api/notes", { entity, all: "true" }),

  /**
   * One row's history — every status and field change, with before and after.
   *
   * The event log has always held this and nothing has ever shown it.
   *
   * Its own endpoint rather than `/api/activity?entity=`, because that route is
   * the `keel_activity` tool and the tool no longer takes an entity (TQ-24).
   * B-15 is the rule this follows: the local API has more endpoints than the
   * tool surface has tools, since a UI knows what it wants and a model chooses
   * worse among more options.
   */
  history: (entity: string, limit = 500) =>
    get<{ events: EventRow[]; total: number; truncated: boolean }>(
      `/api/entity/${encodeURIComponent(entity)}/history`,
      { limit },
    ),

  entities: (params: {
    project?: string;
    type?: string;
    status?: string;
    limit?: number;
  }) => get<Page<Entity>>("/api/entities", { ...params, limit: params.limit ?? 500 }),

  /**
   * Metrics and their observations, for one project.
   *
   * Two reads rather than one endpoint: observations are their own artifact
   * type, and a metric with a thousand points should not be forced through the
   * same response as one with three.
   */
  metrics: (project: string) =>
    get<{ items: Entity[]; total: number }>("/api/entities", {
      project,
      type: "metric",
      limit: 200,
    }),

  observations: (project: string) =>
    get<{ items: Entity[]; total: number }>("/api/entities", {
      project,
      type: "metric_observation",
      limit: 5000,
    }),

  entity: (id: string, depth = 0) =>
    get<{ artifacts: Array<{ entity: Entity; document?: DocumentBody; neighbours?: Neighbour[] }> }>(
      `/api/entity/${id}`,
      { depth },
    ),

  document: (id: string, version?: number, diffAgainst?: number) =>
    get<{ revisions: Revision[]; document: DocumentBody | null; diff: Diff | null }>(
      `/api/document/${id}`,
      { version, diff_against: diffAgainst },
    ),

  graph: (id: string, direction: "outbound" | "inbound" | "both" = "both", depth = 2) =>
    get<{ neighbours: Neighbour[] }>(`/api/graph/${id}`, { direction, depth }),

  search: (query: string, params?: { project?: string; types?: string; limit?: number }) =>
    get<Page<SearchHit> & { hits: SearchHit[] }>("/api/search", { query, ...params }),

  activity: (params?: { project?: string; limit?: number; cursor?: string }) =>
    get<{ events: EventRow[]; total: number; truncated: boolean; cursor: string | null }>(
      "/api/activity",
      params,
    ),

  /**
   * What changed, grouped by the session that changed it.
   *
   * Its own endpoint rather than a shape on `/api/activity`, because that URL is
   * the `keel_activity` tool and this is a different question: the tool pages
   * every mutation from a cursor for a model catching up, and this answers "what
   * did each session do" for a person who left Claude working.
   *
   * The union with notes is the part that cannot be done here: a note leaves no
   * row in `events` (TQ-29), so a per-session count built from the feed alone
   * silently misses the part most worth reading.
   */
  changed: (params: {
    project?: string;
    actor?: string;
    since?: string;
    limit?: number;
  }) =>
    get<{
      sessions: Array<{
        session_id: string | null;
        actor: string;
        started_at: string;
        ended_at: string;
        headline: string;
        changes: Array<{
          id: string;
          kind: "field" | "created" | "note";
          entity_id: string;
          entity_type: string;
          reference: string;
          summary: string;
          at: string;
        }>;
      }>;
      changes: number;
      truncated: boolean;
    }>("/api/changes", params),
};

/**
 * Subscribe to change notifications.
 *
 * The daemon emits a `lagged` event when a subscriber has fallen behind and
 * lost messages. That is surfaced rather than swallowed: a UI that missed
 * changes should refetch, and quietly continuing would leave it showing stale
 * state indefinitely.
 */
export function subscribe(onChange: (change: ChangeEvent) => void): () => void {
  const source = new EventSource(`${BASE}/api/events`);
  const forward = (raw: MessageEvent | Event) => {
    const data = "data" in raw && typeof raw.data === "string" ? raw.data : null;
    let change: ChangeEvent = { kind: "entity", summary: "" };
    if (data) {
      try {
        change = { ...change, ...(JSON.parse(data) as ChangeEvent) };
      } catch {
        // A change we cannot parse is still a change. Refetching on it is the
        // safe direction: the cost is one wasted read, and the alternative is
        // showing stale state because a payload shape moved.
      }
    }
    onChange(change);
  };
  source.addEventListener("change", forward);
  source.addEventListener("lagged", forward);
  return () => source.close();
}

/** One announced write. */
export interface ChangeEvent {
  /**
   * `entity` for anything that wrote an event; `note` for a note.
   *
   * Notes are announced separately because they are not events, and the daemon
   * cannot see them by watching the event log (TQ-29).
   */
  kind: "entity" | "note";
  /** The row it is about, when known. */
  entity_id?: string;
  /** One line describing it. */
  summary: string;
}
