/**
 * The order tasks are in, in one place.
 *
 * The board and the detail view must agree: `J` and `K` walk the same sequence
 * the board shows, and a next-task key that jumps somewhere the eye did not
 * expect is worse than no key at all. Sharing the comparator is what makes that
 * true by construction rather than by both files being edited together.
 */

import type { Entity } from "./api";

/**
 * A task's readable identifier: `KEEL-42`.
 *
 * Composed here rather than stored, so that re-keying a project does not mean
 * rewriting every row that mentions it. Returns the ULID when the key is not to
 * hand — that is still a working address, just not a readable one, and showing
 * `undefined-42` would be worse than showing the long form.
 */
export function taskRef(key: string | undefined, task: Entity): string {
  const number = task.number;
  if (!key || typeof number !== "number" || number <= 0) return String(task.id);
  return `${key}-${number}`;
}

/** Lifecycle order, left to right. Matches TaskStatus::ALL. */
export const COLUMNS = ["todo", "in_progress", "blocked", "review", "done", "wont_do"] as const;

export type Column = (typeof COLUMNS)[number];

/** Where a rank position and its reason come from — the digest's `next_up`. */
export type RankMap = Map<string, { position: number; why: string }>;

/**
 * Compare two tasks the way the board does.
 *
 * Ranked work first, in rank order — a column that displays "3" above "1" is
 * showing a ranking and contradicting it in the same breath. Then priority, and
 * priority is compared by its position in `p0…p3` rather than as text: sorting
 * the strings puts a task with no priority under the literal word "undefined",
 * which is where several of them were.
 */
export function compareTasks(rank: RankMap) {
  return (a: Entity, b: Entity): number => {
    const ra = rank.get(String(a.id))?.position ?? Infinity;
    const rb = rank.get(String(b.id))?.position ?? Infinity;
    if (ra !== rb) return ra - rb;

    const pa = priorityIndex(a.priority);
    const pb = priorityIndex(b.priority);
    if (pa !== pb) return pa - pb;

    return String(a.title ?? "").localeCompare(String(b.title ?? ""));
  };
}

/** p0 first, anything unrecognised last. Never a string comparison. */
function priorityIndex(priority: unknown): number {
  const at = ["p0", "p1", "p2", "p3"].indexOf(String(priority));
  return at === -1 ? 99 : at;
}

/**
 * Every task in board reading order: down each column, then on to the next.
 *
 * This is the sequence `J` and `K` walk.
 */
export function inBoardOrder(tasks: Entity[], rank: RankMap): Entity[] {
  const compare = compareTasks(rank);
  return COLUMNS.flatMap((column) =>
    tasks.filter((t) => String(t.status) === column).sort(compare),
  );
}

// --- Grouping and sorting ------------------------------------------------

/** What the columns, or the list's section headings, are cut by. */
export type GroupBy = "status" | "priority" | "milestone" | "label" | "parent" | "none";

/**
 * What orders tasks inside a group.
 *
 * Two of these are orderings rather than fields, and they are different
 * things: `next` is the ranking the digest computes and gives an agent, and
 * `rank` is the deliberate order a person put the tasks in. Naming them apart
 * matters — they were briefly the same word, and a sort that silently means
 * one when you asked for the other is the kind of thing nobody reports.
 */
export type SortBy = "next" | "rank" | "priority" | "status" | "updated" | "title" | "number";

export type SortDir = "asc" | "desc";

export const GROUP_BY: GroupBy[] = [
  "status",
  "priority",
  "milestone",
  "label",
  "parent",
  "none",
];
export const SORT_BY: SortBy[] = [
  "next",
  "rank",
  "priority",
  "status",
  "updated",
  "title",
  "number",
];

/** One column, or one section of the list. */
export interface Group {
  /** Stable identity — a status name, a priority, a milestone id, a label. */
  key: string;
  /** What to print at the top of it. */
  label: string;
  tasks: Entity[];
}

const PRIORITIES = ["p0", "p1", "p2", "p3"];

/**
 * Cut a task list into groups.
 *
 * Grouping by label is the one that behaves differently: a task with three
 * labels appears under all three. That is the honest rendering — the
 * alternative is picking one of its labels arbitrarily and hiding it from the
 * other two — but it does mean the group sizes sum to more than the task count,
 * which is why the header says how many *tasks* match rather than adding the
 * columns up.
 *
 * Everything ends with a "none" bucket rather than dropping the tasks that have
 * no value for the grouping field. A task with no milestone is not nothing; it
 * is a task nobody has scheduled, which is usually the interesting group.
 */
