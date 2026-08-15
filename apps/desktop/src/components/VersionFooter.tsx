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
 */

import { useState } from "react";
import { ApiError, api } from "../lib/api";
import { Button } from "./ui";

export function VersionFooter({
  version,
  stagedVersion,
  onApplied,
}: {
  version: string | undefined;
  stagedVersion: string | null | undefined;
  onApplied: () => void;
}) {
  const [applying, setApplying] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  async function apply() {
    setApplying(true);
    setFailed(null);
    try {
      await api.applyUpdate();
      // The daemon is going away. Give it a moment to come back on the new
      // binary before asking the page to refetch, rather than racing the
      // restart and reporting the gap as a failure.
      setTimeout(onApplied, 1500);
    } catch (e) {
      setApplying(false);
      setFailed(
        e instanceof ApiError
          ? e.message
          : "The update could not be applied. Nothing has changed.",
      );
    }
  }

  if (!version) return null;

  return (
    <div className="mt-cosy px-2.5">
      <p className="text-micro text-ink-faint">
        Keel <span className="font-mono">{version}</span>
      </p>

      {stagedVersion && !applying && (
        <div className="mt-cosy">
          <p className="text-micro text-ink-muted">
            <span className="font-mono">{stagedVersion}</span> is downloaded and
            verified.
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
          Restarting the daemon into {stagedVersion}…
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
