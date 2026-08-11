/**
 * What changed — sessions newest first, each expandable to what it did.
 *
 * This replaces a feed of up to 300 mutations whose own header said its job was
 * "what did Claude do today" and which answered "what were the last 300 events".
 * None of the rows was a link, there was no grouping, and there was no time
 * range. KB chose the rebuild over the cheap fix (TQ-35), on the grounds that
 * "what happened while I was away" is the single most valuable question this app
 * can answer for someone who leaves Claude working and comes back.
 *
 * Three things make it answer that question rather than list events:
 *
 * 1. **Grouped by session**, with a one-line account of each. Notes are in the
 *    union, which is the part the event log alone could not have given — a note
 *    writes no event (TQ-29), and a note is where a session records what it
 *    found.
 * 2. **Every row goes somewhere.** That was the most-cited defect and it was
 *    total: `Activity.tsx` had no anchor of any kind.
 * 3. **A marker for what is new since you were last here**, from a timestamp in
 *    local storage, plus a today / this week / everything range.
 */

import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { href, setQuery } from "../lib/router";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Spinner, Tooltip, When, when } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

const ACTORS = ["human", "claude", "github", "system"] as const;

/** The ranges offered, and what each means as a `since`. */
const RANGES = {
  today: "Today",
  week: "This week",
  all: "Everything",
} as const;
type RangeId = keyof typeof RANGES;

/** Where the "last here" mark is kept. Local, because it is about this reader. */
const SEEN_KEY = "keel.changed.lastSeen";

interface ChangeRow {
  id: string;
  kind: "field" | "created" | "note";
  entity_id: string;
  entity_type: string;
  reference: string;
  summary: string;
  at: string;
}

interface SessionRow {
  session_id: string | null;
  actor: string;
  started_at: string;
  ended_at: string;
  headline: string;
  changes: ChangeRow[];
}

interface ChangedResponse {
  sessions: SessionRow[];
  changes: number;
  truncated: boolean;
}

/**
 * The instant a range starts, or undefined for everything.
 *
 * Computed from the reader's own clock rather than asked of the daemon, because
 * "today" means the day the person is having, not the day UTC is having.
 */
function sinceFor(range: RangeId, now = new Date()): string | undefined {
  if (range === "all") return undefined;
  const start = new Date(now);
  start.setHours(0, 0, 0, 0);
  if (range === "week") start.setDate(start.getDate() - 6);
  return start.toISOString();
}

/** What this reader had already seen, before this visit updated it. */
function readLastSeen(): string | null {
  try {
    return window.localStorage.getItem(SEEN_KEY);
  } catch {
    // Private browsing, or storage disabled. The marker is a convenience and its
    // absence is not worth an error state.
    return null;
  }
}

