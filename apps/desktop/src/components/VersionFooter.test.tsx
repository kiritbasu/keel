/**
 * The version footer, and the thing it was missing: a way to find out what a
 * version contains.
 *
 * The number on its own is not information — KB asked for the link after taking
 * an update and having nowhere to read what he had taken. The URLs come from
 * the daemon rather than being composed here, because the repository is
 * configurable, so these tests check that what arrives is what is rendered and
 * that the absence of it degrades to plain text rather than a dead link.
 */

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { VersionFooter } from "./VersionFooter";

afterEach(cleanup);

const NOTES = "https://github.com/kiritbasu/keel/releases/tag/v0.1.3";
const STAGED_NOTES = "https://github.com/kiritbasu/keel/releases/tag/v0.1.4";

describe("VersionFooter", () => {
  it("links the running version to its release notes", () => {
    render(
      <VersionFooter
        version="0.1.3"
        stagedVersion={null}
        releaseNotes={NOTES}
        onApplied={() => {}}
      />,
    );

    const link = screen.getByRole("link", { name: "0.1.3" });
    expect(link.getAttribute("href")).toBe(NOTES);
    // A new tab, because this is a desktop shell and navigating it away from
    // the app would strand the reader with no way back.
    expect(link.getAttribute("target")).toBe("_blank");
  });

  it("shows the version as plain text when there is no link to give", () => {
    render(
      <VersionFooter
        version="0.1.3"
        stagedVersion={null}
        onApplied={() => {}}
      />,
    );

    expect(screen.getByText("0.1.3").tagName).toBe("SPAN");
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("offers the staged version's notes beside the restart button", () => {
    render(
      <VersionFooter
        version="0.1.3"
        stagedVersion="0.1.4"
        releaseNotes={NOTES}
        stagedReleaseNotes={STAGED_NOTES}
        onApplied={() => {}}
      />,
    );

    // Deciding whether to restart means knowing what the restart brings.
    expect(
      screen.getByRole("link", { name: /what's in it/i }).getAttribute("href"),
    ).toBe(STAGED_NOTES);
    expect(screen.getByRole("button", { name: /restart into it/i })).toBeTruthy();
  });

  it("still offers the restart when the staged notes are missing", () => {
    render(
      <VersionFooter
        version="0.1.3"
        stagedVersion="0.1.4"
        onApplied={() => {}}
      />,
    );

    // A missing link must not take the update with it — the offer is the
    // load-bearing half.
    expect(screen.getByRole("button", { name: /restart into it/i })).toBeTruthy();
    expect(screen.queryByRole("link")).toBeNull();
  });

  it("renders nothing at all until the version is known", () => {
    const { container } = render(
      <VersionFooter
        version={undefined}
        stagedVersion={null}
        onApplied={() => {}}
      />,
    );

    expect(container.innerHTML).toBe("");
  });
});
