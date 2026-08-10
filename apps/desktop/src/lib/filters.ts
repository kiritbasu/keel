/**
 * What you are looking at, expressed as an address.
 *
 * The board previously had exactly two filters, they did not combine, and they
 * were lost on reload — so "p0 bugs in the current milestone" was something you
 * reassembled by hand every time and could not send to anyone.
 *
 * Putting the filter in the URL is what fixes all three at once. A filtered
 * view *is* a link, which means saved views come for free: bookmark it. And
 * because the address is the state, Back undoes a filter change the way it
 * undoes anything else.
 *
 * The encoding is deliberately plain — `?status=todo,blocked&label=desktop` —
 * because these URLs get read, edited and pasted by people, and a base64 blob
 * would be shorter and useless.
 */

import type { Entity } from "./api";

/** Everything that narrows the task list. Empty arrays mean "no restriction". */
export interface TaskFilter {
  status: string[];
  priority: string[];
  kind: string[];
  labels: string[];
  /** Milestone id, or the literal `none` for tasks belonging to no milestone. */
  milestone: string | undefined;
  /** Only tasks something is linked to as a blocker. */
  blocked: boolean;
  /** Free text over the title and body. */
  text: string;
}

export const EMPTY_FILTER: TaskFilter = {
  status: [],
  priority: [],
  kind: [],
  labels: [],
  milestone: undefined,
  blocked: false,
  text: "",
};

function list(value: string | undefined): string[] {
  return (value ?? "")
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

/** Read the filter out of a route's query. */
export function parseFilter(query: Record<string, string>): TaskFilter {
  return {
    status: list(query.status),
    priority: list(query.priority),
    kind: list(query.kind),
    labels: list(query.label),
    milestone: query.milestone || undefined,
    blocked: query.blocked === "true",
    text: query.q ?? "",
  };
}

/**
 * Write the filter back into query parameters.
 *
 * Every empty value becomes `undefined` rather than an empty string, so
 * `setQuery` drops it: two views that are the same view get the same address,
 * and an unfiltered board is `#/projects/keel/board` rather than a URL trailing
 * seven empty parameters.
 */
export function filterToQuery(filter: TaskFilter): Record<string, string | undefined> {
  return {
    status: filter.status.join(",") || undefined,
    priority: filter.priority.join(",") || undefined,
    kind: filter.kind.join(",") || undefined,
    label: filter.labels.join(",") || undefined,
    milestone: filter.milestone || undefined,
    blocked: filter.blocked ? "true" : undefined,
    q: filter.text || undefined,
  };
}

/** Whether anything is actually being narrowed. */
export function isFiltering(filter: TaskFilter): boolean {
  return (
    filter.status.length > 0 ||
    filter.priority.length > 0 ||
    filter.kind.length > 0 ||
    filter.labels.length > 0 ||
    filter.milestone !== undefined ||
    filter.blocked ||
    filter.text.trim() !== ""
  );
}

/** How many separate conditions are in force, for a "clear (3)" affordance. */
export function activeCount(filter: TaskFilter): number {
  return (
    filter.status.length +
    filter.priority.length +
    filter.kind.length +
    filter.labels.length +
    (filter.milestone ? 1 : 0) +
    (filter.blocked ? 1 : 0) +
    (filter.text.trim() ? 1 : 0)
  );
}

/** Add or remove one value from a multi-valued facet. */
export function toggle(values: string[], value: string): string[] {
  return values.includes(value) ? values.filter((v) => v !== value) : [...values, value];
}

/**
 * Narrow a task list.
 *
 * Conditions combine with AND across facets and OR within one — the way every
 * tracker behaves, and the only reading under which "status: todo, blocked"
 * means anything useful. `blockedIds` is passed in rather than derived here
 * because being blocked is a fact about the *graph*, and the app must not grow
 * a second opinion about what blocked means (see the "one definition of
 * blocked" task).
 */
export function applyFilter(
  tasks: Entity[],
  filter: TaskFilter,
  blockedIds: ReadonlySet<string>,
): Entity[] {
  const needle = filter.text.trim().toLowerCase();

  return tasks.filter((task) => {
    if (filter.status.length && !filter.status.includes(String(task.status))) return false;
    if (filter.priority.length && !filter.priority.includes(String(task.priority))) return false;
    if (filter.kind.length && !filter.kind.includes(String(task.kind))) return false;

    if (filter.labels.length) {
      const labels = (task.labels as string[] | undefined) ?? [];
      if (!filter.labels.some((wanted) => labels.includes(wanted))) return false;
    }

    if (filter.milestone) {
      const on = task.milestone_id ? String(task.milestone_id) : "none";
      if (on !== filter.milestone) return false;
    }

    if (filter.blocked && !blockedIds.has(String(task.id))) return false;

    if (needle) {
      // Title and body, because a task's detail is where the searchable words
      // usually are — the title is one line and often generic.
      const haystack = `${String(task.title ?? "")} ${String(task.body ?? "")}`.toLowerCase();
      if (!haystack.includes(needle)) return false;
    }

    return true;
  });
}
