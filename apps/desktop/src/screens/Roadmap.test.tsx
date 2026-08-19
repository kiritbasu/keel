/**
 * The Roadmap screen's right-hand column.
 *
 * It used to render `target_date`, falling back to the words "no target" — and
 * a target date is reachable only through an undocumented field bag, so seven
 * of this project's fifteen phases said "no target" and the other four said the
 * day the store was seeded. The column promised a plan nobody had made and said
 * nothing about whether the phase was moving (KEEL-332).
 *
 * What it says now is derived: how many of the phase's tasks are closed, and
 * when one of them last moved. Both numbers come from the daemon, which is the
 * part these tests are really guarding — a screen that counted them itself
 * would need every task in the project, and would be a second answer to a
 * question the digest and the tracker already answer.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

/** What the next `api.entities` returns. Each test sets the rows it needs. */
const state = { items: [] as Array<Record<string, unknown>> };

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    entities: async () => ({
      items: state.items,
      total: state.items.length,
      truncated: false,
    }),
  },
}));

const { RoadmapScreen } = await import("./Roadmap");

function phase(over: Record<string, unknown>) {
  return {
    id: "mst_1",
    type: "milestone",
    kind: "milestone",
    name: "Phase 10 — Release",
    status: "open",
    state: "active",
    tasks_total: 35,
    tasks_closed: 29,
    tasks_started: 2,
    last_activity: "2026-08-18T09:00:00Z",
    shipped_at: null,
    target_date: null,
    audit: {},
    ...over,
  };
}

const route: Route = { screen: "roadmap", project: "specline", query: {} };

async function show(items: Array<Record<string, unknown>>) {
  state.items = items;
  render(<RoadmapScreen route={route} generation={0} milestoneNoun="Phase" />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/specline/roadmap";
});
afterEach(cleanup);

describe("what an unshipped phase says on the right", () => {
  it("says how many of its tasks are closed", async () => {
    await show([phase({})]);
    expect(screen.getByText("29 / 35")).toBeTruthy();
  });

  it("never says 'no target' again, whatever is missing", async () => {
    await show([
      phase({ id: "mst_1", target_date: null, last_activity: null }),
      phase({ id: "mst_2", name: "Phase 14 — Inbox", tasks_total: 0, tasks_closed: 0 }),
    ]);
    expect(screen.queryByText(/no target/)).toBeNull();
  });

  // The distinction the fraction alone loses: a phase nobody has broken down
  // and a phase nobody has started both read as an empty bar, and only one of
  // them is waiting on somebody to write tasks.
  it("says a phase is unscoped rather than showing 0 / 0", async () => {
    await show([phase({ tasks_total: 0, tasks_closed: 0, tasks_started: 0 })]);
    expect(screen.getByText("not scoped")).toBeTruthy();
    expect(screen.queryByText("0 / 0")).toBeNull();
  });

  it("shows when the phase last moved", async () => {
    await show([phase({})]);
    expect(screen.getByText(/^moved /)).toBeTruthy();
  });

  it("leaves the activity out rather than inventing one", async () => {
    await show([phase({ last_activity: null })]);
    expect(screen.queryByText(/^moved /)).toBeNull();
    expect(screen.getByText("29 / 35")).toBeTruthy();
  });

  // The field is kept for the day an external commitment exists. It is no
  // longer what the column falls back to, but a date somebody did set must
  // still show.
  it("still shows a target date when one was actually set", async () => {
    await show([phase({ target_date: "2026-09-30" })]);
    expect(screen.getByText(/^due /)).toBeTruthy();
    expect(screen.getByText("29 / 35")).toBeTruthy();
  });
});

describe("what a shipped row says", () => {
  it("says when it shipped, not how its tasks are doing", async () => {
    await show([
      phase({
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-16T11:56:01Z",
      }),
    ]);
    expect(screen.getByText(/^shipped /)).toBeTruthy();
    expect(screen.queryByText("29 / 35")).toBeNull();
  });

  // A release carries no tasks, so the progress branch would render it as
  // "not scoped" — which is true and useless. It is the shipped date that says
  // what a release row is for.
  it("does not call a release unscoped", async () => {
    await show([
      phase({
        kind: "release",
        name: "v0.3.0",
        version_string: "0.3.0",
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-18T09:26:23Z",
        tasks_total: 0,
        tasks_closed: 0,
        last_activity: null,
      }),
    ]);
    expect(screen.queryByText("not scoped")).toBeNull();
    expect(screen.getByText(/^shipped /)).toBeTruthy();
  });
});

describe("the two strands", () => {
  // A phase and a release answer different questions, and the store now holds
  // ten of each. One chronological list buries the plan inside a changelog.
  it("keeps releases out of the phase list, under their own heading", async () => {
    await show([
      phase({ id: "mst_1", name: "Phase 10 — Release" }),
      phase({
        id: "mst_2",
        kind: "release",
        name: "0.3.0 — what to pick up next",
        version_string: "0.3.0",
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-18T09:26:23Z",
        sort_order: 109,
      }),
    ]);
    expect(screen.getByText("Released")).toBeTruthy();
    const lists = screen.getAllByRole("list");
    expect(lists).toHaveLength(2);
    expect(lists[0]?.textContent).toContain("Phase 10");
    expect(lists[0]?.textContent).not.toContain("0.3.0");
    expect(lists[1]?.textContent).toContain("0.3.0");
  });

  it("says nothing about releases when there are none", async () => {
    await show([phase({})]);
    expect(screen.queryByText("Released")).toBeNull();
    expect(screen.getAllByRole("list")).toHaveLength(1);
  });

  // `sort_order` was assigned by a backfill. The next release will be created
  // by whoever cuts it, and the date is the fact that cannot be wrong.
  it("orders releases by when they shipped, not by the order they arrived", async () => {
    await show([
      phase({
        id: "mst_2",
        kind: "release",
        name: "0.3.0",
        state: "shipped",
        shipped_at: "2026-08-18T09:26:23Z",
        sort_order: null,
      }),
      phase({
        id: "mst_1",
        kind: "release",
        name: "0.1.0",
        state: "shipped",
        shipped_at: "2026-08-15T07:58:25Z",
        sort_order: null,
      }),
    ]);
    const names = screen.getAllByText(/^0\.\d\.0$/).map((el) => el.textContent);
    expect(names).toEqual(["0.1.0", "0.3.0"]);
  });
});

describe("an older daemon", () => {
  // The counts are new. A desktop app pointed at a daemon that predates them
  // gets nothing at all — and "nothing came back" must not be rendered as a
  // fact about the phase. `?? 0` would make every row on such a daemon claim
  // to be unscoped, which is false rather than merely unhelpful.
  it("says nothing rather than claiming the phase is unscoped", async () => {
    await show([
      {
        id: "mst_1",
        type: "milestone",
        kind: "milestone",
        name: "Phase 10 — Release",
        status: "open",
        state: "active",
        audit: {},
      },
    ]);
    expect(screen.queryByText(/NaN/)).toBeNull();
    expect(screen.queryByText("not scoped")).toBeNull();
    expect(screen.getByText("Phase 10 — Release")).toBeTruthy();
  });

  // The distinction that makes the line above meaningful: a daemon that did
  // answer, with zero, still says so.
  it("still says unscoped when the daemon actually reported zero", async () => {
    await show([phase({ tasks_total: 0, tasks_closed: 0 })]);
    expect(screen.getByText("not scoped")).toBeTruthy();
  });
});
