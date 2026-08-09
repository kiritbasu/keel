/**
 * Screen 9 — Activity. The event feed, filterable by actor.
 *
 * REQ-10 lists this as v1. Its purpose is "what did Claude do today": the
 * store is written to by an agent far more often than by a human, and being
 * able to see that at a glance is what makes the arrangement trustworthy.
 */

import { useMemo, useState } from "react";
import { api, type EventRow } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Id, Spinner, cx, when } from "../components/ui";
import type { ScreenProps } from "../App";

const ACTORS = ["human", "claude", "github", "system"] as const;

export function ActivityScreen({ project, generation }: ScreenProps) {
  const [actor, setActor] = useState<string | null>(null);
  const [scoped, setScoped] = useState(Boolean(project));

  const { data, error, loading, reload } = useAsync(
    () => api.activity({ project: scoped ? project : undefined, limit: 300 }),
    [project, scoped, generation],
  );

  const events = useMemo(() => {
    const all = (data?.events ?? []).slice().reverse();
    return actor ? all.filter((e) => e.actor === actor) : all;
  }, [data, actor]);

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <div className="p-6">
        <ErrorBox error={error} retry={reload} />
      </div>
    );
  }

  return (
    <div className="mx-auto max-w-4xl space-y-4 p-6">
      <header className="flex flex-wrap items-center gap-3">
        <h1 className="text-xl font-semibold tracking-tight">Activity</h1>
        {project && (
          <button
            onClick={() => setScoped((v) => !v)}
            className={cx(
              "rounded border px-2 py-1 text-[12px]",
              scoped
                ? "border-accent/50 bg-accent/10 text-accent"
                : "border-border-subtle text-ink-muted hover:bg-surface-hover",
            )}
          >
            {project} only
          </button>
        )}
        {ACTORS.map((a) => (
          <button
            key={a}
            onClick={() => setActor((v) => (v === a ? null : a))}
            className={cx(
              "rounded border px-2 py-1 text-[12px]",
              actor === a
                ? "border-accent/50 bg-accent/10 text-accent"
                : "border-border-subtle text-ink-faint hover:bg-surface-hover",
            )}
          >
            {a}
          </button>
        ))}
        <span className="ml-auto text-[12px] text-ink-faint">
          {events.length}
          {data?.truncated ? ` of ${data.total}` : ""}
        </span>
      </header>

      {events.length === 0 ? (
        <Empty message="Nothing has changed." />
      ) : (
        <ul className="space-y-1">
          {events.map((e: EventRow) => (
            <li
              key={e.id}
              className="flex items-start gap-3 rounded border border-transparent px-2 py-1.5 hover:border-border-subtle hover:bg-surface-raised"
            >
              <span className="w-16 shrink-0 pt-0.5 text-right text-[11px] text-ink-faint tabular-nums">
                {when(e.created_at)}
              </span>
              <Badge
                tone={
                  e.actor === "claude"
                    ? "border-accent/40 text-accent bg-accent/10"
                    : e.actor === "human"
                      ? "border-good/40 text-good bg-good/10"
                      : undefined
                }
              >
                {e.actor}
              </Badge>
              <div className="min-w-0 flex-1">
                <p className="selectable truncate text-[13px]" title={e.summary}>
                  {e.summary}
                </p>
                <div className="mt-0.5 flex items-center gap-2">
                  <Id value={e.entity_id} />
                  {e.session_id ? (
                    <span className="font-mono text-[10px] text-ink-faint" title="The conversation that made this change">
                      {e.session_id}
                    </span>
                  ) : (
                    <span
                      className="text-[10px] text-warn"
                      title="No session id. Attribution is cooperative (D-10), so this is legal — but a rising count here means the skill has stopped threading it."
                    >
                      unattributed
                    </span>
                  )}
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}

      {data?.truncated && (
        <p className="text-[12px] text-ink-faint">
          Showing the most recent {events.length} of {data.total} changes.
        </p>
      )}
    </div>
  );
}
