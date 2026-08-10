import { describe, expect, it } from "vitest";
import {
  compareTasks,
  groupTasks,
  inBoardOrder,
  sortTasks,
  taskRef,
  type RankMap,
} from "./tasks";
import type { Entity } from "./api";

function task(id: string, fields: Partial<Entity> = {}): Entity {
  return { id, type: "task", audit: {} as Entity["audit"], status: "todo", ...fields } as Entity;
}

const noRank: RankMap = new Map();

describe("compareTasks", () => {
  it("puts ranked work first, in rank order", () => {
    const rank: RankMap = new Map([
      ["b", { position: 1, why: "unblocks 4" }],
      ["a", { position: 2, why: "unblocks 1" }],
    ]);
    const sorted = [task("a"), task("b"), task("c")].sort(compareTasks(rank));
    expect(sorted.map((t) => t.id)).toEqual(["b", "a", "c"]);
  });

  it("then orders by priority, p0 first", () => {
    const sorted = [
      task("c", { priority: "p2" }),
      task("a", { priority: "p0" }),
      task("b", { priority: "p1" }),
    ].sort(compareTasks(noRank));
    expect(sorted.map((t) => t.id)).toEqual(["a", "b", "c"]);
  });

  // The bug this replaced: priorities were compared as strings, so a task with
  // no priority stringified to "undefined" and sorted between p3 and nothing —
  // in practice it landed above p0 or below p3 depending on the neighbour, and
  // the column order looked arbitrary.
  it("sorts a task with no priority last, not under the word 'undefined'", () => {
    const sorted = [task("none"), task("urgent", { priority: "p0" })].sort(compareTasks(noRank));
    expect(sorted.map((t) => t.id)).toEqual(["urgent", "none"]);

    const stringSort = [task("none"), task("urgent", { priority: "p0" })].sort((a, b) =>
      String(a.priority).localeCompare(String(b.priority)),
    );
    expect(stringSort.map((t) => t.id)).toEqual(["urgent", "none"]);
    // …and the case that made it visible: "undefined" sorts *below* p3 by text,
    // so an unprioritised task jumped ahead of nothing and behind everything,
    // which is only correct by accident.
    const p3First = [task("low", { priority: "p3" }), task("none")].sort((a, b) =>
      String(a.priority).localeCompare(String(b.priority)),
    );
    expect(p3First.map((t) => t.id)).toEqual(["low", "none"]);
  });

  it("falls back to the title so the order is stable rather than insertion order", () => {
    const sorted = [task("b", { title: "Zebra" }), task("a", { title: "Apple" })].sort(
      compareTasks(noRank),
    );
    expect(sorted.map((t) => t.id)).toEqual(["a", "b"]);
  });
});

describe("inBoardOrder", () => {
  it("walks down each column, then on to the next", () => {
    const tasks = [
      task("done1", { status: "done" }),
      task("todo1", { status: "todo", priority: "p1" }),
      task("todo0", { status: "todo", priority: "p0" }),
      task("prog", { status: "in_progress" }),
    ];
    expect(inBoardOrder(tasks, noRank).map((t) => t.id)).toEqual([
      "todo0",
      "todo1",
      "prog",
      "done1",
    ]);
  });

  // Failure case: a status the board has no column for must not silently
  // vanish from the sequence J and K walk without also vanishing from the
  // board — they are the same list by construction.
  it("drops a task whose status has no column, exactly as the board does", () => {
    const tasks = [task("ok", { status: "todo" }), task("odd", { status: "not_a_status" })];
    expect(inBoardOrder(tasks, noRank).map((t) => t.id)).toEqual(["ok"]);
  });

  it("is empty for no tasks rather than throwing", () => {
    expect(inBoardOrder([], noRank)).toEqual([]);
  });
});

describe("taskRef", () => {
  it("composes the readable identifier", () => {
    expect(taskRef("KEEL", task("tsk_1", { number: 42 }))).toBe("KEEL-42");
  });

  // Failure cases. Both fall back to the ULID, which is still a working
  // address — `undefined-42` would be neither readable nor resolvable, and
  // would be rendered into links and copied out of them.
  it("falls back to the id when the key has not arrived yet", () => {
    expect(taskRef(undefined, task("tsk_1", { number: 42 }))).toBe("tsk_1");
    expect(taskRef("", task("tsk_1", { number: 42 }))).toBe("tsk_1");
  });

  it("falls back to the id when the task has no number", () => {
    expect(taskRef("KEEL", task("tsk_1"))).toBe("tsk_1");
    expect(taskRef("KEEL", task("tsk_1", { number: 0 }))).toBe("tsk_1");
  });
});

