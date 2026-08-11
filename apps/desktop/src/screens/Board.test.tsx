/**
 * The tasks screen: two layouts, and a view that lives in the address.
 *
 * The regression worth naming is the last one — the ranked "Next" panel used to
 * vanish the moment any filter was applied, so the best thing in the app was
 * only ever on screen when you did not need it.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import type { Route } from "../lib/router";

const TASKS = [
  {
    id: "tsk_1",
    type: "task",
    number: 1,
    title: "Routing and URLs",
    status: "done",
    priority: "p0",
    kind: "task",
    labels: ["desktop"],
    milestone_id: "mst_1",
    audit: { updated_at: "2026-08-01T00:00:00Z" },
  },
  {
    id: "tsk_2",
    type: "task",
    number: 2,
    title: "Filters that compose",
    status: "todo",
    priority: "p1",
    kind: "task",
    labels: ["desktop", "phase6"],
    milestone_id: "mst_1",
    audit: { updated_at: "2026-08-09T00:00:00Z" },
  },
  {
    id: "tsk_3",
    type: "task",
    number: 3,
    title: "Delete the file-edit hook",
    status: "todo",
    priority: "p0",
    kind: "bug",
    labels: ["plugin"],
    audit: { updated_at: "2026-08-05T00:00:00Z" },
  },
];

/** Counted so the board's appetite is asserted rather than assumed. */
const called = { ready: 0, context: 0, notes: 0, noteCounts: 0 };

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    entities: async ({ type }: { type?: string }) => ({
      items:
        type === "milestone"
          ? [{ id: "mst_1", type: "milestone", name: "Phase 6" }]
          : TASKS,
      total: type === "milestone" ? 1 : TASKS.length,
      truncated: false,
    }),
    context: async () => {
      called.context += 1;
      return { project: null, next_up: null };
    },
    ready: async () => {
      called.ready += 1;
      return {
        ready: [
          { id: "tsk_3", reference: "KEEL-3", title: "Delete the file-edit hook", why: "p0" },
        ],
        total: 1,
        truncated: false,
        blocked: ["tsk_2"],
      };
    },
    notes: async () => {
      called.notes += 1;
      return { notes: [], total: 0 };
    },
    noteCounts: async () => {
      called.noteCounts += 1;
      return { counts: { tsk_2: 3 }, total: 3 };
    },
  },
}));

const { BoardScreen } = await import("./Board");

function at(query: Record<string, string>): Route {
  return { screen: "board", project: "keel", query };
}

async function show(query: Record<string, string> = {}) {
  render(<BoardScreen route={at(query)} generation={0} projectKey="KEEL" />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/keel/board";
  called.ready = 0;
  called.context = 0;
  called.notes = 0;
  called.noteCounts = 0;
});
afterEach(cleanup);

describe("what the board asks the daemon for", () => {
  // KEEL-123. The board used to wait on the whole digest — 27 KB of project
  // briefing — for the ranking and the blocked set, and pull every note body in
  // the project to put a number on a card. Both are cheap calls now, and this is
  // what stops either creeping back: the shapes still work, so nothing else
  // would fail if they did.
  it("reads the ranking from /api/ready and never the digest", async () => {
    await show();
    expect(called.ready).toBe(1);
    expect(called.context).toBe(0);
  });

  it("asks for note counts, not note bodies", async () => {
    await show();
    expect(called.noteCounts).toBe(1);
    expect(called.notes).toBe(0);
  });

  // The blocked column is the reason `/api/ready` had to grow a `blocked`
  // parameter at all. If the ids stopped arriving the column would quietly
  // vanish and every card would look fine.
  it("still draws the blocked column from the ids /api/ready returns", async () => {
    await show();
    // The column heading, not the filter menu that shares the word.
    const headings = screen
      .getAllByText("blocked")
      .filter((el) => el.tagName === "SPAN" && el.className.includes("uppercase"));
    expect(headings).toHaveLength(1);
  });
});

