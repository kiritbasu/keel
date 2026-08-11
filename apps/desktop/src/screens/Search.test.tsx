/**
 * Search results that go somewhere.
 *
 * A hit used to be dead text: it told you what it had found and gave you no way
 * to reach it, which is most of a search engine missing.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

const HITS = [
  {
    entity_id: "tsk_1",
    entity_type: "task",
    project_id: "prj_1",
    title: "Routing and URLs",
    excerpt: "",
    score: 1,
    source: "both",
  },
  {
    entity_id: "dec_1",
    entity_type: "decision",
    project_id: "prj_1",
    title: "Readable identifiers are composed",
    excerpt: "",
    score: 0.9,
    source: "keyword",
  },
  {
    entity_id: "mst_1",
    entity_type: "milestone",
    project_id: "prj_1",
    title: "Phase 6",
    excerpt: "",
    score: 0.5,
    source: "keyword",
  },
  {
    entity_id: "trm_1",
    entity_type: "term",
    project_id: "prj_1",
    title: "Digest",
    excerpt: "",
    score: 0.4,
    source: "keyword",
  },
];

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    projects: async () => ({
      projects: [{ id: "prj_1", type: "project", name: "Keel", slug: "keel", audit: {} }],
    }),
    search: async () => ({ hits: HITS, items: HITS, total: HITS.length, truncated: false }),
    // The starter chips are built from the digest, so the screen reads it on
    // mount. Real shapes rather than empties, so the chips actually render and
    // the assertions below are about a screen someone would recognise.
    context: async () => ({
      questions: [
        { id: "que_1", entity_type: "question", label: "TQ-30 — Does the app stay read-only?", status: "open" },
      ],
      decisions: [
        { id: "dec_1", entity_type: "decision", label: "Choose DuckDB over SQLite", status: "accepted" },
      ],
      terms: [{ term: "Mirror", definition: "Generated read-only markdown.", global: false }],
    }),
  },
}));

const { SearchScreen } = await import("./Search");

function at(query: Record<string, string>): Route {
  return { screen: "search", project: "keel", query };
}

async function show(query: Record<string, string>) {
  render(<SearchScreen route={at(query)} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/keel/search";
});
afterEach(cleanup);

describe("where a hit leads", () => {
  it("sends a task to its own page", async () => {
    await show({ q: "routing" });
    expect(screen.getByText("Routing and URLs").closest("a")?.getAttribute("href")).toBe(
      "#/projects/keel/tasks/tsk_1",
    );
  });

  it("sends a prose artifact to the document reader", async () => {
    await show({ q: "routing" });
    expect(
      screen.getByText("Readable identifiers are composed").closest("a")?.getAttribute("href"),
    ).toBe("#/projects/keel/documents/dec_1");
  });

  it("sends a milestone to the roadmap, which is where milestones are rendered", async () => {
    await show({ q: "routing" });
    expect(screen.getByText("Phase 6").closest("a")?.getAttribute("href")).toBe(
      "#/projects/keel/roadmap",
    );
  });

  // The types with no page of their own. Landing on the right project is a
  // worse answer than landing on the row, and a better one than landing
  // nowhere — which is what every hit used to do.
  it("sends a type with no page of its own to its project", async () => {
    await show({ q: "routing" });
    expect(screen.getByText("Digest").closest("a")?.getAttribute("href")).toBe(
      "#/projects/keel",
    );
  });

  it("makes every hit a link, with none left as dead text", async () => {
    await show({ q: "routing" });
    for (const hit of HITS) {
      expect(screen.getByText(hit.title).closest("a")).toBeTruthy();
    }
  });
});

describe("starter queries", () => {
  // The screen used to suggest "why is billing slow", copied from a tool
  // description written for a generic project. On the one screen whose whole
  // job is to invite a question, that named nothing the reader had ever seen.
  it("offers questions built from this project's own content", async () => {
    await show({});
    // An open question, verbatim, without the identifier it is filed under.
    expect(screen.getByText("Does the app stay read-only?")).toBeTruthy();
    // A decision framed as a why — the case where semantic search earns its
    // keep, because the answer's title never uses the word "why".
    expect(screen.getByText("why did we decide that choose DuckDB over SQLite")).toBeTruthy();
    // A glossary term, which shows the store knows the project's vocabulary.
    expect(screen.getByText('what does "Mirror" mean')).toBeTruthy();
  });

  it("no longer mentions billing anywhere", async () => {
    await show({});
    expect(document.body.textContent).not.toContain("billing");
  });
});
