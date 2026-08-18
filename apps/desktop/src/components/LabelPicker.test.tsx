/**
 * Finding a label by typing.
 *
 * The thing this replaced showed ten of sixty-four and told you to ask Claude
 * for the rest, so the property that matters most is that a label outside any
 * "most used" ten is reachable — which is the first test here.
 *
 * The second property, since KEEL-304: a label that does not exist yet can be
 * made here, and doing so cannot produce a second spelling of one that already
 * does. Those two pull against each other, which is why creating was refused
 * for so long, so most of what follows is about the second one holding.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { LabelPicker, normaliseLabel } from "./LabelPicker";

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

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "tool" },
    });
    fireEvent.click(screen.getByRole("button", { name: "tooling" }));

    expect(onChange).toHaveBeenCalledWith(["tooling"]);
  });

  it("matches anywhere in the label, not only the start", () => {
    render(<LabelPicker available={LABELS} chosen={[]} onChange={() => {}} />);

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "esk" },
    });

    expect(screen.getByRole("button", { name: "desktop" })).toBeTruthy();
  });

  it("does not offer a label that is already chosen", () => {
    render(
      <LabelPicker available={LABELS} chosen={["ui"]} onChange={() => {}} />,
    );

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
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

    const input = screen.getByLabelText("Find or add a label");
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

  it("offers to create a label that does not exist yet", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={LABELS} chosen={[]} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "brand-new-thing" },
    });
    fireEvent.click(screen.getByRole("button", { name: /Create/ }));

    expect(onChange).toHaveBeenCalledWith(["brand-new-thing"]);
  });

  /**
   * The whole reason creating was refused for so long: `Data Safety` and
   * `data-safety` have to be the same label, or the board's facets, the filters
   * and the ranking each see two.
   */
  it("creates the normalised form, and says so before you take it", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={LABELS} chosen={[]} onChange={onChange} />);

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "  Data   Safety " },
    });

    // Shown rather than applied silently — the button names what it will make.
    const create = screen.getByRole("button", { name: /Create/ });
    expect(create.textContent).toContain("data-safety");

    fireEvent.click(create);
    expect(onChange).toHaveBeenCalledWith(["data-safety"]);
  });

  /**
   * Typing must not be able to produce a twin of something that already exists,
   * whatever case or spacing it arrives in.
   */
  it("offers the existing label rather than creating a twin of it", () => {
    const onChange = vi.fn();
    render(
      <LabelPicker
        available={["data-safety", ...LABELS]}
        chosen={[]}
        onChange={onChange}
      />,
    );

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "DATA SAFETY" },
    });

    expect(screen.queryByRole("button", { name: /Create/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "data-safety" }));
    expect(onChange).toHaveBeenCalledWith(["data-safety"]);
  });

  /**
   * `available` is not guaranteed normalised — MCP writes a label exactly as
   * given, by design (B-86) — so the picker has to fold both sides before
   * deciding what is already on the task. An exact-string check would offer
   * `UI` next to a chosen `ui` and put both on one row.
   */
  it("treats an unnormalised existing label as the one already chosen", () => {
    const onChange = vi.fn();
    render(
      <LabelPicker available={["UI", ...LABELS]} chosen={["ui"]} onChange={onChange} />,
    );

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "ui" },
    });

    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.getByText(/already on this task/)).toBeTruthy();
  });

  it("does not offer to create a label already on the task", () => {
    render(
      <LabelPicker available={LABELS} chosen={["ui"]} onChange={() => {}} />,
    );

    fireEvent.change(screen.getByLabelText("Find or add a label"), {
      target: { value: "UI" },
    });

    expect(screen.queryByRole("option")).toBeNull();
    expect(screen.getByText(/already on this task/)).toBeTruthy();
  });

  /**
   * Text that normalises to nothing is the one input that cannot become a
   * label. Silently offering nothing would read as the box ignoring you, which
   * is the failure the old refusal message existed to avoid.
   */
  it("says when what you typed could not be a label at all", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={LABELS} chosen={[]} onChange={onChange} />);

    const input = screen.getByLabelText("Find or add a label");
    fireEvent.change(input, { target: { value: "---" } });

    expect(screen.queryByRole("button", { name: /Create/ })).toBeNull();
    expect(screen.getByText(/nothing in it that could be a label/)).toBeTruthy();

    // And Enter falls through to the dialog rather than adding an empty label.
    fireEvent.keyDown(input, { key: "Enter" });
    expect(onChange).not.toHaveBeenCalled();
  });

  /**
   * Enter is also the dialog's submit. If a highlighted suggestion did not stop
   * it, picking a label would create the task — so the handler must claim the
   * key while there is something to pick, and leave it alone when there is not.
   */
  it("leaves Enter alone when there is no suggestion to take", () => {
    const onChange = vi.fn();
    render(<LabelPicker available={[]} chosen={[]} onChange={onChange} />);

    const input = screen.getByLabelText("Find or add a label");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(onChange).not.toHaveBeenCalled();
  });
});

describe("normaliseLabel", () => {
  it("folds the spellings that would otherwise split one label into several", () => {
    for (const raw of ["ui", "UI", " ui ", "Ui", "-ui-", "  UI  "]) {
      expect(normaliseLabel(raw)).toBe("ui");
    }
    expect(normaliseLabel("Data   Safety")).toBe("data-safety");
    expect(normaliseLabel("data--safety")).toBe("data-safety");
  });

  it("leaves every label already in use unchanged", () => {
    // The rule codifies the set rather than imposing on it: if any of these
    // moved, the reversal in KEEL-304 would have needed a migration.
    for (const label of LABELS) expect(normaliseLabel(label)).toBe(label);
    for (const label of ["data-safety", "decision-needed", "phase10", "8a"]) {
      expect(normaliseLabel(label)).toBe(label);
    }
  });

  it("returns nothing for text with nothing label-shaped in it", () => {
    for (const raw of ["", "   ", "-", "---", " - - "]) {
      expect(normaliseLabel(raw)).toBe("");
    }
  });
});
