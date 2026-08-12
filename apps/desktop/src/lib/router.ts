/**
 * Addresses.
 *
 * Every screen, project, document, search and task has a URL. That is the whole
 * point: without it there is nothing to link to, nothing to bookmark, nothing
 * for Back to go back to, and a reload always lands on Home having forgotten
 * what you were looking at.
 *
 * Two decisions worth knowing about.
 *
 * **The hash, not the path.** A path-based router needs the server to fall back
 * to `index.html` for any deep URL. Vite's dev server does that; Tauri's asset
 * protocol does not, so `/projects/keel/board` would 404 on reload in the built
 * app — the exact thing routing was added to fix. The hash never reaches a
 * server, so the same bundle behaves identically in dev, in the Tauri webview
 * and in a future static web build, which is what SPEC §10 asks for.
 *
 * **Hand-written, not a dependency.** Eleven routes and a query string is less
 * code than the configuration a router library would need, and this way the
 * route table is one readable list. See DECISIONS B-14 for the same reasoning
 * applied to components.
 */

import { useCallback, useSyncExternalStore } from "react";

export type ScreenId =
  | "home"
  | "project"
  | "roadmap"
  | "board"
  | "ready"
  | "task"
  | "documents"
  | "search"
  | "changed";

/** Where the user is. Everything the app needs to render a screen. */
export interface Route {
  screen: ScreenId;
  /** Project slug, when the address names one. */
  project?: string;
  /** The document being read, when the address names one. */
  documentId?: string;
  /** The task being read, when the address names one. */
  taskId?: string;
  /** Everything after `?`. Filters live here so a filtered view is a link. */
  query: Record<string, string>;
}

/**
 * The route table, most specific first.
 *
 * Order matters only where two patterns could match the same path; keeping the
 * project-scoped forms above the bare ones makes that impossible to get wrong
 * by accident.
 */
const ROUTES: Array<{ pattern: string; screen: ScreenId }> = [
  { pattern: "/projects/:project/documents/:documentId", screen: "documents" },
  { pattern: "/projects/:project/documents", screen: "documents" },
  { pattern: "/projects/:project/tasks/:taskId", screen: "task" },
  { pattern: "/projects/:project/roadmap", screen: "roadmap" },
  { pattern: "/projects/:project/board", screen: "board" },
  { pattern: "/projects/:project/ready", screen: "ready" },
  { pattern: "/projects/:project/search", screen: "search" },
  { pattern: "/projects/:project/changed", screen: "changed" },
  { pattern: "/projects/:project", screen: "project" },
  { pattern: "/roadmap", screen: "roadmap" },
  { pattern: "/search", screen: "search" },
  { pattern: "/changed", screen: "changed" },
  { pattern: "/", screen: "home" },
];

/** Which screens are meaningless without a project selected. */
export const NEEDS_PROJECT: Record<ScreenId, boolean> = {
  home: false,
  project: true,
  roadmap: false,
  board: true,
  ready: true,
  task: true,
  documents: true,
  search: false,
  changed: false,
};

function segments(path: string): string[] {
  return path.split("/").filter(Boolean);
}

/**
 * Turn a hash into a route.
 *
 * An address that matches nothing resolves to Home rather than to an error
 * screen. A stale bookmark or a typo is not worth a dead end, and there is no
 * state to lose — the reader is one click from anywhere.
 */
