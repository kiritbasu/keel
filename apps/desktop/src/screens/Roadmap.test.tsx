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

  // This used to assert `queryByText(/no target/)` was null. That string is not
  // in any render path — only in comments — so the assertion could not fail
  // whatever the component did. What actually needs guarding is that a row with
  // nothing but counts still says something useful.
  it("says something about a phase with no target and no activity", async () => {
    await show([
      phase({ target_date: null, last_activity: null, shipped_at: null }),
    ]);
    expect(screen.getByText("29 / 35")).toBeTruthy();
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

  it("drops the target once the phase has shipped", async () => {
    await show([
      phase({
        target_date: "2026-08-09",
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-09T12:00:00Z",
      }),
    ]);
    expect(screen.queryByText(/^due /)).toBeNull();
    expect(screen.getByText(/^shipped /)).toBeTruthy();
  });

  // A date the browser cannot parse used to throw `RangeError` out of render,
  // which unmounts the screen rather than losing one cell.
  it("survives a target date it cannot parse", async () => {
    await show([phase({ target_date: "not a date" })]);
    expect(screen.getByText("29 / 35")).toBeTruthy();
    expect(screen.queryByText(/^due /)).toBeNull();
  });
});

describe("what a shipped row says", () => {
  // This test used to assert the opposite — that a shipped row shows no
  // fraction — because the branch keyed off `shipped_at` rather than `kind`.
  // Eight of the fifteen phases in the real store carry a shipped date, so the
  // column that exists to say how far a phase got was hidden on more than half
  // of them, and disagreed with `product/STATUS.md` about the same rows.
  it("shows a shipped phase's date and its fraction", async () => {
    await show([
      phase({
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-16T11:56:01Z",
      }),
    ]);
    expect(screen.getByText(/^shipped /)).toBeTruthy();
    expect(screen.getByText("29 / 35")).toBeTruthy();
  });

  it("prefers the shipped date over the last-moved date", async () => {
    await show([
      phase({
        state: "shipped",
        status: "shipped",
        shipped_at: "2026-08-16T11:56:01Z",
        last_activity: "2026-08-18T09:00:00Z",
      }),
    ]);
    expect(screen.getByText(/^shipped /)).toBeTruthy();
    expect(screen.queryByText(/^moved /)).toBeNull();
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

describe("grouping", () => {
  // Plan order is what somebody typed; it does not answer "where is this
  // project now". Fifteen phases in `sort_order` buried the three that were
  // moving in the middle of the twelve that were not.
  it("puts what is in flight above what is finished", async () => {
    await show([
      phase({
        id: "mst_1",
        name: "Phase 0 — Spine",
        state: "shipped",
        sort_order: 0,
      }),
      phase({
        id: "mst_2",
        name: "Phase 10 — Release",
        state: "active",
        sort_order: 10,
      }),
    ]);
    const headings = screen
      .getAllByRole("heading", { level: 2 })
      .map((h) => h.textContent);
    expect(headings).toEqual(["In flight", "Shipped"]);

    const body = document.body.textContent ?? "";
    expect(body.indexOf("Phase 10")).toBeLessThan(body.indexOf("Phase 0"));
  });

  it("keeps the manual order inside a group", async () => {
    await show([
      phase({
        id: "mst_2",
        name: "Phase 11 — Hardening",
        state: "active",
        sort_order: 11,
      }),
      phase({
        id: "mst_1",
        name: "Phase 10 — Release",
        state: "active",
        sort_order: 10,
      }),
    ]);
    const body = document.body.textContent ?? "";
    expect(body.indexOf("Phase 10")).toBeLessThan(body.indexOf("Phase 11"));
  });

  // `complete` is derived and `shipped` is declared, and only a person can say
  // which (B-57). Three of this project's phases sat in that state unnoticed.
  it("gives finished-not-declared its own heading and says what to do", async () => {
    await show([phase({ state: "complete", name: "Phase 12 — Search" })]);
    expect(screen.getByText("Finished, not yet declared")).toBeTruthy();
    expect(screen.getByText(/say which/)).toBeTruthy();
  });

  // The failure this guards is a phase vanishing from the one screen whose job
  // is to list them — which a new state in the enum would cause silently.
  it("shows a phase whose state matches no group rather than dropping it", async () => {
    await show([phase({ state: "something_new", name: "Phase 99 — Unknown" })]);
    expect(screen.getByText("Everything else")).toBeTruthy();
    expect(screen.getByText("Phase 99 — Unknown")).toBeTruthy();
  });

  it("renders every phase exactly once across the groups", async () => {
    const states = [
      "active",
      "blocked",
      "complete",
      "planned",
      "shipped",
      "paused",
      "cut",
    ];
    await show(
      states.map((s, i) =>
        phase({ id: `mst_${i}`, name: `Phase ${i} — ${s}`, state: s }),
      ),
    );
    for (let i = 0; i < states.length; i++) {
      expect(screen.getAllByText(`Phase ${i} — ${states[i]}`)).toHaveLength(1);
    }
  });

  it("counts the phases and how many are moving", async () => {
    await show([
      phase({ id: "mst_1", state: "active", name: "A" }),
      phase({ id: "mst_2", state: "shipped", name: "B" }),
      phase({ id: "mst_3", state: "shipped", name: "C" }),
    ]);
    expect(screen.getByText(/3 phases · 1 in flight/)).toBeTruthy();
  });
});

describe("releases are not here any more", () => {
  // They have a screen of their own. A release carries no tasks, so on this
  // screen it could only ever render as "not scoped" in a column about
  // progress — which is the second reason it did not belong.
  it("ignores a release row entirely", async () => {
    await show([
      phase({ id: "mst_1", name: "Phase 10 — Release", state: "active" }),
      phase({
        id: "mst_2",
        kind: "release",
        name: "0.3.0 — what to pick up next",
        version_string: "0.3.0",
        state: "shipped",
        shipped_at: "2026-08-18T09:26:23Z",
        tasks_total: 0,
        tasks_closed: 0,
      }),
    ]);
    expect(screen.queryByText(/0\.3\.0/)).toBeNull();
    expect(screen.queryByText("not scoped")).toBeNull();
    expect(screen.getByText("Phase 10 — Release")).toBeTruthy();
    expect(screen.getByText(/1 phase · 1 in flight/)).toBeTruthy();
  });
});

describe("the description", () => {
  // Briefly clamped to one line to keep the page short. KB asked for it back
  // in full: it is the sentence saying what the phase was for, and a roadmap
  // of bare names answers that only for whoever wrote them.
  it("shows a finished phase's summary in full, not truncated", async () => {
    const summary =
      "Fold DuckDB and Lance into one database, so a backup is one file and there " +
      "is one thing to keep consistent.";
    await show([
      phase({ state: "shipped", name: "Phase 9 — One database", summary }),
    ]);
    const el = screen.getByText(summary);
    expect(el.className).not.toMatch(/truncate|line-clamp/);
    expect(getComputedStyle(el).whiteSpace).not.toBe("nowrap");
  });
});