describe("the filter, read from the address", () => {
  it("shows everything when the address carries no filter", async () => {
    await show();
    expect(screen.getByText("Routing and URLs")).toBeTruthy();
    expect(screen.getByText("Filters that compose")).toBeTruthy();
  });

  it("narrows to what the query names", async () => {
    await show({ status: "todo" });
    expect(screen.queryByText("Routing and URLs")).toBeNull();
    expect(screen.getByText("Filters that compose")).toBeTruthy();
  });

  it("combines facets with AND", async () => {
    await show({ status: "todo", priority: "p0" });
    expect(screen.queryByText("Filters that compose")).toBeNull();
    // Twice on screen — once in the Next panel, once as the one matching card.
    expect(screen.getAllByText("Delete the file-edit hook").length).toBeGreaterThan(0);
    expect(screen.getByText("1 of 3")).toBeTruthy();
  });

  it("counts distinct matches against the total", async () => {
    await show({ priority: "p0" });
    expect(screen.getByText("2 of 3")).toBeTruthy();
  });

  // Failure case: a filter matching nothing must say so rather than showing
  // everything, which is how a silently-dropped filter looks from the outside.
  it("says nothing matches rather than falling back to everything", async () => {
    await show({ status: "review" });
    expect(screen.getByText("No tasks match.")).toBeTruthy();
    expect(screen.getByText("Clear a filter above.")).toBeTruthy();
  });
});

describe("the ranked Next panel", () => {
  it("is there unfiltered", async () => {
    await show();
    expect(screen.getByText("Next")).toBeTruthy();
  });

  // The regression. Narrowing the board to look at something else does not
  // stop "what should I do next" being the question.
  it("is still there when a filter is applied", async () => {
    await show({ status: "todo", priority: "p1" });
    expect(screen.getByText("Next")).toBeTruthy();
    expect(screen.getByText("KEEL-3")).toBeTruthy();
  });
});

describe("the two layouts", () => {
  it("is a board by default", async () => {
    await show();
    expect(document.querySelector("table")).toBeNull();
  });

  it("is a table when the address says list", async () => {
    await show({ view: "list" });
    expect(document.querySelector("table")).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: /Ref/ })).toBeTruthy();
  });

  it("puts a sort into the address when a column header is clicked", async () => {
    await show({ view: "list" });
    fireEvent.click(sortHeader("Priority"));
    expect(window.location.hash).toContain("sort=priority");
  });

  it("reverses when the column already sorted by is clicked again", async () => {
    await show({ view: "list", sort: "priority" });
    fireEvent.click(sortHeader("Priority"));
    expect(window.location.hash).toContain("dir=desc");
  });
});

/** The column header's own button — "Priority" also names a filter menu. */
function sortHeader(name: string): HTMLElement {
  return within(screen.getByRole("columnheader", { name: new RegExp(name) })).getByRole("button");
}

describe("grouping", () => {
  it("groups by status by default, keeping the empty columns", async () => {
    await show();
    expect(screen.getByText("in progress")).toBeTruthy();
    expect(screen.getByText("review")).toBeTruthy();
  });

  it("groups by milestone, and names it", async () => {
    await show({ group: "milestone" });
    // The column heading specifically. Since C7 every card also carries a
    // milestone chip, so a bare text match now finds the heading *and* the
    // chips under it — which is the feature working, not a collision.
    const headings = screen
      .getAllByText("Phase 6")
      .filter((el) => el.tagName === "SPAN" && el.className.includes("uppercase"));
    expect(headings).toHaveLength(1);
    expect(screen.getByText("no milestone")).toBeTruthy();
  });

  // C7. The single most-asked question about this project is "what is left in
  // Phase 8", and it used to mean opening every card.
  it("shows what each task is part of, and marks the ones nobody placed", async () => {
    await show({});
    const chips = screen.getAllByRole("button", { name: /Phase 6|unplaced/ });
    expect(chips.length).toBeGreaterThan(0);
    // An unassigned task is visibly unassigned rather than silently blank —
    // that gap usually means something was filed and never placed.
    expect(screen.getAllByRole("button", { name: "unplaced" }).length).toBeGreaterThan(0);
  });

  // Failure case: a board with one column is not a board. `none` is a list
  // option, and offering it on the board and then ignoring it silently would be
  // worse than degrading visibly to the default.
  it("falls back to status when the board is asked for no grouping", async () => {
    await show({ group: "none" });
    expect(screen.getByText("todo")).toBeTruthy();
    expect(screen.queryByText("All")).toBeNull();
  });

  it("honours no grouping in the list", async () => {
    await show({ view: "list", group: "none" });
    expect(screen.queryByText("todo", { selector: "th" })).toBeNull();
  });
});

describe("a hand-edited address", () => {
  // Degrade, do not break. Someone will type these by hand — that is the point
  // of a readable URL — and a typo must not produce an error screen.
  it("falls back to the defaults for values it does not recognise", async () => {
    await show({ group: "byMood", sort: "vibes", dir: "sideways", view: "hologram" });
    expect(document.querySelector("table")).toBeNull();
    expect(screen.getByText("todo")).toBeTruthy();
    expect(screen.getByText("Routing and URLs")).toBeTruthy();
  });
});