export function ChangedScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const actor = route.query.actor;
  const range = (route.query.range as RangeId | undefined) ?? "week";

  // Read once per mount and held, so the marker does not move under the reader
  // as the screen refreshes. Writing it back is the last thing this screen does.
  const [lastSeen] = useState(readLastSeen);
  const [open, setOpen] = useState<Record<string, boolean>>({});

  const { data, error, loading, reload } = useAsync<ChangedResponse>(
    () =>
      api.changed({
        project,
        actor,
        since: sinceFor(range),
        limit: 500,
      }),
    [project, generation, actor, range],
  );

  // Written after the data arrives, not on mount: marking everything seen before
  // it is on screen would lose the mark for a reader whose daemon was down.
  useEffect(() => {
    if (!data) return;
    try {
      window.localStorage.setItem(SEEN_KEY, new Date().toISOString());
    } catch {
      // See readLastSeen.
    }
  }, [data]);

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="What changed" crumbs={project ? projectCrumbs(route, "What changed") : undefined}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const sessions = data?.sessions ?? [];
  const isNew = (session: SessionRow) => Boolean(lastSeen && session.ended_at > lastSeen);
  const newCount = sessions.filter(isNew).length;

  return (
    <Page
      title="What changed"
      crumbs={project ? projectCrumbs(route, "What changed") : undefined}
      meta={
        <span className="text-small text-ink-faint">
          {sessions.length} session{sessions.length === 1 ? "" : "s"} ·{" "}
          {data?.changes ?? 0} change{data?.changes === 1 ? "" : "s"}
          {newCount > 0 ? ` · ${newCount} new since you were last here` : ""}
        </span>
      }
      toolbar={
        <>
          {(Object.keys(RANGES) as RangeId[]).map((id) => (
            <Chip
              key={id}
              selected={range === id}
              onClick={() => setQuery(route, { range: id === "week" ? undefined : id })}
            >
              {RANGES[id]}
            </Chip>
          ))}
          <span className="mx-1 text-border-subtle" aria-hidden>
            |
          </span>
          {ACTORS.map((a) => (
            <Chip
              key={a}
              selected={actor === a}
              onClick={() =>
                setQuery(route, { actor: actor === a ? undefined : a }, { replace: true })
              }
            >
              {a}
            </Chip>
          ))}
        </>
      }
    >
      {sessions.length === 0 ? (
        <Empty
          message="Nothing changed in this window."
          hint={range === "all" ? undefined : "Try Everything, or a different actor."}
        />
      ) : (
        <ol className="space-y-1.5">
          {sessions.map((session) => {
            const key = session.session_id ?? "untracked";
            const expanded = open[key] ?? false;
            return (
              <li
                key={key}
                className="rounded-card border border-border-subtle bg-surface-raised"
              >
                <button
                  type="button"
                  aria-expanded={expanded}
                  onClick={() => setOpen((o) => ({ ...o, [key]: !expanded }))}
                  className="flex w-full items-baseline gap-2 px-3 py-2.5 text-left hover:bg-surface-hover"
                >
                  <span className="w-3 shrink-0 font-mono text-micro text-ink-faint" aria-hidden>
                    {expanded ? "−" : "+"}
                  </span>
                  <Badge
                    tone={
                      session.actor === "claude"
                        ? "border-accent/40 text-accent bg-accent/10"
                        : session.actor === "human"
                          ? "border-good/40 text-good bg-good/10"
                          : undefined
                    }
                  >
                    {session.actor}
                  </Badge>
                  <span className="min-w-0 flex-1 truncate text-small">{session.headline}</span>
                  {isNew(session) && (
                    <Tooltip align="right" text="This landed since you last opened this screen">
                      <span className="rounded bg-brand/15 px-1.5 py-0.5 text-micro text-brand">
                        new
                      </span>
                    </Tooltip>
                  )}
                  <span className="shrink-0 text-micro text-ink-faint">
                    <When iso={session.ended_at} />
                  </span>
                </button>

                {expanded && (
                  <ul className="border-t border-border-subtle px-3 py-1.5">
                    {session.changes.map((change) => {
                      const to = destination(change, project);
                      return (
                        <li key={change.id} className="flex items-baseline gap-2 py-1">
                          <span className="w-14 shrink-0 text-right text-micro tabular-nums text-ink-faint">
                            {when(change.at)}
                          </span>
                          {change.kind === "note" ? (
                            <Tooltip align="left" text="A note. Notes leave no event, so this half of the story is invisible to the event feed">
                              <span className="shrink-0 text-micro text-brand">note</span>
                            </Tooltip>
                          ) : (
                            <span className="w-[2.6rem] shrink-0 text-micro text-ink-faint">
                              {change.kind === "created" ? "new" : ""}
                            </span>
                          )}
                          {to ? (
                            <a href={to} className="min-w-0 flex-1 truncate text-small hover:text-accent">
                              {change.reference ? (
                                <span className="mr-1.5 font-mono text-micro text-ink-faint">
                                  {change.reference}
                                </span>
                              ) : null}
                              {change.summary}
                            </a>
                          ) : (
                            <span className="min-w-0 flex-1 truncate text-small">
                              {change.summary}
                            </span>
                          )}
                        </li>
                      );
                    })}
                  </ul>
                )}

                {expanded && (
                  <div className="border-t border-border-subtle px-3 py-1.5">
                    {session.session_id ? (
                      <Tooltip align="left" text="The conversation that made these changes">
                        <span className="font-mono text-micro text-ink-faint">
                          {session.session_id}
                        </span>
                      </Tooltip>
                    ) : (
                      <Tooltip
                        align="left"
                        text="These changes did not say which conversation they came from — a migration, a bootstrap or a direct call. That is allowed, but if it becomes common, attribution has stopped working."
                      >
                        <span className="text-micro text-ink-faint">no session recorded</span>
                      </Tooltip>
                    )}
                  </div>
                )}
              </li>
            );
          })}
        </ol>
      )}

      {data?.truncated && (
        <p className="mt-3 text-small text-ink-faint">
          Showing the most recent {data.changes} changes. Older ones exist — narrow the range
          or the actor to see further back.
        </p>
      )}
    </Page>
  );
}

/**
 * Where a change leads.
 *
 * The same map Search uses, for the same reason: five of the thirteen types have
 * a page of their own, and the rest are rendered only as part of a project, so
 * that is where they lead. Landing on the project is a worse answer than landing
 * on the row and a much better one than landing nowhere, which is what every row
 * on this screen used to do.
 */
function destination(change: ChangeRow, project: string | undefined): string | undefined {
  // A change carries no project of its own — this screen groups by session, not
  // by project — so the address in the bar is what scopes it. On the
  // all-projects view there is nothing to scope with, and a link to the wrong
  // project is worse than no link.
  if (!project) return undefined;
  switch (change.entity_type) {
    case "task":
      return href({ screen: "task", project, taskId: change.reference || change.entity_id });
    case "spec":
    case "decision":
    case "question":
    case "feedback":
    case "design":
      return href({ screen: "documents", project, documentId: change.entity_id });
    case "milestone":
      return href({ screen: "roadmap", project });
    default:
      return href({ screen: "project", project });
  }
}
