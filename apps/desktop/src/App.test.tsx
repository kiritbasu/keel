/**
 * The shell's keyboard and its addresses.
 *
 * The regression worth naming: `App` used to return early from its key handler
 * on *any* modified keypress, so Cmd-K could never reach the palette. A test
 * for it exists because the bug was invisible — nothing was broken on screen,
 * the feature simply had nowhere to arrive.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

/** Handles onto the live feed, so a test can drop and restore it. */
const feedHooks: {
  onChange?: (c: unknown) => void;
  onStatus?: (s: string) => void;
} = {};

vi.mock("./lib/api", () => {
  const empty = { items: [], total: 0, truncated: false };
  // Empty unless a test says otherwise, so no existing expectation moves.
  const tasksResponse = () =>
    (globalThis as { __tasks?: unknown[] }).__tasks?.length
      ? { items: (globalThis as { __tasks?: unknown[] }).__tasks, total: 1 }
      : {};
  return {
    ApiError: class ApiError extends Error {},
    subscribe: (
      onChange: (c: unknown) => void,
      onStatus?: (s: string) => void,
    ) => {
      feedHooks.onChange = onChange;
      feedHooks.onStatus = onStatus;
      return () => {};
    },
    api: {
      projects: async () => ({
        projects: [
          {
            id: "prj_1",
            type: "project",
            name: "Specline",
            slug: "specline",
            // As the real row carries them: a key, and the alias the rename
            // left behind so old links keep working (KEEL-312).
            key: "KEEL",
            aliases: ["keel"],
            audit: {},
          },
        ],
      }),
      context: async () => ({
        project: {
          id: "prj_1",
          name: "Specline",
          slug: "specline",
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
      entities: async ({ type }: { type?: string }) =>
        type === "task" ? { ...empty, ...tasksResponse() } : empty,
      ready: async () => ({ ready: [], total: 0, truncated: false }),
      notes: async () => ({ notes: [], total: 0 }),
      noteCounts: async () => ({ counts: {}, total: 0 }),
      activity: async () => ({
        events: [],
        total: 0,
        truncated: false,
        cursor: null,
      }),
      document: async () => ({ revisions: [], document: null, diff: null }),
      graph: async () => ({ neighbours: [] }),
      search: async () => ({ hits: [], items: [], total: 0, truncated: false }),
      health: async () => ({
        status: "ok",
        protocol: "",
        version: "",
        projects: 1,
      }),
    },
  };
});

const { App } = await import("./App");

beforeEach(() => {
  window.location.hash = "";
  (globalThis as { __tasks?: unknown[] }).__tasks = [];
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
    expect(
      screen.getByRole("dialog", { name: "Command palette" }),
    ).toBeTruthy();
  });

  it("opens on Ctrl-K too, for anyone not on a Mac", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    expect(
      screen.getByRole("dialog", { name: "Command palette" }),
    ).toBeTruthy();
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

describe("the rail carries no shortcut glyph", () => {
  /**
   * Three attempts at drawing one digit legibly, then removal.
   *
   * A bare right-aligned number read as a count — the convention in every mail
   * client and issue tracker — so an empty store showed "All projects 6, Search
   * 7", which reads as data appearing from nowhere, and every digit was wrong
   * as a count anyway. A leading `·` fixed that and read as unclear. A boxed
   * keycap was the third and did not land either (KEEL-223, KEEL-342).
   */
  it("renders no keycap beside a nav label", async () => {
    render(<App />);
    await settle();
    expect(document.querySelectorAll("nav a kbd")).toHaveLength(0);
  });

  /**
   * `⌘K` in the header is a different thing and stays. Scoping the assertion
   * above to the rail is what lets this one exist — a blanket "no kbd anywhere"
   * would have taken the palette hint with it.
   */
  it("keeps the palette hint in the header", async () => {
    render(<App />);
    await settle();
    const hints = [...document.querySelectorAll("kbd")].map(
      (k) => k.textContent,
    );
    expect(hints).toContain("⌘K");
  });

  /**
   * Not in a tooltip either. The hover hint was the halfway house when only
   * the glyph had gone; with the keypress gone too there is no key to name,
   * and a title advertising one would be a lie.
   */
  it("advertises no key in a tooltip", async () => {
    render(<App />);
    await settle();
    for (const a of document.querySelectorAll("nav a")) {
      expect(a.getAttribute("title") ?? "").not.toMatch(/press/i);
    }
  });
});

describe("the keys the rail no longer claims", () => {
  /**
   * Digit-to-screen is gone, keypress and glyph together (KEEL-342).
   *
   * The glyph went first, and keeping the keypress would have left the worse
   * half: an undiscoverable bare `3` swallowed from anywhere outside a text
   * field, for a destination one click away. Three attempts at drawing the
   * digit legibly is also three pieces of evidence that nobody was reaching
   * for it.
   */
  it("does nothing when a digit is pressed", async () => {
    window.location.hash = "#/projects/specline";
    render(<App />);
    await settle();
    for (const key of ["1", "3", "5", "9", "0"]) {
      fireEvent.keyDown(window, { key });
      await settle();
      expect(window.location.hash).toBe("#/projects/specline");
    }
  });

  /**
   * `/` and `⌘K` stay, and the difference is that both are conventions a
   * reader already has — neither needed a glyph in the rail to be found.
   */
  it("still sends `/` to search", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "/" });
    await waitFor(() => expect(window.location.hash).toBe("#/search"));
  });

  it("still opens the palette on ⌘K", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    expect(screen.getByLabelText("Jump to")).toBeTruthy();
  });

  // `/` is a navigation key and a character somebody types. The guard that
  // kept a stray "6" out of the search box has to keep a stray "/" out too.
  it("does not navigate on a keypress inside a text field", async () => {
    render(<App />);
    await settle();
    fireEvent.keyDown(window, { key: "k", metaKey: true });
    const input = screen.getByLabelText("Jump to");

    fireEvent.keyDown(input, { key: "/" });
    await settle();
    expect(window.location.hash).toBe("#/");
  });
});

