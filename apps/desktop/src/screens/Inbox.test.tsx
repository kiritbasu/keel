/**
 * The Inbox screen.
 *
 * Three things are worth holding down here, and they are the three the design
 * turns on rather than the three that are easiest to assert.
 *
 * **Oldest first.** Rendered in the order the daemon gives it, which is oldest
 * first — the opposite of every other list in the app. A screen that re-sorted
 * would bury the signal nobody has looked at in two months under whatever was
 * filed this morning, which is the entire failure the Inbox exists to prevent.
 *
 * **One field to file.** The box asks for the sentence and nothing else. Every
 * extra required field is a reason to close it and do it later, and a signal
 * filed later is a signal not filed.
 *
 * **No body.** Hard constraint 7: the interface captures what somebody said and
 * does not write a document revision. The screen has no field for one, and this
 * asserts that it never sends one even though the daemon would also refuse.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { Route } from "../lib/router";

const DAY = 24 * 60 * 60 * 1000;
const NOW = Date.parse("2026-08-19T12:00:00Z");

const INBOX = {
  items: [
    {
      id: "fbk_1",
      type: "feedback",
      summary: "this should work with codex",
      kind: "idea",
      source: "Madhu",
      contact: null,
      occurred_at: null,
      triaged: false,
      audit: { created_at: new Date(NOW - 40 * DAY).toISOString() },
    },
    {
      id: "fbk_2",
      type: "feedback",
      summary: "search should look inside documents",
      kind: "idea",
      source: null,
      contact: null,
      occurred_at: null,
      triaged: false,
      audit: { created_at: new Date(NOW - 1 * DAY).toISOString() },
    },
  ],
  total: 5,
  truncated: true,
};

const filed: Array<Record<string, unknown>> = [];
const state = { response: INBOX as typeof INBOX, fail: null as string | null };

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    inbox: async () => state.response,
    createSignal: async (signal: Record<string, unknown>) => {
      if (state.fail) throw new Error(state.fail);
      filed.push(signal);
      return signal;
    },
  },
}));

const { InboxScreen } = await import("./Inbox");

function at(): Route {
  return { screen: "inbox", project: "specline", query: {} };
}

async function show() {
  render(<InboxScreen route={at()} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  vi.useFakeTimers({ now: NOW, shouldAdvanceTime: true });
  window.location.hash = "#/projects/specline/inbox";
  filed.length = 0;
  state.response = INBOX;
  state.fail = null;
});
afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("the list", () => {
  it("renders the order it was given, oldest first, without re-sorting it", async () => {
    await show();
    const said = screen
      .getAllByText(/should/)
      .map((el) => el.textContent);
    expect(said).toEqual([
      "this should work with codex",
      "search should look inside documents",
    ]);
  });

  it("names who asked, when somebody did", async () => {
    await show();
    expect(screen.getByText("Madhu")).toBeTruthy();
  });

  // The age is what separates four signals nobody has looked at in two months
  // from forty filed this week. A badge on every row would say neither.
  it("marks only the signals that have actually been waiting", async () => {
    await show();
    const badges = screen.getAllByText(/waiting \d+ days/);
    expect(badges).toHaveLength(1);
    expect(badges[0]?.textContent).toBe("waiting 40 days");
  });

  // Hard constraint 4. An Inbox showing two of five with nothing saying so
  // reads as an Inbox of two, and somebody would empty it and believe they
  // had finished.
  it("says when the list was cut, and how much there was", async () => {
    await show();
    expect(screen.getByText(/Showing 2 of 5/)).toBeTruthy();
  });

  it("says what the Inbox is for when it is empty", async () => {
    state.response = { items: [], total: 0, truncated: false };
    await show();
    expect(screen.getByText("The Inbox is empty.")).toBeTruthy();
  });
});

describe("filing", () => {
  async function openBox() {
    await show();
    fireEvent.click(screen.getByText("File a signal"));
  }

  it("files a signal from the sentence alone", async () => {
    await openBox();
    fireEvent.change(screen.getByPlaceholderText("this should work with codex"), {
      target: { value: "the board should group by phase" },
    });
    fireEvent.click(screen.getByText("File it"));
    await settle();

    expect(filed).toEqual([
      { project: "specline", summary: "the board should group by phase" },
    ]);
  });

  it("records who asked when a name is given", async () => {
    await openBox();
    fireEvent.change(screen.getByPlaceholderText("this should work with codex"), {
      target: { value: "support codex" },
    });
    fireEvent.change(screen.getByPlaceholderText("Who asked (optional)"), {
      target: { value: "Madhu" },
    });
    fireEvent.click(screen.getByText("File it"));
    await settle();

    expect(filed[0]).toMatchObject({ source: "Madhu" });
  });

  // Hard constraint 7, at the surface that could break it. The interface
  // captures what was said; a document revision is written from the session
  // the conversation happened in.
  it("never sends a body", async () => {
    await openBox();
    fireEvent.change(screen.getByPlaceholderText("this should work with codex"), {
      target: { value: "onboarding felt slow" },
    });
    fireEvent.click(screen.getByText("File it"));
    await settle();

    expect(filed[0]).not.toHaveProperty("body");
  });

  // The box exists to be gone in six seconds. Reaching for the mouse to submit
  // one line is most of the cost it is trying not to have.
  it("files on Enter", async () => {
    await openBox();
    const box = screen.getByPlaceholderText("this should work with codex");
    fireEvent.change(box, { target: { value: "file me with the keyboard" } });
    fireEvent.keyDown(box, { key: "Enter" });
    await settle();

    expect(filed).toHaveLength(1);
  });

  it("keeps Shift+Enter for a line break rather than filing", async () => {
    await openBox();
    const box = screen.getByPlaceholderText("this should work with codex");
    fireEvent.change(box, { target: { value: "two lines" } });
    fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
    await settle();

    expect(filed).toHaveLength(0);
  });

  it("files nothing when there is nothing to say", async () => {
    await openBox();
    fireEvent.change(screen.getByPlaceholderText("this should work with codex"), {
      target: { value: "   " },
    });
    fireEvent.click(screen.getByText("File it"));
    await settle();

    expect(filed).toHaveLength(0);
  });

  // A failure has to stay on screen with the text still in the box. Clearing
  // the field on a failed write is how a captured thought gets lost by the
  // one mechanism that existed to keep it.
  it("keeps what was typed when the write fails, and says so", async () => {
    state.fail = "the daemon is not answering";
    await openBox();
    const box = screen.getByPlaceholderText("this should work with codex");
    fireEvent.change(box, { target: { value: "do not lose me" } });
    fireEvent.click(screen.getByText("File it"));
    await settle();

    expect(screen.getByText(/was not filed/)).toBeTruthy();
    expect((box as HTMLTextAreaElement).value).toBe("do not lose me");
  });
});
