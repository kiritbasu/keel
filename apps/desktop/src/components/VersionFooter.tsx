/**
 * The version this daemon is running, and the one waiting, if any.
 *
 * Two things live here for one reason: you cannot tell what you are running
 * without leaving the interface, and the first person to install a second copy
 * of Keel could not work out which version it was. `keel --version` answers it,
 * but only if you know the binary is on your path — and the case where it is
 * not is exactly the case you are trying to diagnose.
 *
 * The update half is the interface's one write (B-75, amending hard constraint
 * 7). It cannot choose a version: the daemon checks on its own schedule and
 * stages what is safe, and this asks it to apply what it already staged.
 *
 * Applying restarts the daemon, so the button says so before the click rather
 * than after. The page then loses its connection for a second or two, which is
 * indistinguishable from a crash unless it is named — hence the wait state
 * saying the daemon is restarting rather than an error.
 *
 * `onApplied` fires once the daemon answers again, and the parent reloads:
 * refetching data left the browser running the interface the *previous* binary
 * served, which worked and was quietly the wrong version (KEEL-259).
 */

import { useState } from "react";
import { ApiError, api } from "../lib/api";
import { Button } from "./ui";

/**
 * What the daemon says about its own update checking, or `undefined` when it is
 * too old to say anything.
 *
 * The absence is load-bearing. A daemon from before the updater has no such
 * field, and that alone establishes it is behind — no request, no comparison
 * against a known-latest, nothing outbound.
 */
export type UpdateCheck = {
  enabled?: boolean;
  last_checked_at?: string | null;
  last_error?: string | null;
};

/** How long a check may be silent before its silence is worth naming. */
const STALE_AFTER_MS = 48 * 60 * 60 * 1000;

/**
 * Wait until the daemon answers again, or give up.
 *
 * Applying an update replaces the process, so there is a gap where nothing is
 * listening. This used to be `setTimeout(…, 1500)` — a guess about how long a
 * restart takes, which on a slow machine reported a failure for something that
 * had worked.
 *
 * Returns when health answers. Throws when it has not come back within the
 * deadline, which is a real failure worth showing: the daemon was asked to
 * restart and did not.
 */
export async function waitForDaemon(
  attempts = 40,
  gapMs = 500,
): Promise<void> {
  for (let i = 0; i < attempts; i += 1) {
    try {
      await api.health();
      return;
    } catch {
      await new Promise((resolve) => setTimeout(resolve, gapMs));
    }
  }
  throw new Error(
    "The daemon did not come back after restarting. Check `keel-daemon`'s output.",
  );
}

/**
 * The one sentence to put under the version, or null when there is nothing
 * worth saying.
 *
 * Exported for its tests: every branch here is a state a person has been in and
 * could not distinguish from being current.
 */
export function checkStatus(
  updateCheck: UpdateCheck | undefined,
  knowsAboutUpdates: boolean,
  now: number = Date.now(),
): { text: string; tone: "faint" | "warn" } | null {
  // Two different absences, and conflating them would put a false sentence on
  // the screen. `staged_version` appeared with the updater in 0.1.2, so a
  // daemon without it has no updater at all and will never find one. A daemon
  // that has the field but no `update_check` does check — it just cannot say
  // when, so its silence still is not evidence of being current (KEEL-227).
  if (!knowsAboutUpdates) {
    return {
      text: "This daemon predates automatic updates and will never find one. Reinstall to get a version that can.",
      tone: "warn",
    };
  }
  if (!updateCheck) {
    return {
      text: "This daemon cannot say when it last checked for updates.",
      tone: "faint",
    };
  }
  if (updateCheck.enabled === false) {
    return {
      text: "Update checks are off (KEEL_AUTO_UPDATE=0).",
      tone: "faint",
    };
  }
  const at = updateCheck.last_checked_at
    ? new Date(updateCheck.last_checked_at)
    : null;
  // A stamp nobody can read is treated as no stamp. Failing closed, because
  // the alternative is a timestamp that cannot be interpreted vouching for a
  // version by saying nothing.
  const when = at !== null && !Number.isNaN(at.getTime()) ? at : null;
  if (when === null) {
    return { text: "No update check has completed yet.", tone: "faint" };
  }

  if (updateCheck.last_error) {
    return {
      text: `Last update check failed: ${updateCheck.last_error}`,
      tone: "warn",
    };
  }
  if (now - when.getTime() > STALE_AFTER_MS) {
    return {
      text: `Last checked for updates ${when.toLocaleDateString()} — nothing since.`,
      tone: "warn",
    };
  }
  return null;
}

