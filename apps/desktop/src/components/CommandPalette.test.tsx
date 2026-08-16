import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { CommandPalette, rank, score, screenItems, type PaletteItem } from "./CommandPalette";
import type { Route } from "../lib/router";

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    projects: async () => ({
      projects: [{ id: "prj_1", type: "project", name: "Specline", slug: "specline", audit: {} }],
    }),
    entities: async ({ type }: { type?: string }) => ({
      items:
        type === "task"
          ? [
              {
                id: "tsk_1",
                type: "task",
                title: "The task detail view",
                status: "todo",
                priority: "p0",
                audit: {},
              },
            ]
          : [{ id: "spc_1", type: "spec", title: "Specline — Spec", audit: {} }],
      total: 1,
      truncated: false,
    }),
  },
}));

function item(label: string, hint?: string): PaletteItem {
  return {
    id: label,
    label,
    kind: "task",
    ...(hint ? { hint } : {}),
    route: { screen: "board", project: "specline" },
  };
}

describe("score", () => {
  it("ranks a prefix above a word start above a substring above a subsequence", () => {
    expect(score("Board", "boa")).toBe(0);
    expect(score("The board view", "boa")).toBe(1);
    expect(score("Keyboard", "boa")).toBe(2);
    expect(score("Back off already", "boa")).toBe(3);
  });

  it("is case-insensitive", () => {
    expect(score("BOARD", "board")).toBe(0);
    expect(score("board", "BOARD")).toBe(0);
  });

  it("treats an empty query as matching everything equally", () => {
    expect(score("anything at all", "")).toBe(0);
  });

  // Failure case. Without this the palette would show every row for every
  // query, which is the same as showing nothing useful.
  it("returns null when the letters are absent or out of order", () => {
    expect(score("Board", "xyz")).toBeNull();
    expect(score("abc", "cba")).toBeNull();
    expect(score("Board", "boards")).toBeNull();
  });
});

describe("rank", () => {
  it("drops non-matches and orders by tier", () => {
    const items = [item("Back off already"), item("Keyboard"), item("Board"), item("Nothing here")];
    expect(rank(items, "boa").map((i) => i.label)).toEqual([
      "Board",
      "Keyboard",
      "Back off already",
    ]);
  });

  it("is stable within a tier, so source order shows through", () => {
    const items = [item("Board one"), item("Board two"), item("Board three")];
    expect(rank(items, "board").map((i) => i.label)).toEqual([
      "Board one",
      "Board two",
      "Board three",
    ]);
  });

  it("returns everything for an empty query", () => {
    const items = [item("a"), item("b")];
    expect(rank(items, "")).toHaveLength(2);
  });

  it("returns nothing when nothing matches", () => {
    expect(rank([item("Board")], "zzz")).toEqual([]);
  });

  // Typing a reference is how you find a task whose title you cannot recall,
  // which is most of the point of having one.
  it("finds a task by its reference as well as its title", () => {
    const items = [item("The task detail view", "KEEL-42")];
    expect(rank(items, "KEEL-42")).toHaveLength(1);
    expect(rank(items, "keel-42")).toHaveLength(1);
  });

  // …but a reference must never displace a real title match, or searching for
  // a word starts returning whatever identifier shares its letters.
  it("ranks every title match above every reference match", () => {
    const items = [item("Something else", "KEEL-42"), item("Keel-42 in the title")];
    expect(rank(items, "keel-42").map((i) => i.label)).toEqual([
      "Keel-42 in the title",
      "Something else",
    ]);
  });
});

describe("screenItems", () => {
  it("offers only the screens that work without a project", () => {
    const labels = screenItems(undefined).map((i) => i.route.screen);
    expect(labels).toEqual(["home", "roadmap", "search", "changed"]);
  });

  it("offers every screen once a project is named, and carries the project into the route", () => {
    const items = screenItems("specline");
    expect(items).toHaveLength(7);
    const board = items.find((i) => i.route.screen === "board");
    expect(board?.route.project).toBe("specline");
    expect(board?.hint).toBe("specline");
  });

  it("does not put a project on the screens that are global", () => {
    const home = screenItems("specline").find((i) => i.route.screen === "home");
    expect(home?.route.project).toBeUndefined();
  });
});

describe("the palette, driven by the keyboard", () => {
  const route: Route = { screen: "board", project: "specline", query: {} };

  beforeEach(() => {
    window.location.hash = "";
  });
  afterEach(cleanup);

  async function open() {
    render(<CommandPalette open onClose={() => {}} route={route} generation={0} />);
    // Let the three fetches land, so the list is populated rather than "Looking…".
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    return screen.getByLabelText("Jump to");
  }

  it("goes to what is selected when Enter is pressed", async () => {
    const input = await open();
    fireEvent.change(input, { target: { value: "detail" } });
    await waitFor(() => expect(screen.getByRole("option", { selected: true })).toBeTruthy());
    expect(screen.getByRole("option", { selected: true }).textContent).toContain("The task detail view");

    fireEvent.keyDown(input, { key: "Enter" });
    expect(window.location.hash).toBe("#/projects/specline/tasks/tsk_1");
  });

  it("moves the selection with the arrow keys", async () => {
    const input = await open();
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "ArrowUp" });
    fireEvent.keyDown(input, { key: "Enter" });
    // Second entry in the screen list, which is the project dashboard.
    expect(window.location.hash).toBe("#/projects/specline");
  });

  it("will not run off either end of the list", async () => {
    const input = await open();
    for (let i = 0; i < 5; i++) fireEvent.keyDown(input, { key: "ArrowUp" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(window.location.hash).toBe("#/");
  });

  // Failure case: Enter with nothing matched must do nothing rather than open
  // whatever happened to be first before the query was typed.
  it("does nothing on Enter when nothing matches", async () => {
    const input = await open();
    fireEvent.change(input, { target: { value: "zzzzzzz" } });
    await waitFor(() => expect(screen.queryByRole("option")).toBeNull());

    fireEvent.keyDown(input, { key: "Enter" });
    expect(window.location.hash).toBe("");
  });
});
