/**
 * The Releases screen.
 *
 * Releases used to be a second section on the roadmap, and before that they
 * were interleaved with the phases in one list. Both were wrong for the same
 * reason: a phase is a unit of plan that holds tasks and has progress, a
 * release is a unit of record that went out on a date and holds nothing, and
 * putting them on one page implied a relationship neither has to the other
 * (KEEL-336).
 *
 * What these tests really guard is that the two screens do not drift back
 * together — a phase must never appear here, and the ordering rule must stay
 * the mirror of the tracker's rather than a second opinion about it.
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

const { ReleasesScreen } = await import("./Releases");

function release(over: Record<string, unknown>) {
  return {
    id: "mst_1",
    type: "milestone",
    kind: "release",
    name: "0.3.0 — what to pick up next",
    version_string: "0.3.0",
    status: "shipped",
    state: "shipped",
    shipped_at: "2026-08-18T09:26:23Z",
    summary: "specline_next got a front door.",
    audit: {},
    ...over,
  };
}

const route: Route = { screen: "releases", project: "specline", query: {} };

async function show(items: Array<Record<string, unknown>>) {
  state.items = items;
  render(<ReleasesScreen route={route} generation={0} milestoneNoun="Phase" />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/specline/releases";
});
afterEach(cleanup);

describe("what it lists", () => {
  it("shows the version, what went out, and when", async () => {
    await show([release({})]);
    expect(screen.getByText("0.3.0")).toBeTruthy();
    expect(screen.getByText("what to pick up next")).toBeTruthy();
    expect(screen.getByText("specline_next got a front door.")).toBeTruthy();
  });

  // A phase on this screen would be the old mistake, arriving from the other
  // direction. Both screens read the same endpoint, so nothing but the filter
  // keeps them apart.
  it("ignores a phase entirely", async () => {
    await show([
      release({ id: "mst_1" }),
      {
        id: "mst_2",
        type: "milestone",
        kind: "milestone",
        name: "Phase 10 — Release, distribution and install",
        status: "open",
        state: "active",
        tasks_total: 36,
        tasks_closed: 30,
        audit: {},
      },
    ]);
    expect(screen.queryByText(/Phase 10/)).toBeNull();
    expect(screen.getByText(/1 version · latest v0\.3\.0/)).toBeTruthy();
  });

  it("says so when nothing has shipped", async () => {
    await show([]);
    expect(screen.getByText("Nothing has shipped yet.")).toBeTruthy();
  });
});

describe("the order", () => {
  // Newest first, the opposite of the roadmap and deliberately: a plan is read
  // forwards from where you are, a changelog backwards from now.
  it("puts the newest version at the top", async () => {
    await show([
      release({
        id: "mst_1",
        name: "0.1.0 — first",
        version_string: "0.1.0",
        shipped_at: "2026-08-15T07:58:25Z",
      }),
      release({
        id: "mst_2",
        name: "0.3.0 — latest",
        version_string: "0.3.0",
        shipped_at: "2026-08-18T09:26:23Z",
      }),
    ]);
    const rows = screen.getByRole("table").textContent ?? "";
    expect(rows.indexOf("0.3.0")).toBeLessThan(rows.indexOf("0.1.0"));
  });

  // The date order and the name order must disagree, or the name tiebreak does
  // the work and the test passes with the whole date comparison deleted.
  it("orders by date, not by name", async () => {
    await show([
      release({
        id: "mst_1",
        name: "0.9.0 — older",
        version_string: "0.9.0",
        shipped_at: "2026-08-15T07:58:25Z",
      }),
      release({
        id: "mst_2",
        name: "0.10.0 — newer",
        version_string: "0.10.0",
        shipped_at: "2026-08-18T09:26:23Z",
      }),
    ]);
    const rows = screen.getByRole("table").textContent ?? "";
    expect(rows.indexOf("0.10.0")).toBeLessThan(rows.indexOf("0.9.0"));
  });

  // The one place on this screen where no date means the future rather than
  // the unknown. The tracker's table sorts oldest-first and puts it last,
  // which is the same rule read the other way up.
  it("puts a named-but-uncut version at the top", async () => {
    await show([
      release({
        id: "mst_1",
        name: "0.3.0 — shipped",
        version_string: "0.3.0",
      }),
      release({
        id: "mst_2",
        name: "0.4.0 — next",
        version_string: "0.4.0",
        state: "planned",
        status: "open",
        shipped_at: null,
      }),
    ]);
    const rows = screen.getByRole("table").textContent ?? "";
    expect(rows.indexOf("0.4.0")).toBeLessThan(rows.indexOf("0.3.0"));
    expect(screen.getByText("unreleased")).toBeTruthy();
  });

  // `latest` is the newest *shipped* version, so an uncut one at the top of
  // the list must not be announced as the current release.
  it("does not call an uncut version the latest", async () => {
    await show([
      release({
        id: "mst_1",
        name: "0.3.0 — shipped",
        version_string: "0.3.0",
      }),
      release({
        id: "mst_2",
        name: "0.4.0 — next",
        version_string: "0.4.0",
        shipped_at: null,
      }),
    ]);
    expect(screen.getByText(/latest v0\.3\.0/)).toBeTruthy();
  });
});

describe("the version column", () => {
  // Every release in the store is named "0.3.0 — what to pick up next", so
  // printing the name beside the version column would say the version twice.
  it("does not repeat the version in the prose", async () => {
    await show([release({})]);
    const cells = screen.getAllByRole("cell").map((c) => c.textContent);
    expect(cells.filter((c) => c?.includes("0.3.0"))).toHaveLength(1);
  });

  it("shows a name that does not start with its version whole", async () => {
    await show([
      release({
        name: "The one that fixed the installer",
        version_string: "0.1.3",
      }),
    ]);
    expect(screen.getByText("The one that fixed the installer")).toBeTruthy();
    expect(screen.getByText("0.1.3")).toBeTruthy();
  });

  // Null rather than the name. Falling back to the name put the same string in
  // both cells of one row, which reads as a rendering bug rather than as a
  // release nobody gave a number.
  it("leaves the version blank rather than repeating the name", async () => {
    await show([release({ name: "An unversioned cut", version_string: null })]);
    const cells = screen.getAllByRole("cell").map((c) => c.textContent);
    expect(cells.filter((c) => c?.includes("An unversioned cut"))).toHaveLength(
      1,
    );
    expect(cells[0]).toBe("—");
  });
});
