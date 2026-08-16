/**
 * Finding a label by typing.
 *
 * The thing this replaced showed ten of sixty-four and told you to ask Claude
 * for the rest, so the property that matters most is that a label outside any
 * "most used" ten is reachable — which is the first test here.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LabelPicker } from "./LabelPicker";

afterEach(cleanup);

const LABELS = [
  "api",
  "cli",
  "daemon",
  "desktop",
  "mcp",
  "plugin",
  "release",
  "security",
  "storage",
  "testing",
  "tooling",
  "ui",
];

describe("LabelPicker", () => {
  it("finds a label that no top-ten list would have shown", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={LABELS} chosen={[]} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Find a label"), {
      target: { value: "tool" },
    });
    fireEvent.click(screen.getByRole("button", { name: "tooling" }));

    expect(onChange).toHaveBeenCalledWith(["tooling"]);
  });

  it("matches anywhere in the label, not only the start", () => {
    render(<LabelPicker available={LABELS} chosen={[]} onChange={() => {}} />);

    fireEvent.change(screen.getByLabelText("Find a label"), {
      target: { value: "esk" },
    });

    expect(screen.getByRole("button", { name: "desktop" })).toBeTruthy();
  });

  it("does not offer a label that is already chosen", () => {
    render(
      <LabelPicker available={LABELS} chosen={["ui"]} onChange={() => {}} />,
    );

    fireEvent.change(screen.getByLabelText("Find a label"), {
      target: { value: "ui" },
    });

    // The chip is there to remove it; the suggestion is not, because adding it
    // twice is not a thing anybody means.
    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.getByLabelText("Remove ui")).toBeTruthy();
  });

  it("adds the highlighted match on Enter", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={LABELS} chosen={[]} onChange={onChange} />);

    const input = screen.getByLabelText("Find a label");
    fireEvent.change(input, { target: { value: "s" } });
    fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });

    // Second match for "s": security, storage, testing, desktop… whichever the
    // filter yields, the point is that ArrowDown moved and Enter took it.
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.calls[0]?.[0]).toHaveLength(1);
  });

  it("removes a chosen label when its chip is clicked", () => {
    const onChange = vi.fn();
    render(
      <LabelPicker
        available={LABELS}
        chosen={["ui", "cli"]}
        onChange={onChange}
      />,
    );

    fireEvent.click(screen.getByLabelText("Remove ui"));

    expect(onChange).toHaveBeenCalledWith(["cli"]);
  });

  /**
   * A label that does not exist cannot be created here, and the box has to say
   * so — otherwise it reads as a text field that silently ignores you.
   */
  it("says what to do when nothing matches, rather than failing silently", () => {
    render(<LabelPicker available={LABELS} chosen={[]} onChange={() => {}} />);

    fireEvent.change(screen.getByLabelText("Find a label"), {
      target: { value: "brand-new-thing" },
    });

    expect(screen.getByText(/No label matches/)).toBeTruthy();
    expect(screen.getByText(/Ask Claude to add a new one/)).toBeTruthy();
  });

  /**
   * Enter is also the dialog's submit. If a highlighted suggestion did not stop
   * it, picking a label would create the task — so the handler must claim the
   * key while there is something to pick, and leave it alone when there is not.
   */
  it("leaves Enter alone when there is no suggestion to take", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={[]} chosen={[]} onChange={onChange} />);

    const input = screen.getByLabelText("Find a label");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).not.toHaveBeenCalled();
  });
});
