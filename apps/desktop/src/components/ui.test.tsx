import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Button, Chip, Dialog, Menu, MenuItem, Tooltip, priorityTone, statusTone } from "./ui";

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
