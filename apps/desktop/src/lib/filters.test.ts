import { describe, expect, it } from "vitest";
import {
  EMPTY_FILTER,
  activeCount,
  applyFilter,
  filterToQuery,
  isFiltering,
  parseFilter,
  toggle,
} from "./filters";
import type { Entity } from "./api";

function task(id: string, fields: Partial<Entity> = {}): Entity {
  return {
    id,
    type: "task",
    audit: {} as Entity["audit"],
    status: "todo",
    priority: "p2",
    kind: "task",
    labels: [],
    ...fields,
  } as Entity;
}

const nothingBlocked = new Set<string>();

describe("parseFilter", () => {
  it("reads every facet out of the query", () => {
    expect(
      parseFilter({
        status: "todo,blocked",
        priority: "p0",
        kind: "bug",
        label: "desktop,phase6",
        milestone: "mst_1",
        blocked: "true",
        q: "billing",
      }),
    ).toEqual({
      status: ["todo", "blocked"],
      priority: ["p0"],
      kind: ["bug"],
      labels: ["desktop", "phase6"],
      milestone: "mst_1",
      blocked: true,
      text: "billing",
    });
  });

  it("reads an empty query as no filter at all", () => {
    expect(parseFilter({})).toEqual(EMPTY_FILTER);
    expect(isFiltering(parseFilter({}))).toBe(false);
  });

  // Failure case: a hand-edited URL should degrade, not break. Empty segments
  // and stray whitespace are what happens when someone deletes one value from
  // a comma-separated list by hand.
  it("survives a hand-edited query", () => {
    expect(parseFilter({ status: "todo,,  ,blocked " }).status).toEqual([
      "todo",
      "blocked",
    ]);
    expect(parseFilter({ blocked: "yes" }).blocked).toBe(false);
    expect(parseFilter({ milestone: "" }).milestone).toBeUndefined();
  });
});

describe("filterToQuery", () => {
  it("round-trips with parseFilter", () => {
    const filter = parseFilter({
      status: "todo,blocked",
      priority: "p0",
      label: "desktop",
      milestone: "mst_1",
      blocked: "true",
      q: "billing",
    });
    const query = filterToQuery(filter);
    const cleaned = Object.fromEntries(
      Object.entries(query).filter(([, v]) => v !== undefined),
    ) as Record<string, string>;
    expect(parseFilter(cleaned)).toEqual(filter);
  });

  // Two views that are the same view must have the same address, or a
  // bookmarked filter and a freshly-built one look like different pages.
  it("drops every empty value rather than writing it as blank", () => {
    expect(filterToQuery(EMPTY_FILTER)).toEqual({
      status: undefined,
      priority: undefined,
      kind: undefined,
      label: undefined,
      milestone: undefined,
      blocked: undefined,
      q: undefined,
    });
  });
});

describe("activeCount", () => {
  it("counts each value, not each facet", () => {
    expect(
      activeCount(parseFilter({ status: "todo,blocked", label: "desktop" })),
    ).toBe(3);
  });

  it("ignores whitespace-only text", () => {
    expect(activeCount(parseFilter({ q: "   " }))).toBe(0);
  });
});

describe("toggle", () => {
  it("adds and removes", () => {
    expect(toggle([], "todo")).toEqual(["todo"]);
    expect(toggle(["todo"], "todo")).toEqual([]);
    expect(toggle(["todo"], "done")).toEqual(["todo", "done"]);
  });
});

