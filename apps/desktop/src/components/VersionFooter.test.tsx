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

import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { VersionFooter, checkStatus } from "./VersionFooter";
import { api } from "../lib/api";

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

/**
 * KEEL-227. "Nothing is staged" was rendered as silence, and silence reads as
 * "you are up to date" — whether the daemon checked an hour ago, has been
 * failing quietly for a month, or is too old to check at all. Three states, one
 * appearance, and the further behind you were the less it said.
 */
describe("VersionFooter — whether checking is happening at all", () => {
  it("says a daemon that sends no staged_version at all predates updating entirely", () => {
    render(
      <VersionFooter
        version="0.1.0"
        stagedVersion={undefined}
        onApplied={() => {}}
      />,
    );

    // The absence of the field *is* the evidence: `staged_version` arrived
    // with the updater in 0.1.2, so a daemon without it has no updater.
    // Nothing outbound, nothing compared against a known-latest.
    expect(screen.getByText(/predates automatic updates/i)).toBeTruthy();
  });

  it("distinguishes a daemon that checks but cannot say when", () => {
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion={null}
        onApplied={() => {}}
      />,
    );

    // It has the updater — it sent `staged_version` — and only lacks the
    // stamp. Telling this person to reinstall would be false.
    expect(screen.queryByText(/predates automatic updates/i)).toBeNull();
    expect(screen.getByText(/cannot say when it last checked/i)).toBeTruthy();
  });

  it("says so when checks are switched off rather than implying all is well", () => {
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion={null}
        updateCheck={{ enabled: false }}
        onApplied={() => {}}
      />,
    );

    expect(screen.getByText(/KEEL_AUTO_UPDATE=0/)).toBeTruthy();
  });

  it("reports a check that ran and failed", () => {
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion={null}
        updateCheck={{
          enabled: true,
          last_checked_at: new Date().toISOString(),
          last_error: "could not reach the release manifest",
        }}
        onApplied={() => {}}
      />,
    );

    expect(
      screen.getByText(/could not reach the release manifest/i),
    ).toBeTruthy();
  });

  it("says nothing when a check succeeded recently, because there is nothing to say", () => {
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion={null}
        updateCheck={{
          enabled: true,
          last_checked_at: new Date().toISOString(),
          last_error: null,
        }}
        onApplied={() => {}}
      />,
    );

    expect(screen.queryByText(/update check/i)).toBeNull();
    expect(screen.queryByText(/too old/i)).toBeNull();
  });

  it("does not add a second update notice when one is already staged", () => {
    render(
      <VersionFooter
        version="0.1.0"
        stagedVersion="0.1.4"
        onApplied={() => {}}
      />,
    );

    // An update sitting there ready is the more useful thing to read, and a
    // line about checking would be arguing with it.
    expect(screen.queryByText(/cannot say when/i)).toBeNull();
    expect(screen.getByRole("button", { name: /restart into it/i })).toBeTruthy();
  });

  it("names the binary it is running, for a machine with more than one", () => {
    render(
      <VersionFooter
        version="0.1.0"
        stagedVersion={null}
        executable="/Users/kb/.cargo/bin/keel-daemon"
        onApplied={() => {}}
      />,
    );

    expect(
      screen.getByTitle("Running /Users/kb/.cargo/bin/keel-daemon"),
    ).toBeTruthy();
  });
});

describe("checkStatus", () => {
  const DAY = 24 * 60 * 60 * 1000;

  it("calls a check that has not run in days stale rather than silent", () => {
    const status = checkStatus(
      {
        enabled: true,
        last_checked_at: new Date(1_000_000_000_000 - 5 * DAY).toISOString(),
        last_error: null,
      },
      true,
      1_000_000_000_000,
    );

    expect(status?.text).toMatch(/Last checked for updates/);
    expect(status?.tone).toBe("warn");
  });

  it("is quiet about a check from an hour ago", () => {
    expect(
      checkStatus(
        {
          enabled: true,
          last_checked_at: new Date(1_000_000_000_000 - 3600_000).toISOString(),
          last_error: null,
        },
        true,
        1_000_000_000_000,
      ),
    ).toBeNull();
  });

  it("treats an unparseable timestamp as a check that has not happened", () => {
    // Failing closed: a stamp nobody can read cannot vouch for the version.
    const status = checkStatus(
      { enabled: true, last_checked_at: "not a date", last_error: null },
      true,
      1_000_000_000_000,
    );
    expect(status).not.toBeNull();
  });
});

/**
 * KEEL-258. There was no way to ask, so "no update showing" and "it has not
 * looked since the release existed" were the same picture — which is what
 * happened the day 0.1.5 was published.
 *
 * These stub `api.checkForUpdate` rather than `fetch`: `post` refuses without a
 * token taken from the document the daemon served, so a fetch-level stub never
 * runs and every test would pass for the wrong reason.
 */
describe("VersionFooter — asking for a check", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  function answers(body: Awaited<ReturnType<typeof api.checkForUpdate>>) {
    vi.spyOn(api, "checkForUpdate").mockResolvedValue(body);
  }

  it("offers a check when nothing is staged", () => {
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onApplied={() => {}} />,
    );
    expect(screen.getByRole("button", { name: /check for updates/i })).toBeTruthy();
  });

  it("does not offer one when an update is already downloaded", () => {
    render(
      <VersionFooter version="0.1.4" stagedVersion="0.1.5" onApplied={() => {}} />,
    );
    // The useful button at that point is the one that takes it. A second look
    // asks a question the daemon has already answered.
    expect(screen.queryByRole("button", { name: /check for updates/i })).toBeNull();
    expect(screen.getByRole("button", { name: /restart into it/i })).toBeTruthy();
  });

  it("says so when there is nothing new, rather than staying silent", async () => {
    answers({ outcome: "up_to_date", version: "0.1.5" });
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onApplied={() => {}} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    // The whole point: a check that found nothing has to be distinguishable
    // from a check that never ran.
    expect(await screen.findByText(/0\.1\.5 is the latest release/i)).toBeTruthy();
  });

  it("explains a release that cannot be applied automatically", async () => {
    answers({
      outcome: "needs_a_person",
      version: "0.2.0",
      schema_from: 4,
      schema_to: 5,
    });
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onApplied={() => {}} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    const status = await screen.findByRole("status");
    expect(status.textContent).toMatch(/0\.2\.0/);
    expect(status.textContent).toMatch(/schema 4 → 5/);
    expect(status.textContent).toMatch(/keel update/);
  });

  it("reports a failed check with its reason rather than as silence", async () => {
    answers({ outcome: "failed", error: "could not reach the release manifest" });
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onApplied={() => {}} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    expect(
      await screen.findByText(/could not reach the release manifest/i),
    ).toBeTruthy();
  });

  it("refreshes rather than announcing it, when the check stages something", async () => {
    answers({ outcome: "staged", version: "0.1.6" });
    let refreshed = 0;
    render(
      <VersionFooter
        version="0.1.5"
        stagedVersion={null}
        onApplied={() => {
          refreshed += 1;
        }}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    // The offer appears on the refresh. Saying it here as well would be the
    // interface talking over itself.
    expect(refreshed).toBe(1);
  });
});