describe("addresses", () => {
  it("renders the screen the address names, not Home", async () => {
    window.location.hash = "#/projects/specline/board";
    render(<App />);
    await settle();
    await waitFor(() =>
      expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(
        "Tasks",
      ),
    );
  });

  // Failure case: an address that names a project-scoped screen without a
  // project is corrected, and corrected with `replace` so Back does not bounce.
  it("corrects a project-scoped address that names no project", async () => {
    window.location.hash = "#/board";
    render(<App />);
    await waitFor(() => expect(window.location.hash).toBe("#/"));
  });

  it("gives the sidebar real links, so copy-link and middle-click work", async () => {
    window.location.hash = "#/projects/specline";
    render(<App />);
    await settle();
    const board = screen.getByRole("link", { name: /Board/ });
    expect(board.getAttribute("href")).toBe("#/projects/specline/board");
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
    expect(
      screen.getByRole("link", { name: /Board/ }).getAttribute("href"),
    ).toBe("#/projects/specline/board");
    expect(
      screen.getByRole("link", { name: /Library/ }).getAttribute("href"),
    ).toBe("#/projects/specline/documents");
  });

  it("names the project you are in, as one row rather than one per project", async () => {
    render(<App />);
    await settle();
    // A button, not a list: the old rail grew a row for every project, so the
    // shell got taller as the store did.
    expect(screen.getByRole("button", { name: /Specline/ })).toBeTruthy();
  });

  it("keeps Roadmap with the project, though the router does not demand one", async () => {
    render(<App />);
    await settle();
    expect(
      screen.getByRole("link", { name: /Roadmap/ }).getAttribute("href"),
    ).toBe("#/projects/specline/roadmap");
  });
});

describe("the live feed's state is visible", () => {
  // Asserted through the message rather than through `role="status"` alone.
  // The shell now carries two polite regions — this one and the toast host —
  // so the bare role no longer identifies which is being asked about.
  it("says nothing while the feed is healthy", async () => {
    render(<App />);
    await screen.findByText("Specline");
    act(() => feedHooks.onStatus?.("live"));
    expect(screen.queryByText(/out of date/)).toBeNull();
  });

  /// A stale page must not look identical to a current one. Before this the
  /// only difference between "nothing has changed" and "nothing can reach me"
  /// was that the second one was wrong.
  it("says so when the feed drops, and stops saying so when it returns", async () => {
    render(<App />);
    await screen.findByText("Specline");

    act(() => feedHooks.onStatus?.("down"));
    const notice = await screen.findByText(/out of date/);
    // Still a live region, which is the half of this that matters: a notice
    // nobody is told about is the stale page it exists to prevent.
    expect(notice.getAttribute("role")).toBe("status");

    act(() => feedHooks.onStatus?.("live"));
    await waitFor(() => expect(screen.queryByText(/out of date/)).toBeNull());
  });
});

/**
 * An address that names the project by an alias.
 *
 * The rename left `keel` on the Specline row so old links keep working, and the
 * daemon honours it — which is exactly what made this hard to see. Every fetch
 * succeeded, the screen filled with the right rows, and only the things read
 * off the *matched* project went missing: `KEEL-311` became a raw ULID and
 * "Phase" reverted to "Milestone" (KEEL-312).
 */
describe("a project named by an alias in the address", () => {
  it("still resolves to the project, so the switcher names it", async () => {
    window.location.hash = "#/projects/keel/board";
    render(<App />);
    await settle();

    // The switcher specifically, not any "Specline" on the page — the brand
    // in the rail says it too, so a looser assertion passes against the bug.
    const switcher = screen
      .getAllByRole("button")
      .find((b) => b.getAttribute("aria-haspopup") === "menu");
    expect(switcher?.textContent).toContain("Specline");
    expect(switcher?.textContent).not.toContain("keel");
  });

  /**
   * The one KB saw. A task offered by the palette carries the project's key,
   * and the key comes off the resolved row — so on an alias it used to fall
   * back to the id `taskRef` returns when it has no key.
   */
  it("still gives tasks their reference rather than a ULID", async () => {
    (globalThis as { __tasks?: unknown[] }).__tasks = [
      {
        id: "tsk_01M0BC78BJ7BJF6CWNNST5YH8C",
        type: "task",
        number: 311,
        title: "remove 8a 8b labels",
        status: "todo",
        priority: "p2",
        audit: {},
      },
    ];
    window.location.hash = "#/projects/keel/board";
    render(<App />);
    await settle();

    fireEvent.keyDown(window, { key: "k", metaKey: true });
    await settle();

    const palette = screen.getByRole("dialog", { name: "Command palette" });
    expect(palette.textContent).toContain("KEEL-311");
    expect(palette.textContent).not.toContain("tsk_01M0BC78");
  });
});
