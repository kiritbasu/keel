/**
 * The shell's keyboard and its addresses.
 *
 * The regression worth naming: `App` used to return early from its key handler
 * on *any* modified keypress, so Cmd-K could never reach the palette. A test
 * for it exists because the bug was invisible — nothing was broken on screen,
 * the feature simply had nowhere to arrive.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

vi.mock("./lib/api", () => {
  const empty = { items: [], total: 0, truncated: false };
  return {
    ApiError: class ApiError extends Error {},
    subscribe: () => () => {},
    api: {
      projects: async () => ({
        projects: [{ id: "prj_1", type: "project", name: "Keel", slug: "keel", audit: {} }],
      }),
      context: async () => ({
        project: {
          id: "prj_1",
          name: "Keel",
          slug: "keel",
          status: "active",
          open_tasks: 3,
          urgent_tasks: 1,
          blocked_tasks: 0,
          open_questions: 2,
          active_milestone: null,
        },
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
        estimated_tokens: 1200,
      }),
      entities: async () => empty,
      notes: async () => ({ notes: [], total: 0 }),
      activity: async () => ({ events: [], total: 0, truncated: false, cursor: null }),
      document: async () => ({ revisions: [], document: null, diff: null }),
      graph: async () => ({ neighbours: [] }),
      search: async () => ({ hits: [], items: [], total: 0, truncated: false }),
      health: async () => ({ status: "ok", protocol: "", version: "", projects: 1 }),
    },
  };
});

const { App } = await import("./App");

beforeEach(() => {
  window.location.hash = "";
});

afterEach(cleanup);

/** jsdom dispatches hashchange on a task, so give it one before asserting. */
async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("the command palette key", () => {
  it("opens on Cmd-K — the modifier is no longer discarded", async () => {
    render(<App />);
    await settle();
    expect(screen.queryByRole("dialog")).toBeNull();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeTruthy();
  });

  it("opens on Ctrl-K too, for anyone not on a Mac", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(screen.getByRole("dialog", { name: "Command palette" })).toBeTruthy();
  });

  it("closes on Escape", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByRole("dialog")).toBeTruthy();

    fireEvent.keyDown(window, { key: "Escape" });
    await settle();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  // Failure case: the narrowed modifier check must still let the system and the
  // browser keep their own combinations.
  it("ignores other modified keys", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "j", metaKey: true });
    fireEvent.keyDown(window, { key: "3", metaKey: true });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(window.location.hash).toBe("#/");
  });
});

describe("navigation keys", () => {
  it("moves to a global screen and puts it in the address", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "7" });
    await waitFor(() => expect(window.location.hash).toBe("#/search"));
  });

  it("sends `/` to search", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "/" });
    await waitFor(() => expect(window.location.hash).toBe("#/search"));
  });

  it("carries the current project onto a project-scoped screen", async () => {
    window.location.hash = "#/projects/keel";
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "2" });
    await waitFor(() => expect(window.location.hash).toBe("#/projects/keel/board"));
  });

  // This used to do nothing, and that was the bug rather than the safeguard.
  // A project-scoped shortcut pressed from a global screen now falls back to
  // the project you were last in, because there is always one.
  it("falls back to the remembered project rather than doing nothing", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "2" });
    await waitFor(() => expect(window.location.hash).toBe("#/projects/keel/board"));
  });

  // Failure case: this is the bug that put a stray "6" in the search box.
  it("does not navigate on a keypress inside a text field", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const input = screen.getByLabelText("Jump to");

    fireEvent.keyDown(input, { key: "3" });
    await settle();
    expect(window.location.hash).toBe("#/");
  });
});

describe("addresses", () => {
  it("renders the screen the address names, not Home", async () => {
    window.location.hash = "#/projects/keel/board";
    render(<App />);
    await settle();
    await waitFor(() => expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("Tasks"));
  });

  // Failure case: an address that names a project-scoped screen without a
  // project is corrected, and corrected with `replace` so Back does not bounce.
  it("corrects a project-scoped address that names no project", async () => {
    window.location.hash = "#/board";
    render(<App />);
    await waitFor(() => expect(window.location.hash).toBe("#/"));
  });

  it("gives the sidebar real links, so copy-link and middle-click work", async () => {
    window.location.hash = "#/projects/keel";
    render(<App />);
    await settle();
    const board = screen.getByRole("link", { name: /Board/ });
    expect(board.getAttribute("href")).toBe("#/projects/keel/board");
  });
});

describe("the rail, with the project first", () => {
  // The Phase 8 exit criterion, asserted rather than eyeballed. Five of the
  // eight items used to render at 35% opacity with "Pick a project first" on a
  // cold launch, and the control that would have fixed that sat below them.
  it("disables nothing on a cold launch", async () => {
    render(<App />);
    await settle();
    const rail = screen.getByRole("navigation");
    const links = [...rail.querySelectorAll("a")];

    expect(links.length).toBeGreaterThan(0);
    for (const link of links) {
      expect(link.getAttribute("aria-disabled")).toBeNull();
      expect(link.getAttribute("href")).toBeTruthy();
    }
  });

  // The address says nothing about a project, and the project screens still
  // point somewhere real. That is the whole of C1 in one assertion.
  it("points the project screens at a project even from a global screen", async () => {
    render(<App />);
    await settle();
    expect(window.location.hash).toBe("#/");
    expect(screen.getByRole("link", { name: /Board/ }).getAttribute("href")).toBe(
      "#/projects/keel/board",
    );
    expect(screen.getByRole("link", { name: /Library/ }).getAttribute("href")).toBe(
      "#/projects/keel/documents",
    );
  });

  it("names the project you are in, as one row rather than one per project", async () => {
    render(<App />);
    await settle();
    // A button, not a list: the old rail grew a row for every project, so the
    // shell got taller as the store did.
    expect(screen.getByRole("button", { name: /Keel/ })).toBeTruthy();
  });

  it("keeps Roadmap with the project, though the router does not demand one", async () => {
    render(<App />);
    await settle();
    expect(screen.getByRole("link", { name: /Roadmap/ }).getAttribute("href")).toBe(
      "#/projects/keel/roadmap",
    );
  });
});