describe("applyFilter", () => {
  // Numbered, because the reference is part of what a search matches and a row
  // without one is the degraded case rather than the normal one.
  const tasks = [
    task("a", {
      number: 1,
      status: "todo",
      priority: "p0",
      labels: ["desktop"],
      title: "Routing",
    }),
    task("b", {
      number: 2,
      status: "done",
      priority: "p1",
      labels: ["mcp"],
      title: "Billing rework",
    }),
    task("c", {
      number: 3,
      status: "todo",
      priority: "p2",
      labels: ["desktop", "mcp"],
      kind: "bug",
    }),
  ];

  it("returns everything when nothing is set", () => {
    expect(applyFilter(tasks, EMPTY_FILTER, nothingBlocked)).toHaveLength(3);
  });

  // The rule that makes a filter bar usable: OR inside a facet, AND across
  // them. Any other reading makes "status: todo, blocked" mean nothing.
  it("ORs within a facet and ANDs across them", () => {
    expect(
      applyFilter(
        tasks,
        parseFilter({ status: "todo,done" }),
        nothingBlocked,
      ).map((t) => t.id),
    ).toEqual(["a", "b", "c"]);

    expect(
      applyFilter(
        tasks,
        parseFilter({ status: "todo", priority: "p0" }),
        nothingBlocked,
      ).map((t) => t.id),
    ).toEqual(["a"]);
  });

  it("matches a task carrying any one of the wanted labels", () => {
    expect(
      applyFilter(tasks, parseFilter({ label: "mcp" }), nothingBlocked).map(
        (t) => t.id,
      ),
    ).toEqual(["b", "c"]);
  });

  it("searches the body as well as the title", () => {
    const withBody = [
      task("d", { title: "Opaque", body: "aggregation granularity" }),
    ];
    expect(
      applyFilter(withBody, parseFilter({ q: "granularity" }), nothingBlocked),
    ).toHaveLength(1);
  });

  // The bug KB hit: searching the board for the identifier every commit message
  // and every conversation uses found nothing, because the haystack was prose
  // only. The command palette had matched references since it was built, so
  // search worked in one place and not the other.
  it("finds a task by its reference, which is what people type", () => {
    const found = applyFilter(
      tasks,
      parseFilter({ q: "KEEL-2" }),
      nothingBlocked,
      "KEEL",
    );
    expect(found.map((t) => t.id)).toEqual(["b"]);
  });

  it("finds a task by its bare number, without the project key", () => {
    const found = applyFilter(
      tasks,
      parseFilter({ q: "3" }),
      nothingBlocked,
      "KEEL",
    );
    expect(found.map((t) => t.id)).toEqual(["c"]);
  });

  it("matches the reference whatever case it is typed in", () => {
    const found = applyFilter(
      tasks,
      parseFilter({ q: "keel-2" }),
      nothingBlocked,
      "KEEL",
    );
    expect(found.map((t) => t.id)).toEqual(["b"]);
  });

  // The failure case: a reference that belongs to no task must still return
  // nothing. A search that quietly widens when it cannot match is worse than
  // one that comes back empty.
  it("returns nothing for a reference that does not exist", () => {
    expect(
      applyFilter(
        tasks,
        parseFilter({ q: "KEEL-999" }),
        nothingBlocked,
        "KEEL",
      ),
    ).toHaveLength(0);
  });

  it("takes blockedness from the graph rather than from the status field", () => {
    // `b` is `done` and carries no `blocked` status, but something is linked to
    // it as a blocker. Status and links are different questions, and this filter
    // asks the link one.
    expect(
      applyFilter(tasks, parseFilter({ blocked: "true" }), new Set(["b"])).map(
        (t) => t.id,
      ),
    ).toEqual(["b"]);
  });

  it("matches a milestone, and `none` for the tasks nobody has scheduled", () => {
    const scheduled = [
      task("x", { milestone_id: "mst_1" }),
      task("y", { milestone_id: "mst_2" }),
      task("z"),
    ];
    expect(
      applyFilter(
        scheduled,
        parseFilter({ milestone: "mst_1" }),
        nothingBlocked,
      ).map((t) => t.id),
    ).toEqual(["x"]);
    expect(
      applyFilter(
        scheduled,
        parseFilter({ milestone: "none" }),
        nothingBlocked,
      ).map((t) => t.id),
    ).toEqual(["z"]);
  });

  // Failure case: a filter that matches nothing must return nothing, not
  // everything. Silently ignoring an unsatisfiable condition is how a web API
  // returned every type when asked for specs only.
  it("returns nothing when nothing matches", () => {
    expect(
      applyFilter(tasks, parseFilter({ status: "review" }), nothingBlocked),
    ).toEqual([]);
    expect(
      applyFilter(tasks, parseFilter({ label: "nonexistent" }), nothingBlocked),
    ).toEqual([]);
  });
});
