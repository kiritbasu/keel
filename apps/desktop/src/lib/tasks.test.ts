import { describe, expect, it } from "vitest";
import {
  COLUMNS,
  compareTasks,
  dropOnStatus,
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
    // No `blocked` — it is not a status (TQ-25).
    expect(groups.map((g) => g.key)).toEqual([
      "todo",
      "in_progress",
      "review",
      "done",
      "wont_do",
    ]);
    // An empty column is information: it says nothing is in review.
    expect(groups.find((g) => g.key === "todo")?.tasks).toEqual([]);
  });

  // An epic's progress is the one number a container exists to answer, and it
  // is read off the children rather than stored, so it cannot drift out of
  // step with the rows underneath it (KEEL-327).
  it("says how far through an epic is, counted from its children", () => {
    const names = new Map([["epic1", "Codex support"]]);
    const groups = groupTasks(
      [
        task("a", { parent_id: "epic1", status: "done" }),
        task("b", { parent_id: "epic1", status: "wont_do" }),
        task("c", { parent_id: "epic1", status: "todo" }),
        task("d", { parent_id: "epic1", status: "in_progress" }),
      ],
      "parent",
      names,
    );

    const epic = groups.find((g) => g.key === "epic1");
    expect(epic?.label).toBe("Codex support");
    expect(epic?.tasks).toHaveLength(4);
    // `wont_do` counts as finished. A child somebody decided against is not
    // outstanding work, and counting it as such would leave every epic that
    // dropped something permanently short of done.
    expect(epic?.done).toBe(2);
  });

  // Counted over the children the reader can actually see. An epic reporting
  // 3/8 under a filter that hides five of them would be describing rows that
  // are not on the screen.
  it("counts only the children a filter left in", () => {
    const groups = groupTasks(
      [task("a", { parent_id: "epic1", status: "done" })],
      "parent",
      new Map(),
    );
    expect(groups[0]?.done).toBe(1);
    expect(groups[0]?.tasks).toHaveLength(1);
  });

  // The container turning up twice is "one row, not eight" undone: a
  // four-child epic rendered as a heading plus five rows, and the group counts
  // summed to one more than the task total.
  it("does not also list an epic among the tasks that have no parent", () => {
    const groups = groupTasks(
      [
        task("epic1", { kind: "feature" }),
        task("a", { parent_id: "epic1", status: "todo" }),
      ],
      "parent",
      new Map([["epic1", "Codex support"]]),
    );

    expect(groups.map((g) => g.key)).toEqual(["epic1"]);
    expect(groups.flatMap((g) => g.tasks.map((t) => String(t.id)))).toEqual(["a"]);
  });

  // But an epic nobody has given children to is still work somebody has to
  // do, so it stays in the list rather than vanishing for being empty.
  it("keeps a childless epic among the loose rows", () => {
    const groups = groupTasks([task("epic1", { kind: "feature" })], "parent", new Map());
    expect(groups.find((g) => g.key === "none")?.tasks.map((t) => String(t.id))).toEqual([
      "epic1",
    ]);
  });

  // A fraction under a heading that is not a container would be a progress bar
  // for a thing that is not in progress.
  it("leaves the fraction off groupings where it would mean nothing", () => {
    const byStatus = groupTasks([task("a", { status: "done" })], "status", noMilestones);
    expect(byStatus.every((g) => g.done === undefined)).toBe(true);

    const byParent = groupTasks([task("loose")], "parent", new Map());
    expect(byParent.find((g) => g.key === "none")?.done).toBeUndefined();
  });

  // Blocked is a column, but a derived one, and it comes first because it is
  // the one that needs a person.
  it("pulls blocked work into its own column, out of whatever status it has", () => {
    const tasks = [task("stuck", { status: "todo" }), task("free", { status: "todo" })];
    const groups = groupTasks(tasks, "status", noMilestones, new Set(["stuck"]));
    expect(groups[0]?.key).toBe("blocked");
    expect(groups[0]?.tasks.map((t) => t.id)).toEqual(["stuck"]);
    expect(groups.find((g) => g.key === "todo")?.tasks.map((t) => t.id)).toEqual(["free"]);
  });

  // Failure case: a task in two columns would double every count on the board.
  it("shows a blocked task once, not in both columns", () => {
    const groups = groupTasks([task("stuck", { status: "todo" })], "status", noMilestones, new Set(["stuck"]));
    const appearances = groups.flatMap((g) => g.tasks).filter((t) => t.id === "stuck");
    expect(appearances).toHaveLength(1);
  });

  it("has no blocked column when nothing is blocked", () => {
    const groups = groupTasks([task("a", { status: "todo" })], "status", noMilestones, new Set());
    expect(groups.map((g) => g.key)).not.toContain("blocked");
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

  it("uses the digest's ranking for `next`, so the app and the model agree", () => {
    const rank: RankMap = new Map([["second", { position: 1, why: "" }]]);
    const tasks = [task("first", { priority: "p0" }), task("second", { priority: "p3" })];
    expect(sortTasks(tasks, "next", "asc", rank).map((t) => t.id)).toEqual(["second", "first"]);
  });

  // `rank` and `next` are two different orders and were briefly the same word.
  // A sort that silently means one when you asked for the other is exactly the
  // kind of thing nobody reports, so they are asserted apart.
  it("uses the stored deliberate order for `rank`, which is not the digest's", () => {
    const digest: RankMap = new Map([["b", { position: 1, why: "" }]]);
    const tasks = [task("a", { rank: 1 }), task("b", { rank: 2 })];
    expect(sortTasks(tasks, "rank", "asc", digest).map((t) => t.id)).toEqual(["a", "b"]);
    expect(sortTasks(tasks, "next", "asc", digest).map((t) => t.id)).toEqual(["b", "a"]);
  });

  it("sorts by a fractional rank, so a midpoint lands between its neighbours", () => {
    const tasks = [task("a", { rank: 1 }), task("c", { rank: 2 }), task("b", { rank: 1.5 })];
    expect(sortTasks(tasks, "rank", "asc", new Map()).map((t) => t.id)).toEqual(["a", "b", "c"]);
  });

  // Failure case: sorting must not mutate its input, or the board reorders
  // whatever else is holding the same array.
  it("leaves the array it was given alone", () => {
    const tasks = [task("b", { priority: "p3" }), task("a", { priority: "p0" })];
    sortTasks(tasks, "priority", "asc", noRank);
    expect(tasks.map((t) => t.id)).toEqual(["b", "a"]);
  });
});