export function parseHash(hash: string): Route {
  const raw = hash.replace(/^#/, "");
  const [pathPart = "", queryPart = ""] = raw.split("?", 2);
  const path = pathPart || "/";

  const query: Record<string, string> = {};
  for (const [key, value] of new URLSearchParams(queryPart)) query[key] = value;

  const actual = segments(path);
  for (const route of ROUTES) {
    const expected = segments(route.pattern);
    if (expected.length !== actual.length) continue;

    const params: Record<string, string> = {};
    let matched = true;
    for (let i = 0; i < expected.length; i++) {
      const want = expected[i] ?? "";
      const got = actual[i] ?? "";
      if (want.startsWith(":")) params[want.slice(1)] = decodeURIComponent(got);
      else if (want !== got) {
        matched = false;
        break;
      }
    }
    if (!matched) continue;

    return {
      screen: route.screen,
      ...(params.project ? { project: params.project } : {}),
      ...(params.documentId ? { documentId: params.documentId } : {}),
      ...(params.taskId ? { taskId: params.taskId } : {}),
      query,
    };
  }

  return { screen: "home", query };
}

/**
 * Turn a route into a hash.
 *
 * Round-trips with `parseHash`: `parseHash(toHash(r))` is `r` for every route
 * the app can construct. A test holds that, because a link that does not come
 * back as the thing it was built from is a silent navigation bug.
 */
export function toHash(route: Route): string {
  const project = route.project ? encodeURIComponent(route.project) : undefined;
  let path: string;
  switch (route.screen) {
    case "home":
      path = "/";
      break;
    case "project":
      path = project ? `/projects/${project}` : "/";
      break;
    case "documents":
      path = project
        ? route.documentId
          ? `/projects/${project}/documents/${encodeURIComponent(route.documentId)}`
          : `/projects/${project}/documents`
        : "/";
      break;
    case "board":
      path = project ? `/projects/${project}/board` : "/";
      break;
    // Ready is about one project's work, so with no project there is nothing to
    // rank. Home rather than an empty screen with a title.
    case "ready":
      path = project ? `/projects/${project}/ready` : "/";
      break;
    // A task with no id is not an address. Falling back to the board rather
    // than to Home keeps the reader in the same project, which is where they
    // were trying to be.
    case "task":
      path =
        project && route.taskId
          ? `/projects/${project}/tasks/${encodeURIComponent(route.taskId)}`
          : project
            ? `/projects/${project}/board`
            : "/";
      break;
    case "roadmap":
      path = project ? `/projects/${project}/roadmap` : "/roadmap";
      break;
    case "search":
      path = project ? `/projects/${project}/search` : "/search";
      break;
    case "changed":
      path = project ? `/projects/${project}/changed` : "/changed";
      break;
  }

  // Empty values are dropped rather than written as `?q=`, so two views that
  // are the same view have the same address.
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(route.query ?? {})) {
    if (value !== "" && value !== undefined) params.set(key, value);
  }
  const search = params.toString();
  return `#${path}${search ? `?${search}` : ""}`;
}

/** A route's `href`, for an anchor. Anchors, so middle-click and copy-link work. */
export function href(route: Partial<Route> & { screen: ScreenId }): string {
  return toHash({ query: {}, ...route });
}

/**
 * Go somewhere.
 *
 * `replace` is for corrections that should not become a Back destination — a
 * screen that needs a project, reached without one, is not a place the reader
 * chose to be.
 */
export function navigate(
  route: Partial<Route> & { screen: ScreenId },
  options?: { replace?: boolean },
): void {
  const hash = toHash({ query: {}, ...route });
  if (options?.replace) {
    const url = `${window.location.pathname}${window.location.search}${hash}`;
    window.history.replaceState(null, "", url);
    // replaceState does not fire hashchange, so the app would keep rendering
    // the old route against the new address.
    window.dispatchEvent(new HashChangeEvent("hashchange"));
  } else {
    window.location.hash = hash;
  }
}

/** Amend the query of the current route, keeping the path. Filters use this. */
export function setQuery(
  route: Route,
  changes: Record<string, string | undefined>,
  options?: { replace?: boolean },
): void {
  const query = { ...route.query };
  for (const [key, value] of Object.entries(changes)) {
    if (value === undefined || value === "") delete query[key];
    else query[key] = value;
  }
  navigate({ ...route, query }, options);
}

function subscribe(onChange: () => void): () => void {
  window.addEventListener("hashchange", onChange);
  return () => window.removeEventListener("hashchange", onChange);
}

/** The current route, re-rendering on Back, Forward and any navigation. */
export function useRoute(): Route {
  const hash = useSyncExternalStore(
    subscribe,
    () => window.location.hash,
    () => "",
  );
  // Parsed per render rather than memoised: it is a string split, and caching it
  // would mean caring about identity in every dependency array downstream.
  return parseHash(hash);
}

/** `navigate`, as a stable callback, for components that would rather not import it. */
export function useNavigate(): typeof navigate {
  return useCallback(navigate, []);
}