describe("groupTasks", () => {
  const noMilestones = new Map<string, string>();

  it("groups by status in lifecycle order, keeping empty columns", () => {
    const groups = groupTasks([task("a", { status: "done" })], "status", noMilestones);
    expect(groups.map((g) => g.key)).toEqual([
      "todo",
      "in_progress",
      "blocked",
      "review",
      "done",
      "wont_do",
    ]);
    // An empty column is information: it says nothing is in review.
    expect(groups.find((g) => g.key === "todo")?.tasks).toEqual([]);
  });

  it("groups by priority and gathers the unprioritised at the end", () => {
    const groups = groupTasks(
      [task("a", { priority: "p0" }), task("b")],
      "priority",
      noMilestones,
    );
    expect(groups.at(-1)?.key).toBe("none");
    expect(groups.at(-1)?.tasks.map((t) => t.id)).toEqual(["b"]);
  });

  it("names milestones rather than showing their ids", () => {
    const names = new Map([["mst_1", "Phase 6"]]);
    const groups = groupTasks([task("a", { milestone_id: "mst_1" })], "milestone", names);
    expect(groups[0]?.label).toBe("Phase 6");
  });

  // Grouping by label is the one that behaves differently, and it is the
  // honest behaviour: hiding a task from two of its three labels would be
  // worse than the group sizes summing to more than the task count.
  it("puts a multi-labelled task under every one of its labels", () => {
    const groups = groupTasks([task("a", { labels: ["desktop", "mcp"] })], "label", noMilestones);
    expect(groups.map((g) => g.key)).toEqual(["desktop", "mcp"]);
    expect(groups.every((g) => g.tasks.length === 1)).toBe(true);
  });

  it("collects tasks with no value for the grouping field rather than dropping them", () => {
    const groups = groupTasks([task("a")], "label", noMilestones);
    expect(groups.map((g) => g.key)).toEqual(["none"]);
    expect(groups[0]?.tasks.map((t) => t.id)).toEqual(["a"]);
  });

  it("makes one group when grouping is off", () => {
    const groups = groupTasks([task("a"), task("b")], "none", noMilestones);
    expect(groups).toHaveLength(1);
    expect(groups[0]?.tasks).toHaveLength(2);
  });
});

describe("sortTasks", () => {
  const noRank: RankMap = new Map();

  it("sorts by priority with the unprioritised last, in both directions", () => {
    const tasks = [task("none"), task("low", { priority: "p3" }), task("hot", { priority: "p0" })];
    expect(sortTasks(tasks, "priority", "asc", noRank).map((t) => t.id)).toEqual([
      "hot",
      "low",
      "none",
    ]);
    expect(sortTasks(tasks, "priority", "desc", noRank).map((t) => t.id)).toEqual([
      "none",
      "low",
      "hot",
    ]);
  });

  it("sorts by number, not by the string of it", () => {
    const tasks = [task("b", { number: 9 }), task("a", { number: 10 })];
    // A string sort would put "10" before "9".
    expect(sortTasks(tasks, "number", "asc", noRank).map((t) => t.id)).toEqual(["b", "a"]);
  });

  it("sorts by when it was last touched", () => {
    const tasks = [
      task("old", { audit: { updated_at: "2026-01-01T00:00:00Z" } as Entity["audit"] }),
      task("new", { audit: { updated_at: "2026-08-01T00:00:00Z" } as Entity["audit"] }),
    ];
    expect(sortTasks(tasks, "updated", "desc", noRank).map((t) => t.id)).toEqual(["new", "old"]);
  });

  it("uses the digest's ranking when asked for rank, so the app and the model agree", () => {
    const rank: RankMap = new Map([["second", { position: 1, why: "" }]]);
    const tasks = [task("first", { priority: "p0" }), task("second", { priority: "p3" })];
    expect(sortTasks(tasks, "rank", "asc", rank).map((t) => t.id)).toEqual(["second", "first"]);
  });

  // Failure case: sorting must not mutate its input, or the board reorders
  // whatever else is holding the same array.
  it("leaves the array it was given alone", () => {
    const tasks = [task("b", { priority: "p3" }), task("a", { priority: "p0" })];
    sortTasks(tasks, "priority", "asc", noRank);
    expect(tasks.map((t) => t.id)).toEqual(["b", "a"]);
  });
});