export function VersionFooter({
  version,
  stagedVersion,
  releaseNotes,
  stagedReleaseNotes,
  updateCheck,
  executable,
  onApplied,
}: {
  version: string | undefined;
  stagedVersion: string | null | undefined;
  releaseNotes?: string;
  stagedReleaseNotes?: string | null;
  updateCheck?: UpdateCheck;
  executable?: string | null;
  onApplied: () => void;
}) {
  // The version being taken, or null. A string rather than a boolean because
  // the message names it, and reading it back from `stagedVersion` was the bug:
  // by then the daemon has restarted and nothing is staged (KEEL-259).
  const [applying, setApplying] = useState<string | null>(null);
  const [failed, setFailed] = useState<string | null>(null);
  const [checking, setChecking] = useState(false);
  const [checkResult, setCheckResult] = useState<string | null>(null);

  async function check() {
    setChecking(true);
    setCheckResult(null);
    setFailed(null);
    try {
      const r = await api.checkForUpdate();
      switch (r.outcome) {
        case "staged":
          // Nothing to say here: the offer below appears on the refresh, and
          // announcing it twice would be the interface talking over itself.
          onApplied();
          break;
        case "up_to_date":
          setCheckResult(`${r.version ?? version} is the latest release.`);
          break;
        case "ahead":
          // Ordinary rather than exotic: anybody running a prerelease is ahead
          // of what `releases/latest` resolves to.
          setCheckResult(
            `This build is ahead of the latest release (${r.published}).`,
          );
          break;
        case "needs_a_person":
          setCheckResult(
            `${r.version} changes the store's shape (schema ${r.schema_from} → ${r.schema_to}), so it is not applied automatically. Run \`keel update\` to see what it involves.`,
          );
          break;
        case "failed":
          setCheckResult(`The check did not complete: ${r.error}`);
          break;
      }
    } catch (e) {
      setCheckResult(
        e instanceof ApiError
          ? e.message
          : "The check could not be made. Nothing has changed.",
      );
    } finally {
      setChecking(false);
    }
  }

  async function apply() {
    // Captured now, not read back later. By the time this renders the daemon
    // has restarted and health reports nothing staged, so reading
    // `stagedVersion` gave "Restarting the daemon into …" with the version
    // missing — the ellipsis with nothing before it that KB screenshotted.
    const taking = stagedVersion;
    setApplying(taking ?? "the staged version");
    setFailed(null);
    try {
      await api.applyUpdate();
      // **Reload, rather than refetch.** The daemon serves this interface, so
      // the binary that just replaced it serves a different bundle. Refetching
      // data left the browser running the *previous* build's UI against the new
      // daemon — everything worked, and it was quietly not the version you had
      // just installed.
      //
      // Waiting for health to answer rather than sleeping a fixed 1500ms: that
      // number was a guess about how long a process takes to come back, and on
      // a slow machine it raced the restart and reported a failure for
      // something that had worked.
      await waitForDaemon();
      // The parent decides what "it came back" means. It reloads, because the
      // daemon serves this interface and the binary that just replaced it
      // serves a different bundle — see `App.tsx`. Deciding that here would put
      // a page-level navigation inside a footer.
      onApplied();
    } catch (e) {
      setApplying(null);
      setFailed(
        e instanceof ApiError
          ? e.message
          : "The update could not be applied. Nothing has changed.",
      );
    }
  }

  if (!version) return null;

  // `undefined` means the daemon did not send `staged_version` at all, which
  // only a pre-0.1.2 daemon does; `null` means it sent one and nothing is
  // waiting. The difference is the whole of how the interface knows how far
  // back it is talking to, without asking anything.
  const status = checkStatus(updateCheck, stagedVersion !== undefined);

  return (
    <div className="mt-cosy px-2.5">
      <p
        className="text-micro text-ink-faint"
        // Which binary, not only which version. Two installs and the one on
        // your PATH not being the one you updated is the case this footer
        // exists for, and a version alone cannot tell them apart.
        title={executable ? `Running ${executable}` : undefined}
      >
        Keel{" "}
        {releaseNotes ? (
          // A version with no way to find out what is in it is a number. The
          // link is the release's own notes, which the release job generates,
          // so it is the changelog for exactly this build.
          <a
            href={releaseNotes}
            target="_blank"
            rel="noreferrer"
            className="font-mono underline decoration-dotted underline-offset-2 hover:text-ink"
            title={`What changed in ${version}`}
          >
            {version}
          </a>
        ) : (
          <span className="font-mono">{version}</span>
        )}
      </p>

      {/*
        Said only when nothing is staged. An update sitting there ready is the
        more useful thing to read, and two notices about updating at once is
        one too many.
      */}
      {!stagedVersion && status && (
        <p
          className={`mt-cosy text-micro ${
            status.tone === "warn" ? "text-ink-muted" : "text-ink-faint"
          }`}
        >
          {status.text}
        </p>
      )}

      {/*
        Only when nothing is staged. With an update already downloaded, the
        useful button is the one that takes it — offering a second look at that
        point is asking a question the daemon has already answered.

        It is here at all because waiting up to an hour was the only way to find
        out, and "I checked and there is nothing" looked identical to "I have
        not looked since before it was published" (KEEL-258).
      */}
      {!stagedVersion && (
        <div className="mt-cosy">
          <Button size="sm" variant="ghost" onClick={check} disabled={checking}>
            {checking ? "Checking…" : "Check for updates"}
          </Button>
          {checkResult && (
            <p role="status" className="mt-cosy text-micro text-ink-muted">
              {checkResult}
            </p>
          )}
        </div>
      )}

      {stagedVersion && !applying && (
        <div className="mt-cosy">
          <p className="text-micro text-ink-muted">
            <span className="font-mono">{stagedVersion}</span> is downloaded and
            verified.{" "}
            {stagedReleaseNotes && (
              // Before the button, deliberately: deciding whether to restart
              // means knowing what the restart brings, and this is the only
              // place that answers it.
              <a
                href={stagedReleaseNotes}
                target="_blank"
                rel="noreferrer"
                className="underline decoration-dotted underline-offset-2 hover:text-ink"
              >
                What's in it
              </a>
            )}
          </p>
          <Button
            size="sm"
            variant="ghost"
            onClick={apply}
            title={`Restart the daemon into ${stagedVersion}. It will be unavailable for a moment.`}
          >
            Restart into it
          </Button>
        </div>
      )}

      {applying && (
        <p role="status" className="mt-cosy text-micro text-ink-faint">
          Restarting the daemon into {applying}…
        </p>
      )}

      {failed && (
        <p role="alert" className="mt-cosy text-micro text-bad">
          {failed}
        </p>
      )}
    </div>
  );
}
