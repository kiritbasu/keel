/**
 * The order tasks are in, in one place.
 *
 * The board and the detail view must agree: `J` and `K` walk the same sequence
 * the board shows, and a next-task key that jumps somewhere the eye did not
 * expect is worse than no key at all. Sharing the comparator is what makes that
 * true by construction rather than by both files being edited together.
 */

import type { Entity } from "./api";

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

/** p0 first, anything unrecognised last. */
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
