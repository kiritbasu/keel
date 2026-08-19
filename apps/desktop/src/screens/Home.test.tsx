/**
 * The Home screen, and the branch between a new install and a real one.
 *
 * A store with no projects is not an empty list, it is somebody's first run,
 * and the two want different screens. The version this replaced said "Nothing
 * here yet", "Nothing unresolved" and "No activity yet" — three restatements of
 * absence framed by two bordered panels that were the largest shapes on the
 * page and held the least — and answered neither question a new user has: is
 * this working, and what do I do.
 *
 * So what is worth guarding is not the wording. It is that the two states are
 * exclusive, that the first-run screen carries evidence the install works, and
 * that it disappears completely the moment there is a project — an onboarding
 * panel that outlives its usefulness becomes furniture in the space the roll-up
 * needs.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
// Type-only, so the mock below still replaces the module at runtime. Without it
// the empty fixture infers `projects: never[]` and the populated one cannot be
// assigned to it.
import type { Digest } from "../lib/api";

const EMPTY_DIGEST: Digest = {
  project: null,
  projects: [],
  active: [],
  attention: [],
  recent: [],
  decisions: [],
  questions: [],
  specs: [],
  terms: [],
  environments: [],
  next: [],
  next_up: null,
  truncated: [],
  budget_exceeded: false,
  estimated_tokens: 0,
};

const WITH_PROJECT: Digest = {
  ...EMPTY_DIGEST,
  projects: [
    {
      id: "prj_1",
      name: "Tideline",
      slug: "tideline",
      key: "TIDE",
      status: "active",
      open_tasks: 3,
      urgent_tasks: 0,
      blocked_tasks: 0,
      open_questions: 1,
      inbox: 0,
      inbox_oldest_days: null,
      active_milestone: null,
    },
  ],
};

const HEALTH = {
  status: "ok",
  protocol: "2026-07-28",
  version: "0.1.1",
  projects: 0,
  store_busy: false,
  home: "/Users/someone/.specline",
  schema: 4,
};

const state = {
  digest: EMPTY_DIGEST as Digest,
  health: HEALTH as typeof HEALTH | null,
};

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    context: async () => state.digest,
    health: async () => {
      if (!state.health) throw new Error("health is unavailable");
      return state.health;
    },
  },
}));

const { HomeScreen } = await import("./Home");

async function show() {
  render(<HomeScreen route={{ screen: "home", query: {} }} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/";
  state.digest = EMPTY_DIGEST;
  state.health = HEALTH;
});
afterEach(cleanup);

describe("a store with nothing in it", () => {
  /**
   * The question a read-only surface makes hard to answer. There is no button
   * whose failure would tell you anything, so an empty screen is otherwise
   * indistinguishable from a broken one.
   */
  it("says the daemon is running, and shows what only a working one could", async () => {
    await show();
    expect(screen.getByText(/Specline is running/i)).toBeTruthy();
    expect(screen.getByText(/0\.1\.1/)).toBeTruthy();
    expect(screen.getByText(/schema 4/i)).toBeTruthy();
    expect(screen.getByText("/Users/someone/.specline")).toBeTruthy();
  });

  /** Empty is the expected state before first use, and should read that way. */
  it("frames the emptiness as expected rather than as absence", async () => {
    await show();
    expect(
      screen.getByText(/what it should be before you have used it/i),
    ).toBeTruthy();
  });

  /**
   * The actual next move. A description of the *kind* of thing to say is a dead
   * end on a surface that cannot be written to; a sentence you can paste is not.
   */
  it("gives literal sentences to say, not a description of them", async () => {
    await show();
    expect(
      screen.getByText(/we should add rate limiting to the API before launch/i),
    ).toBeTruthy();
    expect(screen.getByText(/we decided on Postgres/i)).toBeTruthy();
  });

  /**
   * The one failure a fresh install actually hits: `/specline:setup` succeeds, and
   * the session that ran it still has no tools, because MCP servers connect at
   * startup.
   */
  it("pre-empts the restart, which is where a new install stalls", async () => {
    await show();
    expect(screen.getByText(/Restart Claude Code/i)).toBeTruthy();
  });

  /** The three restatements of nothing that this replaced. */
  it("does not say 'nothing' three times in bordered panels", async () => {
    await show();
    expect(screen.queryByText(/Nothing here yet/i)).toBeNull();
    expect(screen.queryByText(/Nothing unresolved/i)).toBeNull();
    expect(screen.queryByText(/No activity yet/i)).toBeNull();
  });

  /**
   * Health is fetched separately and is allowed to fail. It decorates the
   * first run; it must not be able to break it, or a slow daemon turns a
   * welcome into a blank page.
   */
  it("still renders when health cannot be read", async () => {
    state.health = null;
    await show();
    expect(screen.getByText(/Specline is running/i)).toBeTruthy();
    expect(screen.queryByText(/schema/i)).toBeNull();
  });
});

describe("a store with a project in it", () => {
  beforeEach(() => {
    state.digest = WITH_PROJECT;
  });

  it("shows the roll-up", async () => {
    await show();
    expect(screen.getByText("Tideline")).toBeTruthy();
    expect(screen.getByText("1 project")).toBeTruthy();
  });

  /** The two states are exclusive, and onboarding does not linger. */
  it("drops the first-run screen entirely", async () => {
    await show();
    expect(screen.queryByText(/Specline is running/i)).toBeNull();
    expect(screen.queryByText(/Restart Claude Code/i)).toBeNull();
  });
});
