/**
 * The Inbox — signals nobody has decided about yet.
 *
 * A signal is something somebody wants: KB's own idea at five in the evening, a
 * request from a friend, a theme in support. It is not a task. Nothing has been
 * committed to, there is nothing to claim, and it stays out of the board, out
 * of `next` and out of the open-task count until somebody triages it (B-90).
 *
 * **Why "Inbox" and not "Ideas" or "Signals".** Every prettier name describes a
 * collection you would be pleased to grow. Nobody has ever felt bad about
 * having two hundred signals; everybody feels bad about two hundred unread. The
 * name has to imply the thing should be *emptied*, because a pile that grows
 * until it is too expensive to read is the exact failure this screen prevents.
 *
 * **Oldest first**, which is the opposite of every other list in the app. A
 * newest-first Inbox buries the thing that has been ignored longest under
 * whatever was filed this morning, and the bottom is where the problem is.
 *
 * The filing box is deliberately one field. Everything else — kind, priority, a
 * phase — is a choice, and a choice is a reason to close the box and do it
 * later. Capture that costs more than the thought did is capture that does not
 * happen, and that is the whole design rather than a nicety.
 */

import { useState } from "react";
import { ApiError, api, type Signal } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import {
  Badge,
  Button,
  Empty,
  ErrorBox,
  Spinner,
  TruncationNote,
  When,
} from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

/**
 * How long a signal may sit before the row says so.
 *
 * A fortnight, the same threshold the digest uses. Two places reading one
 * number would drift, so if a third ever needs it this moves into `lib` —
 * until then a duplicated constant with a comment beats a shared module with
 * one caller.
 */
const STALE_DAYS = 14;

const DAY_MS = 24 * 60 * 60 * 1000;

function daysSince(iso: string, now: number): number {
  return Math.floor((now - new Date(iso).getTime()) / DAY_MS);
}

/** Whether two timestamps land on the same calendar day, locally. */
function sameDay(a: string, b: string): boolean {
  return new Date(a).toDateString() === new Date(b).toDateString();
}

export function InboxScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const [filing, setFiling] = useState(false);

  const { data, error, loading, reload } = useAsync(
    () => api.inbox({ project: project ?? "" }),
    [project, generation],
  );

  const signals = data?.items ?? [];
  const now = Date.now();

  return (
    <Page
      title="Inbox"
      crumbs={projectCrumbs(route, "Inbox")}
      meta={
        <span className="text-small text-ink-faint">
          {data?.total ?? 0} untriaged · oldest first
        </span>
      }
      actions={
        <Button onClick={() => setFiling((f) => !f)}>
          {filing ? "Cancel" : "File a signal"}
        </Button>
      }
    >
      {filing && (
        <FileBox
          project={project ?? ""}
          onFiled={() => {
            setFiling(false);
            reload();
          }}
        />
      )}

      {loading && signals.length === 0 ? (
        <Spinner label="Reading the Inbox…" />
      ) : error ? (
        <ErrorBox error={error} retry={reload} />
      ) : signals.length === 0 ? (
        <Empty
          message="The Inbox is empty."
          hint="Anything anybody wants goes here first — yours, a friend's, a customer's. Nothing here is work until somebody says so."
        />
      ) : (
        <ol className="space-y-1.5">
          {signals.map((signal) => (
            <SignalRow key={signal.id} signal={signal} now={now} />
          ))}
        </ol>
      )}

      {data?.truncated ? (
        <TruncationNote shown={signals.length} total={data.total} />
      ) : null}
    </Page>
  );
}

/**
 * One signal.
 *
 * Not a link, because there is nowhere to go: a signal's whole content is its
 * sentence, and a detail page showing one sentence and four empty fields would
 * be a worse version of this row. That changes when triage lands (KEEL-324).
 */