export function groupTasks(
  tasks: Entity[],
  by: GroupBy,
  names: ReadonlyMap<string, string>,
): Group[] {
  if (by === "none") {
    return [{ key: "all", label: "All", tasks }];
  }

  if (by === "status") {
    return COLUMNS.map((status) => ({
      key: status,
      label: status.replace("_", " "),
      tasks: tasks.filter((t) => String(t.status) === status),
    }));
  }

  if (by === "priority") {
    const groups = PRIORITIES.map((priority) => ({
      key: priority,
      label: priority,
      tasks: tasks.filter((t) => String(t.priority) === priority),
    }));
    const rest = tasks.filter((t) => !PRIORITIES.includes(String(t.priority)));
    if (rest.length) groups.push({ key: "none", label: "no priority", tasks: rest });
    return groups;
  }

  if (by === "milestone") {
    const ids = [...new Set(tasks.map((t) => (t.milestone_id ? String(t.milestone_id) : "")))]
      .filter(Boolean)
      .sort((a, b) => (names.get(a) ?? a).localeCompare(names.get(b) ?? b));
    const groups = ids.map((id) => ({
      key: id,
      label: names.get(id) ?? id,
      tasks: tasks.filter((t) => String(t.milestone_id) === id),
    }));
    const rest = tasks.filter((t) => !t.milestone_id);
    if (rest.length) groups.push({ key: "none", label: "no milestone", tasks: rest });
    return groups;
  }

  if (by === "parent") {
    // One group per parent that something in the filtered set belongs to. The
    // parent itself need not be in the set — a filter for open work should
    // still say which epic each piece is part of.
    const ids = [...new Set(tasks.map((t) => (t.parent_id ? String(t.parent_id) : "")))]
      .filter(Boolean)
      .sort((a, b) => (names.get(a) ?? a).localeCompare(names.get(b) ?? b));
    const groups = ids.map((id) => ({
      key: id,
      label: names.get(id) ?? id,
      tasks: tasks.filter((t) => String(t.parent_id) === id),
    }));
    const rest = tasks.filter((t) => !t.parent_id);
    if (rest.length) groups.push({ key: "none", label: "not part of anything", tasks: rest });
    return groups;
  }

  const labels = [
    ...new Set(tasks.flatMap((t) => ((t.labels as string[] | undefined) ?? []) as string[])),
  ].sort();
  const groups = labels.map((label) => ({
    key: label,
    label,
    tasks: tasks.filter((t) => ((t.labels as string[] | undefined) ?? []).includes(label)),
  }));
  const unlabelled = tasks.filter((t) => (((t.labels as string[] | undefined) ?? []).length === 0));
  if (unlabelled.length) groups.push({ key: "none", label: "no label", tasks: unlabelled });
  return groups;
}

const STATUS_INDEX = new Map(COLUMNS.map((status, i) => [status as string, i]));

/**
 * Order tasks within a group.
 *
 * `next` is the default and means the ranking the digest gives an agent, so the
 * board and the model agree about what to do next. `rank` is the separate,
 * deliberate order a person put the tasks in. Everything else is a property of
 * the row.
 *
 * Comparison is always on a typed key rather than on the stringified value.
 * The old board compared priorities as text, which sorted a task with no
 * priority under the literal word "undefined" — visible only as a column whose
 * order looked arbitrary.
 */
export function sortTasks(
  tasks: Entity[],
  by: SortBy,
  dir: SortDir,
  rank: RankMap,
): Entity[] {
  const sign = dir === "desc" ? -1 : 1;
  const sorted = [...tasks];

  if (by === "next") {
    sorted.sort(compareTasks(rank));
    return dir === "desc" ? sorted.reverse() : sorted;
  }

  sorted.sort((a, b) => {
    switch (by) {
      case "rank":
        return sign * (Number(a.rank ?? 0) - Number(b.rank ?? 0));
      case "priority":
        return sign * (priorityIndex(a.priority) - priorityIndex(b.priority));
      case "status":
        return (
          sign *
          ((STATUS_INDEX.get(String(a.status)) ?? 99) - (STATUS_INDEX.get(String(b.status)) ?? 99))
        );
      case "number":
        return sign * (Number(a.number ?? 0) - Number(b.number ?? 0));
      case "updated":
        return (
          sign *
          (Date.parse(String(a.audit?.updated_at ?? 0)) -
            Date.parse(String(b.audit?.updated_at ?? 0)))
        );
      default:
        return sign * String(a.title ?? "").localeCompare(String(b.title ?? ""));
    }
  });
  return sorted;
}