describe("groupTasks, by parent", () => {
  it("names each group after the parent and gathers the rest", () => {
    const names = new Map([["tsk_epic", "The epic"]]);
    const groups = groupTasks(
      [task("child", { parent_id: "tsk_epic" }), task("loose")],
      "parent",
      names,
    );
    expect(groups.map((g) => g.label)).toEqual(["The epic", "not part of anything"]);
  });

  // The parent need not be in the filtered set: narrowing to open work should
  // still say which epic each piece belongs to.
  it("groups under a parent that is not itself in the list", () => {
    const names = new Map([["tsk_epic", "The epic"]]);
    const groups = groupTasks([task("child", { parent_id: "tsk_epic" })], "parent", names);
    expect(groups[0]?.label).toBe("The epic");
    expect(groups[0]?.tasks).toHaveLength(1);
  });
});

/**
 * What a column does with a dropped card. Three of the six do not simply take
 * it, and each refusal is a rule that lives somewhere else — so this is the
 * one place that has to agree with all three.
 */
describe("dropOnStatus", () => {
  it("takes a card into the two statuses that owe nothing", () => {
    expect(dropOnStatus("todo")).toEqual({ kind: "move", status: "todo" });
    expect(dropOnStatus("review")).toEqual({ kind: "move", status: "review" });
  });

  /** A close owes a reason, a message and evidence. The drop opens the form. */
  it("routes the terminal columns to the close form rather than writing", () => {
    expect(dropOnStatus("done").kind).toBe("close");
    expect(dropOnStatus("wont_do").kind).toBe("close");
  });

  /** Starting work is a claim, and a claim records which session (B-87). */
  it("refuses in_progress, and says why", () => {
    const drop = dropOnStatus("in_progress");
    expect(drop.kind).toBe("refused");
    if (drop.kind === "refused") expect(drop.why).toMatch(/claim/);
  });

  /** Blocked is derived from the graph, so there is nothing to set (TQ-25). */
  it("refuses the derived blocked column, and says why", () => {
    const drop = dropOnStatus("blocked");
    expect(drop.kind).toBe("refused");
    if (drop.kind === "refused") expect(drop.why).toMatch(/not a status/);
  });

  it("refuses anything it does not recognise rather than guessing", () => {
    expect(dropOnStatus("").kind).toBe("refused");
    expect(dropOnStatus("triage").kind).toBe("refused");
  });

  /**
   * Every column the board can render has an answer here. A new status that
   * reached the board without reaching this function would be silently
   * undroppable with a message that says nothing.
   */
  it("has an answer for every column the board renders", () => {
    for (const column of [...COLUMNS, "blocked"]) {
      const drop = dropOnStatus(column);
      if (drop.kind === "refused") {
        expect(drop.why).not.toBe("Cards cannot be dropped here.");
      }
    }
  });
});
