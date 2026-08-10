import { describe, expect, it } from "vitest";
import { compareTasks, inBoardOrder, taskRef, type RankMap } from "./tasks";
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
