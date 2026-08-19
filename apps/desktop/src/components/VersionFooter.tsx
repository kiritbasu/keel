/**
 * The version this daemon is running, and the one waiting, if any.
 *
 * Two things live here for one reason: you cannot tell what you are running
 * without leaving the interface, and the first person to install a second copy
 * of Specline could not work out which version it was. `specline --version` answers
 * it, but only if you know the binary is on your path — and the case where it
 * is not is exactly the case you are trying to diagnose.
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
    "The daemon did not come back after restarting. Check `specline-daemon`'s output.",
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
      text: "Update checks are off (SPECLINE_AUTO_UPDATE=0).",
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

/**
 * The three glyphs this control needs, drawn here rather than pulled from a
 * package.
 *
 * The app has no icon dependency and these are the fourth, fifth and sixth
 * icons in it — see `ThemeControl` for the other three. A package for six
 * icons would be a dependency, a build step and a licence for something that
 * is thirty lines of path data.
 *
 * **22px, in a 30px slot.** The size is deliberate and was chosen by looking:
 * at 13px the control read as decoration beside 11px type rather than
 * something to press. The slot is larger than the glyph so there is a real
 * click target, and so the spinner can swap in without the row changing
 * height.
 *
 * A 24 viewBox where `ThemeControl` uses 16, because these are Tabler-derived
 * outlines whose geometry is authored at 24 and rescaling the path data by
 * hand would be four chances to introduce a wobble for no gain. `strokeWidth`
 * is set to match the optical weight of the 16-box icons, not to match their
 * number.
 */
const ICON = "size-[22px]";
const STROKE = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 1.8,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/**
 * A cloud with an arrow coming down out of it.
 *
 * Chosen over a refresh arrow, which was the first draft: circling arrows mean
 * "reload what you are looking at", and this fetches a thing from somewhere
 * else. The cloud says the release is elsewhere and the arrow says bring it
 * here, which is what pressing this does.
 */
function CloudDownload() {
  return (
    <svg viewBox="0 0 24 24" className={ICON} aria-hidden="true">
      <g {...STROKE}>
        <path d="M19 18a3.5 3.5 0 0 0 0-7h-1a5 4.5 0 0 0-11-2 4.6 4.4 0 0 0-2.1 8.4" />
        <path d="M12 13v9" />
        <path d="M9 19l3 3l3-3" />
      </g>
    </svg>
  );
}

/** The same cloud, struck through. Checks are off, and that is a setting. */
function CloudOff() {
  return (
    <svg viewBox="0 0 24 24" className={ICON} aria-hidden="true">
      <g {...STROKE}>
        <path d="M13.5 5.5a5 4.5 0 0 1 5.5 4.5h1a3.5 3.5 0 0 1 2.4 6.1" />
        <path d="M17 17H6a4.6 4.4 0 0 1-.7-8.7" />
        <path d="M3 3l18 18" />
      </g>
    </svg>
  );
}

/**
 * An arc, spinning.
 *
 * In the same slot as the glyph it replaces, so pressing the button does not
 * move anything — a control that jumps when you use it reads as a mistake.
 */
function Spinner() {
  return (
    <svg
      viewBox="0 0 24 24"
      className={`${ICON} animate-spin`}
      aria-hidden="true"
    >
      <g {...STROKE}>
        <path d="M12 3a9 9 0 1 0 9 9" />
      </g>
    </svg>
  );
}

