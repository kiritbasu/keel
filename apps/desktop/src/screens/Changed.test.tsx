/**
 * What changed.
 *
 * Three properties, and each replaces a specific defect in the screen this
 * rebuilt: every row goes somewhere (none of them did), the range and the actor
 * live in the address (there was no range at all), and a session that wrote a
 * note shows the note (the event log could not have told you).
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

const NOW = "2026-08-11T18:00:00.000Z";
const YESTERDAY = "2026-08-10T09:00:00.000Z";

const RESPONSE = {
  sessions: [
    {
      session_id: "ses_recent",
      actor: "claude",
      started_at: NOW,
      ended_at: NOW,
      headline: "created 1 thing, 4 changes, wrote 2 notes",
      changes: [
        {
          id: "evt_1",
          kind: "created" as const,
          entity_id: "tsk_1",
          entity_type: "task",
          reference: "KEEL-108",
          summary: "created task “specline ready”",
          at: NOW,
        },
        {
          id: "nte_1",
          kind: "note" as const,
          entity_id: "tsk_1",
          entity_type: "task",
          reference: "KEEL-108",
          summary: "Turns out the limiter was fine and the test was not.",
          at: NOW,
        },
        {
          id: "evt_2",
          kind: "field" as const,
          entity_id: "dec_1",
          entity_type: "decision",
          reference: "",
          summary: "status proposed → accepted",
          at: NOW,
        },
      ],
    },
    {
      session_id: null,
      actor: "system",
      started_at: YESTERDAY,
      ended_at: YESTERDAY,
      headline: "created 40 things",
      changes: [
        {
          id: "evt_3",
          kind: "created" as const,
          entity_id: "tsk_2",
          entity_type: "task",
          reference: "KEEL-1",
          summary: "created task “Cargo workspace scaffold”",
          at: YESTERDAY,
        },
      ],
    },
  ],
  changes: 4,
  truncated: false,
};

const calls: Array<Record<string, unknown>> = [];
const state = { response: RESPONSE as typeof RESPONSE };

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    changed: async (params: Record<string, unknown>) => {
      calls.push(params);
      return state.response;
    },
  },
}));

const { ChangedScreen } = await import("./Changed");

function at(query: Record<string, string>): Route {
  return { screen: "changed", project: "specline", query };
}

async function show(query: Record<string, string> = {}) {
  render(<ChangedScreen route={at(query)} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/specline/changed";
  window.localStorage.clear();
  calls.length = 0;
  state.response = RESPONSE;
});
afterEach(cleanup);

describe("sessions", () => {
  it("shows one row per session with its own account of itself", async () => {
    await show();
    expect(screen.getByText("created 1 thing, 4 changes, wrote 2 notes")).toBeTruthy();
    expect(screen.getByText("created 40 things")).toBeTruthy();
  });

  it("keeps a session collapsed until it is asked to open", async () => {
    await show();
    expect(screen.queryByText(/created task “specline ready”/)).toBeNull();
    fireEvent.click(screen.getByText("created 1 thing, 4 changes, wrote 2 notes"));
    expect(screen.getByText(/created task “specline ready”/)).toBeTruthy();
  });

  it("says when a session recorded no conversation, without pretending it is an error", async () => {
    await show();
    fireEvent.click(screen.getByText("created 40 things"));
    expect(screen.getByText("no session recorded")).toBeTruthy();
  });
});

describe("where a row leads", () => {
  it("sends a task change to the task, by its readable identifier", async () => {
    await show();
    fireEvent.click(screen.getByText("created 1 thing, 4 changes, wrote 2 notes"));
    const link = screen.getByText(/created task “specline ready”/).closest("a");
    expect(link?.getAttribute("href")).toBe("#/projects/specline/tasks/KEEL-108");
  });

  it("sends a prose change to the document reader", async () => {
    await show();
    fireEvent.click(screen.getByText("created 1 thing, 4 changes, wrote 2 notes"));
    const link = screen.getByText(/status proposed/).closest("a");
    expect(link?.getAttribute("href")).toBe("#/projects/specline/documents/dec_1");
  });

  // The defect this screen was rebuilt for. `Activity.tsx` had no anchor of any
  // kind, so every row was dead text.
  it("makes every change a link", async () => {
    await show();
    fireEvent.click(screen.getByText("created 1 thing, 4 changes, wrote 2 notes"));
    const rows = screen.getAllByRole("listitem");
    const inner = rows.filter((r) => r.querySelector("a"));
    expect(inner.length).toBeGreaterThanOrEqual(3);
  });
});

describe("notes", () => {
  // The reason this needed a new endpoint rather than a regroup: a note leaves
  // no row in `events`, so the feed could not have shown one.
  it("shows a note a session wrote, marked as a note", async () => {
    await show();
    fireEvent.click(screen.getByText("created 1 thing, 4 changes, wrote 2 notes"));
    expect(screen.getByText(/Turns out the limiter was fine/)).toBeTruthy();
    expect(screen.getByText("note")).toBeTruthy();
  });
});

describe("the range and the actor", () => {
  it("defaults to this week and asks the daemon for it", async () => {
    await show();
    expect(calls[0]?.since).toBeTruthy();
  });

  it("asks for everything when the range says so, and leaves `since` off", async () => {
    await show({ range: "all" });
    expect(calls[0]?.since).toBeUndefined();
  });

  it("puts the range in the address", async () => {
    await show();
    fireEvent.click(screen.getByText("Today"));
    expect(window.location.hash).toContain("range=today");
  });

  // "This week" is the default, so it is written as the absence of a parameter.
  // Two addresses for one view would be two links to the same place.
  it("drops the range from the address when it is the default", async () => {
    await show({ range: "today" });
    fireEvent.click(screen.getByText("This week"));
    expect(window.location.hash).not.toContain("range=");
  });

  it("passes the actor filter through", async () => {
    await show({ actor: "human" });
    expect(calls[0]?.actor).toBe("human");
  });
});

describe("the new-since marker", () => {
  it("marks a session that landed after the last visit", async () => {
    window.localStorage.setItem("specline.changed.lastSeen", YESTERDAY);
    await show();
    expect(screen.getByText("new")).toBeTruthy();
    expect(screen.getByText(/1 new since you were last here/)).toBeTruthy();
  });

  it("marks nothing on a first visit, because everything would be new", async () => {
    await show();
    expect(screen.queryByText("new")).toBeNull();
  });

  it("remembers this visit, so the same rows are not new next time", async () => {
    await show();
    expect(window.localStorage.getItem("specline.changed.lastSeen")).toBeTruthy();
  });
});

describe("nothing to report", () => {
  it("blames the window rather than the project, and says how to widen it", async () => {
    state.response = { sessions: [], changes: 0, truncated: false };
    await show();
    expect(screen.getByText(/Nothing changed in this window/)).toBeTruthy();
    expect(screen.getByText(/Try Everything/)).toBeTruthy();
  });

  it("offers no way to widen when the range is already everything", async () => {
    state.response = { sessions: [], changes: 0, truncated: false };
    await show({ range: "all" });
    expect(screen.queryByText(/Try Everything/)).toBeNull();
  });
});