function SignalRow({ signal, now }: { signal: Signal; now: number }) {
  const waited = daysSince(signal.audit.created_at, now);
  return (
    <li className="rounded-card border border-border-subtle bg-surface-raised px-3 py-2.5">
      <div className="flex items-baseline gap-2">
        <span className="min-w-0 flex-1 font-medium">{signal.summary}</span>
        {/* Only once it has actually been waiting. A badge on everything is a
            badge that says nothing, and the point of the age is to separate
            the four signals nobody has looked at in two months from the forty
            filed this week. */}
        {waited >= STALE_DAYS && (
          <Badge tone="warn">waiting {waited} days</Badge>
        )}
      </div>
      <div className="mt-0.5 flex flex-wrap items-baseline gap-x-2 text-small text-ink-muted">
        {/* Who asked, when there is somebody. An empty attribution reads as
            "somebody said this" and is worse than saying nothing, so the
            daemon stores an absent source rather than a blank one and this
            renders nothing at all. */}
        {signal.source ? <span>{signal.source}</span> : null}
        {/* When it was *filed*, which is the clock the waiting badge runs on.
            Showing `occurred_at` here instead made the two disagree in the
            only case where both exist: a fixture signal said on 29 July and
            filed today rendered the July date beside no badge at all, which
            reads as a three-week-old signal nobody is counting.
            
            How long we have been sitting on something starts when it reached
            the Inbox — a signal filed today about a conversation last year is
            not something anybody has been ignoring. */}
        <When iso={signal.audit.created_at} />
        {/* And when it was said, when that is a different day and somebody
            recorded it. Kept because it is what dates the *want*, which is a
            different question from how long the Inbox has held it. */}
        {signal.occurred_at &&
        !sameDay(signal.occurred_at, signal.audit.created_at) ? (
          <span className="text-ink-faint">
            said <When iso={signal.occurred_at} />
          </span>
        ) : null}
        {signal.kind !== "idea" ? (
          <span className="text-ink-faint">{signal.kind}</span>
        ) : null}
      </div>
    </li>
  );
}

/**
 * The filing box. One field, and everything else optional and out of the way.
 *
 * `source` is the one optional field shown, because it is the only one that
 * cannot be recovered later: what somebody said can be re-read, but who said it
 * is gone the moment the conversation is. Everything else — kind, contact, the
 * verbatim — is either defaulted or belongs in a session.
 */
function FileBox({
  project,
  onFiled,
}: {
  project: string;
  onFiled: () => void;
}) {
  const [said, setSaid] = useState("");
  const [source, setSource] = useState("");
  const [saving, setSaving] = useState(false);
  const [failed, setFailed] = useState<string | null>(null);

  async function submit() {
    if (saving || said.trim() === "") return;
    setSaving(true);
    setFailed(null);
    try {
      await api.createSignal({
        project,
        summary: said.trim(),
        ...(source.trim() ? { source: source.trim() } : {}),
      });
      setSaid("");
      setSource("");
      onFiled();
    } catch (e) {
      setFailed(
        e instanceof ApiError ? e.message : "The signal was not filed.",
      );
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="mb-4 space-y-2 rounded-card border border-border-subtle bg-surface-raised p-3">
      <label className="block space-y-1">
        <span className="text-micro text-ink-muted">
          What was said{" "}
          <span className="text-ink-faint">— in their words, not yours</span>
        </span>
        <textarea
          value={said}
          onChange={(e) => setSaid(e.target.value)}
          autoFocus
          rows={2}
          placeholder="this should work with codex"
          // Enter files it. The box exists to be gone in six seconds, and
          // reaching for the mouse to submit two lines of text is most of the
          // cost it is trying not to have. Shift+Enter still breaks a line,
          // for the occasional quote that needs one.
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void submit();
            }
          }}
          className="w-full rounded-md border border-border-subtle bg-surface px-3 py-2 text-small text-ink placeholder:text-ink-faint"
        />
      </label>

      <div className="flex items-center gap-2">
        <input
          value={source}
          onChange={(e) => setSource(e.target.value)}
          placeholder="Who asked (optional)"
          className="min-w-0 flex-1 rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-small text-ink placeholder:text-ink-faint"
        />
        <Button
          onClick={() => void submit()}
          disabled={saving || said.trim() === ""}
        >
          {saving ? "Filing…" : "File it"}
        </Button>
      </div>

      {failed ? <div className="text-small text-danger">{failed}</div> : null}
    </div>
  );
}