export function VersionFooter({
  version,
  stagedVersion,
  releaseNotes,
  stagedReleaseNotes,
  updateCheck,
  executable,
  onStaged,
  onApplied,
}: {
  version: string | undefined;
  stagedVersion: string | null | undefined;
  releaseNotes?: string;
  stagedReleaseNotes?: string | null;
  updateCheck?: UpdateCheck;
  executable?: string | null;
  /**
   * A check found something and staged it. Refetch, so the offer appears.
   *
   * Separate from `onApplied` because they are separate events, and collapsing
   * them made *checking* reload the whole page — the parent reloads on apply,
   * and one prop carrying both meanings inherited that.
   */
  onStaged: () => void;
  /** The daemon has restarted into the update. The parent reloads. */
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
          // Nothing to say here: the offer below appears once the data comes
          // back, and announcing it twice would be the interface talking over
          // itself. A refetch, not a reload — nothing has restarted.
          onStaged();
          break;
        case "already_staged":
          // Reachable in one ordinary way: press the button, decline the
          // restart, press it again. Saying "up to date" there would be false
          // with the update sitting on disk, and saying "staged" would claim
          // this press found it.
          onStaged();
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
            `${r.version} changes the store's shape (schema ${r.schema_from} → ${r.schema_to}), so it is not applied automatically. Run \`specline update\` to see what it involves.`,
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
      setFailed(
        e instanceof ApiError
          ? e.message
          : "The update could not be applied. Nothing has changed.",
      );
    } finally {
      // Cleared here rather than left to the parent's reload to destroy. The
      // original bug was a progress line that never went away, and "it goes
      // away because the page is replaced" is the same bug with a reload
      // standing in front of it — give this component a parent that does not
      // reload and it comes straight back.
      setApplying(null);
    }
  }

  if (!version) return null;

  // `undefined` means the daemon did not send `staged_version` at all, which
  // only a pre-0.1.2 daemon does; `null` means it sent one and nothing is
  // waiting. The difference is the whole of how the interface knows how far
  // back it is talking to, without asking anything.
  const status = checkStatus(updateCheck, stagedVersion !== undefined);

  // Switched off is its own glyph rather than its own colour. It is a choice
  // somebody made, not a fault, and an amber badge would nag about a setting
  // they chose deliberately.
  const off = updateCheck?.enabled === false;
  // The dot, and only for the two things worth interrupting for: something is
  // waiting, or the updater itself is not working. `null` is the resting state
  // and it is the only one that says nothing — see `checkStatus` for why that
  // silence is a claim the component is entitled to make.
  const dot = stagedVersion
    ? "accent"
    : status?.tone === "warn"
      ? "warn"
      : null;
  const glyph = stagedVersion
    ? "text-accent"
    : status?.tone === "warn"
      ? "text-warn"
      : "text-ink-faint";

  return (
    <div className="mt-cosy px-2.5">
      <div className="flex items-center gap-tight">
        <p
          className="text-micro text-ink-faint"
          // Which binary, not only which version. Two installs and the one on
          // your PATH not being the one you updated is the case this footer
          // exists for, and a version alone cannot tell them apart.
          title={executable ? `Running ${executable}` : undefined}
        >
          Specline{" "}
          {releaseNotes ? (
            // A version with no way to find out what is in it is a number. The
            // link is the release's own notes, which the release job generates,
            // so it is the changelog for exactly this build.
            <a
              href={releaseNotes}
              target="_blank"
              rel="noreferrer"
              className="font-mono underline decoration-dotted underline-offset-2 hover:text-ink"
              title={`What changed in v${version}`}
            >
              v{version}
            </a>
          ) : (
            <span className="font-mono">v{version}</span>
          )}
        </p>

        <span className="flex-1" />

        {/*
          The action, and only the action. The glyph is always "go and look",
          never "you are up to date" — a tick would mean both, which is one
          glyph carrying a verb and a state.

          Refused rather than hidden when checks are off, because the button
          disappearing would leave nothing to explain why, and `specline doctor`
          promises this daemon makes no request at all in that mode.
        */}
        <button
          type="button"
          onClick={check}
          disabled={checking || off}
          aria-label={
            off
              ? "Update checks are off"
              : checking
                ? "Checking for updates"
                : "Check for updates"
          }
          title={
            off
              ? "Update checks are off (SPECLINE_AUTO_UPDATE=0)"
              : status
                ? status.text
                : "Check for updates now"
          }
          className={`relative inline-flex size-[30px] shrink-0 items-center justify-center rounded-control transition-colors ${glyph} ${
            off
              ? "cursor-default"
              : stagedVersion
                ? "bg-accent-quiet hover:bg-surface-hover"
                : "hover:bg-surface-hover hover:text-ink"
          }`}
        >
          {checking ? <Spinner /> : off ? <CloudOff /> : <CloudDownload />}
          {dot && (
            <span
              aria-hidden="true"
              className={`absolute top-[2px] right-[2px] size-[8px] rounded-full ring-2 ring-surface ${
                dot === "accent" ? "bg-accent" : "bg-warn"
              }`}
            />
          )}
        </button>
      </div>

      {/*
        Said only when nothing is staged. An update sitting there ready is the
        more useful thing to read, and two notices about updating at once is
        one too many.

        The resting state — checked recently, nothing waiting — prints nothing
        at all, and that is the one state allowed to. Every state that needs
        the reader to know something keeps its sentence, which is what stops a
        silent icon meaning "fine" when the updater has been broken for a month
        (KEEL-227).
      */}
      {!stagedVersion && status && (
        <p
          className={`mt-tight text-micro ${
            status.tone === "warn" ? "text-ink-muted" : "text-ink-faint"
          }`}
        >
          {status.text}
        </p>
      )}

      {/*
        What the button found, when it found something worth a sentence. The
        staged case says nothing here — the offer below is the answer, and
        announcing it twice would be the interface talking over itself.
      */}
      {!stagedVersion && checkResult && (
        <p role="status" className="mt-tight text-micro text-ink-muted">
          {checkResult}
        </p>
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
