import { useState } from "react";
import { ApiError, api, type Entity } from "../lib/api";
import { Button, Dialog } from "./ui";

/**
 * Closing a task, with the three things the storage layer requires.
 *
 * A component of its own rather than a piece of the task screen, because the
 * board reaches it too: dropping a card on the DONE column has to collect a
 * reason, a message and evidence exactly as the Close button does, and two
 * forms asking for the same three things would eventually ask differently.
 */
export function CloseTaskDialog({
  open,
  task,
  onClose,
  onDone,
}: {
  open: boolean;
  task: Entity;
  onClose: () => void;
  onDone: () => void;
}) {
  const [reason, setReason] = useState("done");
  const [message, setMessage] = useState("");
  const [evidence, setEvidence] = useState("");
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  async function submit() {
    if (saving) return;
    setSaving(true);
    setFailed(null);
    try {
      await api.closeTask(String(task.id), {
        reason,
        message: message.trim(),
        // One per line, because a commit sha and a URL both contain commas
        // often enough that splitting on them would quietly mangle evidence.
        evidence: evidence
          .split("\n")
          .map((line) => line.trim())
          .filter(Boolean),
      });
      onClose();
      onDone();
    } catch (e) {
      setFailed(e instanceof ApiError ? e.message : "It was not closed.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <Dialog open={open} onClose={onClose} label="Close this task">
      <div className="space-y-3 p-4">
        <h2 className="text-small font-semibold text-ink">Close this task</h2>

        <label className="block space-y-1">
          <span className="text-micro text-ink-muted">Reason</span>
          <select
            value={reason}
            onChange={(e) => setReason(e.target.value)}
            className="w-full rounded-md border border-border-subtle bg-surface px-2 py-1.5 text-small text-ink"
          >
            <option value="done">done — it is finished</option>
            <option value="wont_do">wont_do — deliberately not doing it</option>
            <option value="no_change">no_change — nothing needed doing</option>
          </select>
        </label>

        <label className="block space-y-1">
          <span className="text-micro text-ink-muted">
            What happened{" "}
            <span className="text-ink-faint">
              — required, in a sentence or two
            </span>
          </span>
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            rows={3}
            className="w-full resize-y rounded-md border border-border-subtle bg-surface px-3 py-2 text-small text-ink placeholder:text-ink-faint"
            placeholder="Shipped and checked against the published build."
          />
        </label>

        <label className="block space-y-1">
          <span className="text-micro text-ink-muted">
            Evidence{" "}
            <span className="text-ink-faint">
              — one per line. Required for `done`: commit:… pr:… test:… url:…
            </span>
          </span>
          <textarea
            value={evidence}
            onChange={(e) => setEvidence(e.target.value)}
            rows={2}
            className="w-full resize-y rounded-md border border-border-subtle bg-surface px-3 py-2 font-mono text-small text-ink placeholder:text-ink-faint"
            placeholder="commit:abc1234"
          />
        </label>

        {failed && (
          <p role="alert" className="text-micro text-bad">
            {failed}
          </p>
        )}

        <div className="flex justify-end gap-2 pt-1">
          <Button size="sm" variant="ghost" onClick={onClose}>
            Cancel
          </Button>
          <Button
            size="sm"
            variant="primary"
            onClick={() => void submit()}
            disabled={saving}
          >
            {saving ? "Closing…" : "Close task"}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
