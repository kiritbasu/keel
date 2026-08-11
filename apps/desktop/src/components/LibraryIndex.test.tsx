/**
 * The five layouts, one per kind of artifact.
 *
 * These assert *shape*, not styling: a decision register has to be a table you
 * can scan by number, and a design has to be a picture. Those were the two
 * things the old flat list destroyed, and they are what C2 exists to restore.
 */

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { LibraryIndex } from "./LibraryIndex";
import type { Entity } from "../lib/api";

afterEach(cleanup);

const audit = { updated_at: "2026-08-10T09:00:00Z" };

function entity(over: Partial<Entity> & { id: string }): Entity {
  return { audit, ...over } as unknown as Entity;
}

describe("the decision register", () => {
  const decisions = [
    entity({ id: "dec_1", type: "decision", title: "Use DuckDB", number: 3, status: "accepted" }),
    entity({ id: "dec_2", type: "decision", title: "Use Lance", number: 12, status: "superseded" }),
  ];

  it("is a table, because a numbered register is something you scan", () => {
    render(<LibraryIndex type="decision" items={decisions} project="keel" />);
    expect(screen.getByRole("table")).toBeTruthy();
    expect(screen.getByRole("columnheader", { name: "Ref" })).toBeTruthy();
  });

  it("counts down from the newest, and shows the readable number", () => {
    render(<LibraryIndex type="decision" items={decisions} project="keel" />);
    const rows = screen.getAllByRole("row").slice(1); // drop the header
    expect(within(rows[0]!).getByText("B-12")).toBeTruthy();
    expect(within(rows[1]!).getByText("B-3")).toBeTruthy();
  });

  it("says when one was overturned, which is what supersedes means to a reader", () => {
    render(<LibraryIndex type="decision" items={decisions} project="keel" />);
    expect(screen.getByText("superseded")).toBeTruthy();
  });

  it("sends every row to the reader, so revision history is never lost", () => {
    render(<LibraryIndex type="decision" items={decisions} project="keel" />);
    expect(screen.getByRole("link", { name: "Use DuckDB" }).getAttribute("href")).toBe(
      "#/projects/keel/documents/dec_1",
    );
  });
});

describe("questions", () => {
  const questions = [
    entity({ id: "que_1", type: "question", title: "Retention policy?", status: "open" }),
    entity({ id: "que_2", type: "question", title: "Which embedder?", status: "answered" }),
  ];

  // Open first, and separated. "What is still undecided" is a different
  // question from "what did we decide", and one ordered list makes the reader
  // do the separating.
  it("puts the undecided ones in their own section, counted", () => {
    render(<LibraryIndex type="question" items={questions} project="keel" />);
    const headings = screen.getAllByRole("heading");
    expect(headings[0]!.textContent).toContain("Open");
    expect(headings[0]!.textContent).toContain("1");
    expect(headings[1]!.textContent).toContain("Settled");
  });

  it("drops a section rather than showing an empty heading", () => {
    render(<LibraryIndex type="question" items={[questions[0]!]} project="keel" />);
    expect(screen.queryByText(/Settled/)).toBeNull();
  });
});

describe("designs", () => {
  it("renders the picture, because that is the thing you came to look at", () => {
    render(
      <LibraryIndex
        type="design"
        items={[entity({ id: "dsg_1", type: "design", name: "Task page", blob_id: "blb_9" })]}
        project="keel"
      />,
    );
    const img = screen.getByRole("img", { name: "Task page" });
    expect(img.getAttribute("src")).toBe("/api/blob/blb_9");
  });

  // Failure case: a design whose image never landed. The tile still has to be
  // reachable, or the artifact becomes invisible rather than merely pictureless.
  it("says so when a design has no image, instead of a broken tile", () => {
    render(
      <LibraryIndex
        type="design"
        items={[entity({ id: "dsg_2", type: "design", name: "No picture", blob_id: null })]}
        project="keel"
      />,
    );
    expect(screen.getByText("no image")).toBeTruthy();
    expect(screen.getByRole("link", { name: /No picture/ })).toBeTruthy();
  });
});

describe("feedback", () => {
  it("leads with who said it and how they felt", () => {
    render(
      <LibraryIndex
        type="feedback"
        items={[
          entity({
            id: "fbk_1",
            type: "feedback",
            title: "Onboarding was slow",
            source: "interview",
            sentiment: "negative",
          }),
        ]}
        project="keel"
      />,
    );
    expect(screen.getByText("interview")).toBeTruthy();
    expect(screen.getByText("negative")).toBeTruthy();
  });
});

describe("an empty kind", () => {
  it("names the kind rather than saying 'no documents'", () => {
    render(<LibraryIndex type="feedback" items={[]} project="keel" />);
    expect(screen.getByText(/No feedback yet/)).toBeTruthy();
  });
});
