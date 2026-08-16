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
import { ApiError, api } from "../lib/api";
import * as footer from "./VersionFooter";

afterEach(cleanup);

const NOTES = "https://github.com/kiritbasu/specline/releases/tag/v0.1.3";
const STAGED_NOTES = "https://github.com/kiritbasu/specline/releases/tag/v0.1.4";

describe("VersionFooter", () => {
  it("links the running version to its release notes", () => {
    render(
      <VersionFooter
        version="0.1.3"
        stagedVersion={null}
        releaseNotes={NOTES}
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
        onApplied={() => {}}
      />,
    );

    expect(screen.getByText(/SPECLINE_AUTO_UPDATE=0/)).toBeTruthy();
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        onStaged={() => {}}
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
        executable="/Users/kb/.cargo/bin/specline-daemon"
        onStaged={() => {}}
        onApplied={() => {}}
      />,
    );

    expect(
      screen.getByTitle("Running /Users/kb/.cargo/bin/specline-daemon"),
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
      <VersionFooter version="0.1.5" stagedVersion={null} onStaged={() => {}}
        onApplied={() => {}} />,
    );
    expect(screen.getByRole("button", { name: /check for updates/i })).toBeTruthy();
  });

  it("does not offer one when an update is already downloaded", () => {
    render(
      <VersionFooter version="0.1.4" stagedVersion="0.1.5" onStaged={() => {}}
        onApplied={() => {}} />,
    );
    // The useful button at that point is the one that takes it. A second look
    // asks a question the daemon has already answered.
    expect(screen.queryByRole("button", { name: /check for updates/i })).toBeNull();
    expect(screen.getByRole("button", { name: /restart into it/i })).toBeTruthy();
  });

  it("says so when there is nothing new, rather than staying silent", async () => {
    answers({ outcome: "up_to_date", version: "0.1.5" });
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onStaged={() => {}}
        onApplied={() => {}} />,
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
      <VersionFooter version="0.1.5" stagedVersion={null} onStaged={() => {}}
        onApplied={() => {}} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    const status = await screen.findByRole("status");
    expect(status.textContent).toMatch(/0\.2\.0/);
    expect(status.textContent).toMatch(/schema 4 → 5/);
    expect(status.textContent).toMatch(/specline update/);
  });

  it("reports a failed check with its reason rather than as silence", async () => {
    answers({ outcome: "failed", error: "could not reach the release manifest" });
    render(
      <VersionFooter version="0.1.5" stagedVersion={null} onStaged={() => {}}
        onApplied={() => {}} />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    expect(
      await screen.findByText(/could not reach the release manifest/i),
    ).toBeTruthy();
  });

  it("refetches when a check stages something, and does not reload", async () => {
    answers({ outcome: "staged", version: "0.1.6" });
    let refreshed = 0;
    let reloaded = 0;
    render(
      <VersionFooter
        version="0.1.5"
        stagedVersion={null}
        onStaged={() => {
          refreshed += 1;
        }}
        onApplied={() => {
          reloaded += 1;
        }}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: /check for updates/i }));

    // The offer appears once the data comes back. Nothing has restarted, so
    // reloading the page here would be a full navigation for a check — which
    // is what one prop carrying both meanings caused.
    expect(refreshed).toBe(1);
    expect(reloaded).toBe(0);
  });
});

/**
 * KEEL-259. KB took an update and was left with `Restarting the daemon into …`
 * on screen — for ever, and with no version in it.
 */
describe("VersionFooter — taking an update", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("names the version it is taking, even though nothing is staged by then", async () => {
    vi.spyOn(api, "applyUpdate").mockResolvedValue({
      applied: "0.1.5",
      restarting: true,
    });
    // Never comes back, so the message stays up and can be read. The reload is
    // covered separately.
    vi.spyOn(api, "health").mockRejectedValue(new Error("restarting"));

    render(
      <VersionFooter version="0.1.4" stagedVersion="0.1.5" onStaged={() => {}}
        onApplied={() => {}} />,
    );
    await userEvent.click(screen.getByRole("button", { name: /restart into it/i }));

    // The bug was reading `stagedVersion` here, which is null once the daemon
    // has restarted — leaving an ellipsis with nothing before it.
    expect(await screen.findByText(/Restarting the daemon into 0\.1\.5…/)).toBeTruthy();
  });

  it("tells the parent only once the daemon answers again", async () => {
    vi.spyOn(api, "applyUpdate").mockResolvedValue({
      applied: "0.1.5",
      restarting: true,
    });
    let answers = false;
    vi.spyOn(api, "health").mockImplementation(async () => {
      if (!answers) throw new Error("still restarting");
      return { status: "ok", protocol: "x", version: "0.1.5", projects: 1, store_busy: false };
    });

    let applied = 0;
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion="0.1.5"
        onStaged={() => {}}
        onApplied={() => {
          applied += 1;
        }}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /restart into it/i }));

    // Not yet: the daemon has not come back, and firing now is what the old
    // fixed 1500ms wait did — reporting a failure for something that worked.
    expect(applied).toBe(0);

    answers = true;
    // `onApplied` is what reloads, in App.tsx. The daemon serves this
    // interface, so the binary it restarted into serves a different bundle;
    // refetching would leave the replaced build running.
    await vi.waitFor(() => expect(applied).toBe(1), { timeout: 4000 });
  });

  it("says the daemon did not come back, rather than waiting for ever", async () => {
    vi.spyOn(api, "applyUpdate").mockResolvedValue({
      applied: "0.1.5",
      restarting: true,
    });
    vi.spyOn(api, "health").mockRejectedValue(new Error("gone"));

    // One attempt, no gap: the real thing waits twenty seconds, which is not a
    // thing to sit through in a test.
    await expect(footer.waitForDaemon(1, 0)).rejects.toThrow(/did not come back/i);
  });

  it("clears the progress line and explains when applying fails", async () => {
    vi.spyOn(api, "applyUpdate").mockRejectedValue(
      new ApiError("nothing is staged, so there is nothing to apply.", 400),
    );

    render(
      <VersionFooter version="0.1.4" stagedVersion="0.1.5" onStaged={() => {}}
        onApplied={() => {}} />,
    );
    await userEvent.click(screen.getByRole("button", { name: /restart into it/i }));

    expect(await screen.findByRole("alert")).toHaveProperty("textContent");
    expect(screen.queryByText(/Restarting the daemon into/)).toBeNull();
  });
});

/**
 * Both found reviewing this session's own work, and both were mine.
 */
describe("VersionFooter — what the review caught", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("clears the progress line itself, without the parent reloading", async () => {
    vi.spyOn(api, "applyUpdate").mockResolvedValue({
      applied: "0.1.5",
      restarting: true,
    });
    vi.spyOn(api, "health").mockResolvedValue({
      status: "ok",
      protocol: "x",
      version: "0.1.5",
      projects: 1,
      store_busy: false,
    });

    // A parent that does nothing. The first fix worked only because the real
    // parent reloads and destroys the state — which is the same bug with a
    // reload standing in front of it.
    render(
      <VersionFooter
        version="0.1.4"
        stagedVersion="0.1.5"
        onStaged={() => {}}
        onApplied={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /restart into it/i }));

    await vi.waitFor(() =>
      expect(screen.queryByText(/Restarting the daemon into/)).toBeNull(),
    );
  });
});
