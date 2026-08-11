import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Button, Chip, Dialog, Menu, MenuItem, Tooltip, priorityTone, statusTone, when } from "./ui";

afterEach(cleanup);

describe("Button", () => {
  it("is type=button by default, so a button inside a form does not submit it", () => {
    render(<Button>Go</Button>);
    expect(screen.getByRole("button", { name: "Go" }).getAttribute("type")).toBe("button");
  });

  // Failure case: a disabled control that still fires is worse than no disabled
  // state at all, because the reader is told the action is unavailable and it
  // happens anyway.
  it("does not fire when disabled", () => {
    const onClick = vi.fn();
    render(
      <Button disabled onClick={onClick}>
        Go
      </Button>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Go" }));
    expect(onClick).not.toHaveBeenCalled();
  });
});

describe("Chip", () => {
  it("reports whether it is on", () => {
    const { rerender } = render(<Chip selected={false}>urgent only</Chip>);
    expect(screen.getByRole("button").getAttribute("aria-pressed")).toBe("false");
    rerender(<Chip selected>urgent only</Chip>);
    expect(screen.getByRole("button").getAttribute("aria-pressed")).toBe("true");
  });

  it("looks different when selected — one answer, not six", () => {
    const { rerender } = render(<Chip selected={false}>x</Chip>);
    const off = screen.getByRole("button").className;
    rerender(<Chip selected>x</Chip>);
    expect(screen.getByRole("button").className).not.toBe(off);
  });
});

describe("Dialog", () => {
  it("renders nothing when closed", () => {
    render(
      <Dialog open={false} onClose={() => {}} label="Test">
        inside
      </Dialog>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes on Escape", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} label="Test">
        inside
      </Dialog>,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledOnce();
  });

  // Failure case: a stray keypress must not dismiss a modal.
  it("stays open for any other key", () => {
    const onClose = vi.fn();
    render(
      <Dialog open onClose={onClose} label="Test">
        inside
      </Dialog>,
    );
    fireEvent.keyDown(window, { key: "a" });
    expect(onClose).not.toHaveBeenCalled();
  });
});

describe("Menu", () => {
  it("opens, reports its state, and closes when an item asks it to", () => {
    const onPick = vi.fn();
    render(
      <Menu label="Revision v3">
        {(close) => (
          <MenuItem
            onClick={() => {
              close();
              onPick();
            }}
          >
            v2
          </MenuItem>
        )}
      </Menu>,
    );
    const trigger = screen.getByRole("button", { name: /Revision v3/ });
    expect(trigger.getAttribute("aria-expanded")).toBe("false");

    fireEvent.click(trigger);
    expect(trigger.getAttribute("aria-expanded")).toBe("true");

    fireEvent.click(screen.getByRole("menuitem", { name: "v2" }));
    expect(onPick).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("closes when the click lands outside it", () => {
    render(<Menu label="Pick">{() => <MenuItem>v2</MenuItem>}</Menu>);
    fireEvent.click(screen.getByRole("button", { name: /Pick/ }));
    expect(screen.getByRole("menu")).toBeTruthy();

    fireEvent.mouseDown(document.body);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});

describe("Tooltip", () => {
  it("shows on hover and on focus, not only on hover", () => {
    render(
      <Tooltip text="Pick a project first">
        <button>Board</button>
      </Tooltip>,
    );
    expect(screen.queryByRole("tooltip")).toBeNull();

    fireEvent.focus(screen.getByRole("button"));
    expect(screen.getByRole("tooltip").textContent).toBe("Pick a project first");

    fireEvent.blur(screen.getByRole("button"));
    expect(screen.queryByRole("tooltip")).toBeNull();
  });
});

describe("tones", () => {
  it("gives blocked and done visibly different treatments", () => {
    expect(statusTone("blocked")).not.toBe(statusTone("done"));
  });

  it("falls back rather than returning nothing for a status it has never seen", () => {
    expect(statusTone("brand_new_status")).toContain("border-border-subtle");
    expect(priorityTone(undefined)).toContain("border-border-subtle");
  });
});

describe("when", () => {
  // A fixed instant, so a boundary can be asserted without waiting for the
  // clock to cross it.
  const NOW = Date.parse("2026-08-11T12:00:00Z");
  const at = (iso: string) => when(iso, NOW);

  it("reads the recent past the way a feed should", () => {
    expect(at("2026-08-11T11:59:40Z")).toBe("just now");
    expect(at("2026-08-11T11:56:00Z")).toBe("4m ago");
    expect(at("2026-08-11T10:00:00Z")).toBe("2h ago");
    expect(at("2026-08-10T12:00:00Z")).toBe("yesterday");
    expect(at("2026-08-08T12:00:00Z")).toBe("3d ago");
  });

  // The failure this existed to fix. A milestone target is a future date, and
  // the old helper subtracted with no negative branch, so it rendered "-3d ago"
  // on the one screen most likely to hit it.
  it("can express the future, which is what a roadmap is made of", () => {
    expect(at("2026-08-14T12:00:00Z")).toBe("in 3 days");
    expect(at("2026-08-12T12:00:00Z")).toBe("tomorrow");
    expect(at("2026-08-11T14:00:00Z")).toBe("in 2h");
    expect(at("2026-08-11T12:04:00Z")).toBe("in 4m");
  });

  // "Aug 9" silently means five different days on a project more than a year
  // old, so the year appears whenever it is not the current one.
  it("keeps the year when it is not this year", () => {
    expect(at("2026-01-04T12:00:00Z")).not.toMatch(/2026/);
    expect(at("2025-08-09T12:00:00Z")).toMatch(/2025/);
  });

  it("hands back anything it cannot parse rather than rendering NaN", () => {
    expect(at("not a date")).toBe("not a date");
  });
});
