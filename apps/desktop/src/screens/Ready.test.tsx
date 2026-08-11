/**
 * The Ready screen.
 *
 * What matters here is that the screen does not have opinions. The order comes
 * from the daemon and is rendered as given; the filters go into the address so a
 * filtered view is a link. A screen that re-sorted what it was handed would make
 * "what should I do next" a question with two answers, which is exactly what
 * having one ranking behind three doors is meant to prevent.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

const READY = {
  ready: [
    {
      id: "tsk_2",
      reference: "KEEL-108",
      title: "keel ready: what can be worked on right now",
      priority: "p0",
      unblocks: 2,
      why: "unblocks 2 other tasks · p0",
    },
    {
      id: "tsk_1",
      reference: "KEEL-98",
      title: "Make the app legible",
      priority: "p1",
      unblocks: 0,
      why: "nothing is blocking it · p1",
    },
  ],
  total: 7,
  truncated: true,
};

const calls: Array<Record<string, unknown>> = [];

/** What the next `api.ready` returns. A test that wants an empty list sets it. */
const state = { response: READY as typeof READY };

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    ready: async (params: Record<string, unknown>) => {
      calls.push(params);
      return state.response;
    },
    entities: async () => ({
      items: [
        {
          id: "mst_1",
          type: "milestone",
          name: "Phase 8 — The working loop",
          status: "active",
          audit: {},
        },
      ],
      total: 1,
      truncated: false,
    }),
  },
}));

const { ReadyScreen } = await import("./Ready");

function at(query: Record<string, string>): Route {
  return { screen: "ready", project: "keel", query };
}

async function show(query: Record<string, string> = {}) {
  render(<ReadyScreen route={at(query)} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/keel/ready";
  calls.length = 0;
  state.response = READY;
});
afterEach(cleanup);

describe("the ranked list", () => {
  it("renders the order it was given, without re-sorting it", async () => {
    await show();
    const refs = screen
      .getAllByText(/^KEEL-\d+$/)
      .map((el) => el.textContent);
    expect(refs).toEqual(["KEEL-108", "KEEL-98"]);
  });

  it("carries the reason each one is ranked where it is", async () => {
    await show();
    expect(screen.getByText("unblocks 2 other tasks · p0")).toBeTruthy();
  });

  it("sends a row to its own task page", async () => {
    await show();
    expect(
      screen.getByText("Make the app legible").closest("a")?.getAttribute("href"),
    ).toBe("#/projects/keel/tasks/KEEL-98");
  });

  // Hard constraint 4. Two of seven with nothing saying so is how a reader
  // concludes there are only two.
  it("says when the list was cut, and how much there was", async () => {
    await show();
    expect(screen.getByText(/Showing 2 of 7/)).toBeTruthy();
  });
});

describe("the filters", () => {
  it("puts unclaimed in the address rather than in component state", async () => {
    await show();
    fireEvent.click(screen.getByText("Unclaimed only"));
    expect(window.location.hash).toContain("unclaimed=true");
  });

  it("asks the daemon for unclaimed work when the address says so", async () => {
    await show({ unclaimed: "true" });
    expect(calls[0]?.unclaimed).toBe("true");
  });

  it("offers the active milestone by its short name", async () => {
    await show();
    fireEvent.click(screen.getByText("Phase 8"));
    expect(window.location.hash).toContain("milestone=mst_1");
  });

  // Failure case: filters that hide everything must not read as "there is no
  // work". The two need different responses from the reader — clear a filter, or
  // go and unblock something — so the empty state has to know which it is.
  it("blames the filters when they are what emptied the list", async () => {
    state.response = { ready: [], total: 0, truncated: false };
    await show({ unclaimed: "true" });
    expect(screen.getByText(/filters may be narrower/)).toBeTruthy();
  });

  it("blames the work, not the filters, when nothing is filtered", async () => {
    state.response = { ready: [], total: 0, truncated: false };
    await show();
    expect(screen.getByText(/blocked or waiting on a decision/)).toBeTruthy();
  });
});
