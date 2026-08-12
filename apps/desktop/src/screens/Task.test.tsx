/**
 * The detail view.
 *
 * Four things it must get right, each of which was previously impossible to get
 * wrong only because nothing rendered them at all: relationships stated in the
 * direction they were walked, retracted notes shown rather than hidden, the
 * event log rendered as before-and-after, and J/K walking the board's order.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

const TASK = {
  id: "tsk_me",
  type: "task",
  title: "The task detail view",
  // Both nullable, and both exercised below: a row normally carries one of them
  // rather than both, and which one depends on whether it predates 8G.
  body: "Clicking a card opens the task at its own URL." as string | null,
  summary: null as string | null,
  status: "in_progress",
  priority: "p0",
  kind: "task",
  labels: ["desktop", "phase6"],
  milestone_id: "mst_1",
  parent_id: "tsk_parent",
  closed_at: null,
  external_refs: [
    "https://github.com/kb/keel/pull/1",
    "https://github.com/kb/keel/issues/2",
  ],
  audit: { created_at: "2026-08-10T09:00:00Z", updated_at: "2026-08-10T10:00:00Z" },
};

vi.mock("../lib/api", () => ({
  ApiError: class ApiError extends Error {},
  subscribe: () => () => {},
  api: {
    entity: async () => ({ artifacts: [{ entity: TASK }] }),
    notesFor: async () => ({
      notes: [
        {
          id: "nte_1",
          entity_id: "tsk_me",
          entity_type: "task",
          project_id: "prj_1",
          body: "Still believed at the time.",
          author: "claude",
          session_id: "ses_abc",
          surface: "code",
          created_at: "2026-08-10T09:30:00Z",
          archived_at: "2026-08-10T09:45:00Z",
        },
        {
          id: "nte_2",
          entity_id: "tsk_me",
          entity_type: "task",
          project_id: "prj_1",
          body: "What actually happened.",
          author: "human",
          session_id: null,
          surface: null,
          created_at: "2026-08-10T09:50:00Z",
          archived_at: null,
        },
      ],
      total: 2,
    }),
    history: async () => ({
      events: [
        {
          id: "evt_1",
          entity_id: "tsk_me",
          entity_type: "task",
          project_id: "prj_1",
          action: "created",
          field: null,
          before: null,
          after: null,
          actor: "claude",
          session_id: null,
          surface: null,
          summary: "created task “The task detail view”",
          created_at: "2026-08-10T09:00:00Z",
        },
        {
          id: "evt_2",
          entity_id: "tsk_me",
          entity_type: "task",
          project_id: "prj_1",
          action: "status_changed",
          field: "status",
          before: "todo",
          after: "in_progress",
          actor: "claude",
          session_id: null,
          surface: null,
          summary: "status todo → in_progress",
          created_at: "2026-08-10T10:00:00Z",
        },
      ],
      total: 2,
      truncated: false,
    }),
    graph: async (_id: string, direction: string) => ({
      neighbours:
        direction === "outbound"
          ? [
              {
                id: "tsk_child",
                entity_type: "task",
                rel: "blocks",
                label: "Sub-tasks — a parent link",
                anchor: "",
                depth: 1,
                path: [],
              },
            ]
          : [
              {
                id: "tsk_parent",
                entity_type: "task",
                rel: "blocks",
                label: "One page shell for every screen",
                anchor: "",
                depth: 1,
                path: [],
              },
            ],
    }),
    entities: async ({ type }: { type?: string }) => ({
      items:
        type === "milestone"
          ? [{ id: "mst_1", type: "milestone", name: "Phase 6 — Make the tracker real" }]
          : [
              { id: "tsk_first", type: "task", title: "Aaa first", status: "todo", priority: "p0" },
              { id: "tsk_me", type: "task", title: "Me", status: "in_progress", priority: "p0" },
              { id: "tsk_last", type: "task", title: "Zzz last", status: "done", priority: "p0" },
              {
                id: "tsk_parent",
                type: "task",
                title: "The epic above",
                status: "todo",
                priority: "p1",
              },
              {
                id: "tsk_kid_a",
                type: "task",
                title: "A finished piece",
                status: "done",
                priority: "p2",
                parent_id: "tsk_me",
              },
              {
                id: "tsk_kid_b",
                type: "task",
                title: "An unfinished piece",
                status: "todo",
                priority: "p2",
                parent_id: "tsk_me",
              },
            ],
      total: 3,
      truncated: false,
    }),
    context: async () => ({ next_up: null }),
  },
}));

const { TaskScreen } = await import("./Task");

const route = { screen: "task" as const, project: "keel", taskId: "tsk_me", query: {} };

async function show() {
  render(<TaskScreen route={route} generation={0} />);
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

beforeEach(() => {
  window.location.hash = "#/projects/keel/tasks/tsk_me";
});
afterEach(cleanup);

describe("what it shows", () => {
  it("renders the description, the properties and the milestone by name", async () => {
    await show();
    expect(screen.getByText("Clicking a card opens the task at its own URL.")).toBeTruthy();
    expect(screen.getByText("Phase 6 — Make the tracker real")).toBeTruthy();
    expect(screen.getAllByText("in_progress").length).toBeGreaterThan(0);
  });

  // The direction property, at the level a reader sees it. The same stored
  // `blocks` edge must appear under two different headings depending on which
  // way it was walked.
  it("states each relationship in the direction it was walked", async () => {
    await show();
    expect(screen.getByText("Blocked by")).toBeTruthy();
    expect(screen.getByText("Blocks")).toBeTruthy();
    expect(screen.getByText("One page shell for every screen")).toBeTruthy();
    expect(screen.getByText("Sub-tasks — a parent link")).toBeTruthy();
  });

  it("links a related task to its own page", async () => {
    await show();
    const link = screen.getByText("Sub-tasks — a parent link").closest("a");
    expect(link?.getAttribute("href")).toBe("#/projects/keel/tasks/tsk_child");
  });

  // A retracted note stays visible and struck through. Hiding it would rewrite
  // the record: what a session once believed is part of how the row got here.
  it("shows a retracted note rather than dropping it", async () => {
    await show();
    const note = screen.getByText("Still believed at the time.");
    expect(note).toBeTruthy();
    expect(screen.getByText("retracted")).toBeTruthy();
    expect(note.closest(".line-through")).toBeTruthy();
  });

  it("says when a note came from outside a tracked session", async () => {
    await show();
    expect(screen.getByText("written outside a tracked session")).toBeTruthy();
  });

  // The event log has always held before and after; nothing had ever shown it.
  it("renders a field change as before and after", async () => {
    await show();
    // Scoped to the History card — "todo" also names several sub-task statuses.
    const history = screen.getByText("History").closest("section") as HTMLElement;
    const row = within(history).getByText("todo").closest("li");
    expect(row?.textContent).toContain("status");
    expect(row?.textContent).toContain("todo");
    expect(row?.textContent).toContain("→");
    expect(row?.textContent).toContain("in_progress");
  });

  it("falls back to the summary for an event with no field", async () => {
    await show();
    expect(screen.getByText(/created task/)).toBeTruthy();
  });
});

// KEEL-170. This card read `body` alone, so the thirty-one tasks written since
// `summary` became required — several hundred words each, in the field every
// list already shows — displayed "No description." The store was never the
// problem, which is what made it hard to see: the row was complete and the page
// said it was empty.
describe("the description, whichever field carries it", () => {
  const body = TASK.body;
  const summary = TASK.summary;
  afterEach(() => {
    TASK.body = body;
    TASK.summary = summary;
  });

  it("shows the summary when there is no body", async () => {
    TASK.body = null;
    TASK.summary = "The board never says which phase a task belongs to.";
    await show();
    expect(screen.getByText("The board never says which phase a task belongs to.")).toBeTruthy();
    expect(screen.queryByText("No description.")).toBeNull();
  });

  // Said out loud, because a required one-or-two-sentence summary is not the
  // long-form detail a reader opening the page is looking for. Showing it
  // unlabelled would answer the question with something else and look right.
  it("says when what it is showing is the summary", async () => {
    TASK.body = null;
    TASK.summary = "Short, and standing in for a body that was never written.";
    await show();
    expect(screen.getByText("from the summary")).toBeTruthy();
  });

  it("prefers the body when both are there, and does not label it", async () => {
    TASK.summary = "One sentence that must not win.";
    await show();
    expect(screen.getByText("Clicking a card opens the task at its own URL.")).toBeTruthy();
    expect(screen.queryByText("One sentence that must not win.")).toBeNull();
    expect(screen.queryByText("from the summary")).toBeNull();
  });

  it("says there is no description only when neither field has one", async () => {
    TASK.body = null;
    TASK.summary = null;
    await show();
    expect(screen.getByText("No description.")).toBeTruthy();
    expect(screen.queryByText("from the summary")).toBeNull();
  });
});

describe("the keyboard", () => {
  it("J and K walk the board's order", async () => {
    await show();
    // Board order is todo, then in_progress, then done — so the neighbours of
    // the in_progress task are the todo one and the done one.
    fireEvent.keyDown(window, { key: "j" });
    expect(window.location.hash).toBe("#/projects/keel/tasks/tsk_last");

    window.location.hash = "#/projects/keel/tasks/tsk_me";
    fireEvent.keyDown(window, { key: "k" });
    // The last card in the todo column, which is the one immediately before
    // the in_progress column this task sits in.
    expect(window.location.hash).toBe("#/projects/keel/tasks/tsk_kid_b");
  });

  // Failure case: at the ends of the list the keys must do nothing rather than
  // wrap around, which would make J look like it had jumped at random.
  it("stops at the ends rather than wrapping", async () => {
    render(
      <TaskScreen route={{ ...route, taskId: "tsk_first" }} generation={0} />,
    );
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    window.location.hash = "#/projects/keel/tasks/tsk_first";
    fireEvent.keyDown(window, { key: "k" });
    expect(window.location.hash).toBe("#/projects/keel/tasks/tsk_first");
  });

  // Failure case: J typed into a field is a letter, not a command.
  it("ignores J and K typed into a text field", async () => {
    await show();
    const field = document.createElement("input");
    document.body.append(field);
    fireEvent.keyDown(field, { key: "j" });
    expect(window.location.hash).toBe("#/projects/keel/tasks/tsk_me");
    field.remove();
  });
});

describe("when the rest of the project cannot be loaded", () => {
  // The failure that prompted this: the daemon was briefly down, the task
  // itself rendered from cache, and the page quietly lost its readable
  // identifier, its milestone name and J/K — while looking complete.
  it("says what is missing rather than degrading silently", async () => {
    const api = (await import("../lib/api")).api as unknown as {
      entities: () => Promise<unknown>;
    };
    const working = api.entities;
    api.entities = () => Promise.reject(new Error("Cannot reach the Keel daemon."));
    try {
      await show();
      expect(screen.getByText(/could not be loaded/)).toBeTruthy();
    } finally {
      api.entities = working;
    }
  });
});

describe("what this is part of", () => {
  // Composition, not blocking. The two were the same edge before a task had a
  // parent, which is why a rollup was impossible: `blocks` means "must happen
  // first", and the ranking reads every inbound one as something in the way.
  it("shows the parent and the sub-tasks with a progress count", async () => {
    await show();
    expect(screen.getByText("Part of")).toBeTruthy();
    expect(screen.getByText("The epic above")).toBeTruthy();
    expect(screen.getByText("1 of 2 done")).toBeTruthy();
  });

  it("links a sub-task to its own page", async () => {
    await show();
    expect(screen.getByText("A finished piece").closest("a")?.getAttribute("href")).toBe(
      "#/projects/keel/tasks/tsk_kid_a",
    );
  });

  it("shows every external link, not just the first", async () => {
    await show();
    expect(screen.getByText("github.com/kb/keel/pull/1")).toBeTruthy();
    expect(screen.getByText("github.com/kb/keel/issues/2")).toBeTruthy();
  });
});

describe("the ask-Claude prompts", () => {
  // The app cannot write, and a read-only surface reads as either deliberate
  // or inert. The difference is whether it hands you the next move.
  it("offers prompts already addressed to this task", async () => {
    await show();
    // Matching on the button's accessible name: the prompt is a bare text node
    // beside the "copy" affordance, so it is not an element of its own.
    expect(screen.getByRole("button", { name: /close .+ as done with the commit/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /what is blocking/ })).toBeTruthy();
    expect(screen.getByRole("button", { name: /split .+ into sub-tasks/ })).toBeTruthy();
  });

  it("copies one to the clipboard", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    await show();
    const button = screen.getByRole("button", { name: /what is blocking/ });
    fireEvent.click(button);
    // Whatever this task's readable identifier is, the prompt carries it —
    // a prompt addressed to no task is worse than no prompt.
    expect(writeText).toHaveBeenCalledWith(
      expect.stringMatching(/^what is blocking \S+$/),
    );
  });
});
