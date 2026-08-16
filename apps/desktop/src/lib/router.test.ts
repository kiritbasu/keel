import { afterEach, describe, expect, it } from "vitest";
import { NEEDS_PROJECT, href, navigate, parseHash, setQuery, toHash, type Route } from "./router";

afterEach(() => {
  window.location.hash = "";
});

describe("parseHash", () => {
  it("reads the empty hash as Home", () => {
    expect(parseHash("")).toEqual({ screen: "home", query: {} });
    expect(parseHash("#")).toEqual({ screen: "home", query: {} });
    expect(parseHash("#/")).toEqual({ screen: "home", query: {} });
  });

  it("reads a project", () => {
    expect(parseHash("#/projects/specline")).toEqual({ screen: "project", project: "specline", query: {} });
  });

  it("reads a project-scoped screen", () => {
    expect(parseHash("#/projects/specline/board")).toEqual({
      screen: "board",
      project: "specline",
      query: {},
    });
  });

  it("reads a task inside a project", () => {
    expect(parseHash("#/projects/specline/tasks/tsk_01ABC")).toEqual({
      screen: "task",
      project: "specline",
      taskId: "tsk_01ABC",
      query: {},
    });
  });

  it("reads a document inside a project", () => {
    expect(parseHash("#/projects/specline/documents/spc_01ABC")).toEqual({
      screen: "documents",
      project: "specline",
      documentId: "spc_01ABC",
      query: {},
    });
  });

  it("distinguishes a global screen from its project-scoped form", () => {
    expect(parseHash("#/search").project).toBeUndefined();
    expect(parseHash("#/projects/specline/search").project).toBe("specline");
  });

  it("reads the query", () => {
    expect(parseHash("#/search?q=why+is+billing+slow&types=spec,decision").query).toEqual({
      q: "why is billing slow",
      types: "spec,decision",
    });
  });

  it("decodes a slug that needed encoding", () => {
    expect(parseHash("#/projects/two%20words/board").project).toBe("two words");
  });

  // Failure cases. A stale bookmark or a typo must not produce a dead end, and
  // must not be mistaken for a real route with missing parts.
  it("falls back to Home for a path that matches nothing", () => {
    expect(parseHash("#/nonsense/deeper/still").screen).toBe("home");
    expect(parseHash("#/projects").screen).toBe("home");
    expect(parseHash("#/projects/specline/board/extra").screen).toBe("home");
  });

  it("keeps the query when falling back, so a search survives a broken path", () => {
    expect(parseHash("#/nonsense?q=hello").query).toEqual({ q: "hello" });
  });
});

describe("toHash", () => {
  it("omits an empty query rather than writing a bare ?", () => {
    expect(toHash({ screen: "search", query: {} })).toBe("#/search");
    expect(toHash({ screen: "search", query: { q: "" } })).toBe("#/search");
  });

  it("writes the query when there is one", () => {
    expect(toHash({ screen: "search", query: { q: "billing" } })).toBe("#/search?q=billing");
  });

  // Failure case: a project-scoped screen with no project is not an address.
  // Sending it to Home is what makes the redirect in App.tsx a correction
  // rather than a loop.
  it("degrades a project-scoped screen with no project to Home", () => {
    expect(toHash({ screen: "board", query: {} })).toBe("#/");
    expect(toHash({ screen: "documents", query: {} })).toBe("#/");
    expect(toHash({ screen: "task", query: {} })).toBe("#/");
  });

  // Failure case: a task route with a project but no id is not an address
  // either. It degrades to that project's board rather than all the way to
  // Home, which keeps the reader where they were trying to be.
  it("degrades a task with no id to the project's board", () => {
    expect(toHash({ screen: "task", project: "specline", query: {} })).toBe("#/projects/specline/board");
  });

  it("round-trips every shape the app can build", () => {
    const routes: Route[] = [
      { screen: "home", query: {} },
      { screen: "roadmap", query: {} },
      { screen: "project", project: "specline", query: {} },
      { screen: "board", project: "specline", query: {} },
      { screen: "roadmap", project: "specline", query: {} },
      { screen: "changed", project: "specline", query: { actor: "claude" } },
      { screen: "documents", project: "specline", documentId: "spc_1", query: { v: "3", diff: "1" } },
      { screen: "task", project: "specline", taskId: "tsk_1", query: {} },
      { screen: "search", query: { q: "why is billing slow", types: "spec" } },
    ];
    for (const route of routes) {
      expect(parseHash(toHash(route))).toEqual(route);
    }
  });
});

describe("href", () => {
  it("fills in an empty query so callers need not", () => {
    expect(href({ screen: "board", project: "specline" })).toBe("#/projects/specline/board");
  });
});

describe("navigate", () => {
  it("moves the address", () => {
    navigate({ screen: "board", project: "specline" });
    expect(window.location.hash).toBe("#/projects/specline/board");
  });

  it("replaces without adding a history entry", () => {
    const before = window.history.length;
    navigate({ screen: "home" }, { replace: true });
    expect(window.location.hash).toBe("#/");
    expect(window.history.length).toBe(before);
  });
});

describe("setQuery", () => {
  const route: Route = { screen: "search", query: { q: "billing", types: "spec" } };

  it("amends one key and keeps the rest", () => {
    setQuery(route, { types: "decision" });
    expect(parseHash(window.location.hash).query).toEqual({ q: "billing", types: "decision" });
  });

  it("removes a key set to undefined or empty", () => {
    setQuery(route, { types: undefined });
    expect(parseHash(window.location.hash).query).toEqual({ q: "billing" });

    setQuery(route, { types: "" });
    expect(parseHash(window.location.hash).query).toEqual({ q: "billing" });
  });

  it("keeps the path", () => {
    setQuery({ screen: "board", project: "specline", query: {} }, { task: "tsk_1" });
    expect(parseHash(window.location.hash)).toEqual({
      screen: "board",
      project: "specline",
      query: { task: "tsk_1" },
    });
  });
});

describe("NEEDS_PROJECT", () => {
  it("names exactly the screens that cannot render without one", () => {
    expect(NEEDS_PROJECT).toEqual({
      home: false,
      project: true,
      roadmap: false,
      board: true,
      // Ready ranks one project's work. Across every project it would be a
      // list with no shared ordering to be best-first in.
      ready: true,
      task: true,
      documents: true,
      search: false,
      changed: false,
    });
  });
});
