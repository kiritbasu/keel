/**
 * The confirmation that a thing you just did happened, and what it is called.
 *
 * Creating a task used to say nothing: the dialog closed, the board reloaded,
 * and the number the row had just been given was discarded on the client
 * (KEEL-285). The property that matters most is the first test here — the
 * announcement survives the component that asked for it going away, because
 * the caller is a dialog that closes itself in the same breath.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { Toaster, toast } from "./ui";

afterEach(cleanup);

describe("Toaster", () => {
  it("shows what a caller announced", () => {
    render(<Toaster />);
    act(() => toast({ text: "Created KEEL-285" }));
    expect(screen.getByText("Created KEEL-285")).toBeTruthy();
  });

  /// The case it was written for. A component that announces something and
  /// unmounts in the same breath must still be heard, or the toast is only ever
  /// seen by callers that had no reason to need one.
  it("outlives the component that asked for it", () => {
    function Ephemeral() {
      const [alive, setAlive] = useState(true);
      if (!alive) return null;
      return (
        <button
          onClick={() => {
            toast({ text: "Created KEEL-285" });
            setAlive(false);
          }}
        >
          Create
        </button>
      );
    }

    render(
      <>
        <Ephemeral />
        <Toaster />
      </>,
    );
    fireEvent.click(screen.getByText("Create"));

    expect(screen.queryByText("Create")).toBeNull();
    expect(screen.getByText("Created KEEL-285")).toBeTruthy();
  });

  it("carries a link to the thing it is talking about", () => {
    render(<Toaster />);
    act(() =>
      toast({
        text: "Created KEEL-285",
        href: "#/projects/specline/tasks/KEEL-285",
        linkLabel: "Open",
      }),
    );
    expect(screen.getByText("Open").getAttribute("href")).toBe(
      "#/projects/specline/tasks/KEEL-285",
    );
  });

  it("can be dismissed before it expires", () => {
    render(<Toaster />);
    act(() => toast({ text: "Created KEEL-285" }));
    fireEvent.click(screen.getByLabelText("Dismiss"));
    expect(screen.queryByText("Created KEEL-285")).toBeNull();
  });

  it("goes away on its own", () => {
    vi.useFakeTimers();
    try {
      render(<Toaster />);
      act(() => toast({ text: "Created KEEL-285" }));
      act(() => void vi.advanceTimersByTime(9_000));
      expect(screen.queryByText("Created KEEL-285")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  /// Each toast keeps its own clock. Holding them in one effect keyed on the
  /// message list restarted every countdown whenever a message arrived, so a
  /// second toast silently extended the life of the first.
  it("does not restart an existing countdown when another arrives", () => {
    vi.useFakeTimers();
    try {
      render(<Toaster />);
      act(() => toast({ text: "Created KEEL-285" }));
      act(() => void vi.advanceTimersByTime(5_000));
      act(() => toast({ text: "Created KEEL-286" }));
      act(() => void vi.advanceTimersByTime(4_000));

      expect(screen.queryByText("Created KEEL-285")).toBeNull();
      expect(screen.getByText("Created KEEL-286")).toBeTruthy();
    } finally {
      vi.useRealTimers();
    }
  });

  it("stacks rather than replacing", () => {
    render(<Toaster />);
    act(() => {
      toast({ text: "Created KEEL-285" });
      toast({ text: "Created KEEL-286" });
    });
    expect(screen.getByText("Created KEEL-285")).toBeTruthy();
    expect(screen.getByText("Created KEEL-286")).toBeTruthy();
  });

  /// The region is in the DOM before anything is said. One that appears along
  /// with its first message is one assistive technology has not been watching,
  /// so the first announcement is the one it misses.
  it("keeps its live region mounted while empty", () => {
    render(<Toaster />);
    const region = screen.getByRole("status", { name: "Notifications" });
    expect(region.getAttribute("aria-live")).toBe("polite");
    expect(region.textContent).toBe("");
  });

  /// A `toast` call with nothing listening must not throw, or a screen that
  /// forgot the host takes the whole app down rather than losing a message.
  it("does nothing when no host is mounted", () => {
    expect(() => toast({ text: "Nobody is listening" })).not.toThrow();
  });
});
