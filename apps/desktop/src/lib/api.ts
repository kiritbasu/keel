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
   */
  history: (entity: string, limit = 500) =>
    get<{ events: EventRow[]; total: number; truncated: boolean }>("/api/activity", {
      entity,
      limit,
    }),

  entities: (params: {
    project?: string;
    type?: string;
    status?: string;
    limit?: number;
  }) => get<Page<Entity>>("/api/entities", { ...params, limit: params.limit ?? 500 }),

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
};

/**
 * Subscribe to change notifications.
 *
 * The daemon emits a `lagged` event when a subscriber has fallen behind and
 * lost messages. That is surfaced rather than swallowed: a UI that missed
 * changes should refetch, and quietly continuing would leave it showing stale
 * state indefinitely.
 */
export function subscribe(onChange: () => void): () => void {
  const source = new EventSource(`${BASE}/api/events`);
  source.addEventListener("change", () => onChange());
  source.addEventListener("lagged", () => onChange());
  return () => source.close();
}
