/**
 * What the interface shows before Keel has anything in it.
 *
 * # Why this is not an empty state
 *
 * The screen it replaces said "Nothing here yet", "Nothing unresolved" and "No
 * activity yet" — three restatements of absence, framed by two bordered panels
 * that were the largest shapes on the page and held the least. It was accurate
 * and it answered neither question a new user actually has.
 *
 * Those two questions are **is this working** and **what do I do**. A read-only
 * surface makes the first one genuinely hard: there is no button to press whose
 * failure would tell you anything, so an empty screen is indistinguishable from
 * a broken one. Hence the header states the connection positively and shows the
 * version, the schema and where the store is — three facts a broken install
 * could not produce.
 *
 * The second question is harder than it looks, because the honest answer is
 * "nothing, here". Keel fills up as a side effect of talking to Claude, and the
 * app cannot write by design (hard constraint 7). So the useful move is not a
 * call to action on this page but a sentence to say somewhere else, which is
 * why the prompts are copyable rather than clickable — the same reasoning, and
 * the same idiom, as `AskClaude` on the task screen. There is no URL that puts
 * text into a Claude Code session, and a button pretending otherwise would be
 * worse than one that is honest about the clipboard.
 *
 * # Why it disappears completely
 *
 * The moment there is one project this is gone, and nothing about it lingers.
 * An onboarding panel that survives its usefulness becomes furniture, and this
 * one occupies the space the roll-up needs.
 */

import { useState } from "react";
import { Card } from "./ui";

/** The sentences worth trying first, in the order they teach something. */
const PROMPTS = [
  {
    text: "we should add rate limiting to the API before launch",
    teaches: "becomes a task",
  },
  {
    text: "we decided on Postgres because we already run one",
    teaches: "becomes a decision, with the reasoning",
  },
  {
    text: "I don't know whether we need per-tenant keys yet",
    teaches: "becomes an open question",
  },
  {
    text: "what's the state of this project?",
    teaches: "reads it back",
  },
];

export function FirstRun({
  version,
  schema,
  home,
}: {
  version?: string;
  schema?: number;
  home?: string;
}) {
  const [copied, setCopied] = useState<string | null>(null);

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(text);
      window.setTimeout(() => setCopied((c) => (c === text ? null : c)), 1500);
    } catch {
      // A denied clipboard is not worth an error state on a convenience: the
      // text is on screen and can be selected by hand.
    }
  }

  return (
    <div className="space-y-5">
      {/* The connection, stated as a fact rather than implied by the absence of
          an error. On a read-only surface this is the only thing separating
          "working and empty" from "broken". */}
      <div className="rounded-lg border border-border-subtle bg-surface-raised px-5 py-4">
        <div className="flex items-center gap-2">
          <span
            className="h-2 w-2 shrink-0 rounded-full bg-accent"
            aria-hidden="true"
          />
          <span className="text-heading font-medium">Keel is running</span>
        </div>
        <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-small text-ink-muted">
          {version && <span>version {version}</span>}
          {schema !== undefined && (
            <>
              <span className="text-ink-faint">·</span>
              <span>schema {schema}</span>
            </>
          )}
          {home && (
            <>
              <span className="text-ink-faint">·</span>
              <span className="font-mono text-micro selectable">{home}</span>
            </>
          )}
        </div>
        <p className="mt-3 text-small text-ink-muted">
          The store is empty, which is what it should be before you have used
          it. Keel fills up as a side effect of working with Claude — you never
          type into it, and this page only ever reads.
        </p>
      </div>

      <Card title="Say one of these to Claude">
        <p className="mb-3 text-small text-ink-muted">
          Anywhere Claude Code is running, in any project. Copy one, paste it
          into a session, and refresh this page.
        </p>
        <ul className="space-y-1">
          {PROMPTS.map((prompt) => (
            <li key={prompt.text}>
              <button
                type="button"
                onClick={() => copy(prompt.text)}
                title="Copy, then paste into Claude Code"
                className="w-full rounded-control px-2 py-1.5 text-left text-small text-ink-muted hover:bg-surface-hover hover:text-ink"
              >
                <span className="font-mono text-micro">
                  {copied === prompt.text ? "copied" : "copy"}
                </span>{" "}
                <span className="selectable">{prompt.text}</span>
                <span className="ml-2 text-micro text-ink-faint">
                  {prompt.teaches}
                </span>
              </button>
            </li>
          ))}
        </ul>
      </Card>

      {/* The one failure a new install actually hits, and the only one this
          page can pre-empt. `/keel:setup` installs the binaries, but MCP
          servers are connected when Claude Code starts — so a session opened
          before setup finished has no `keel_*` tools however well it went, and
          the symptom is Claude saying it cannot find them. */}
      <p className="text-small text-ink-faint">
        No <span className="font-mono text-micro">keel_*</span> tools in your
        session? Restart Claude Code. MCP servers connect at startup, so a
        session that was already open when Keel was installed will not have
        them.
      </p>
    </div>
  );
}
