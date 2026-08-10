/**
 * Screen 9 — Activity. The event feed, filterable by actor.
 *
 * REQ-10 lists this as v1. Its purpose is "what did Claude do today": the
 * store is written to by an agent far more often than by a human, and being
 * able to see that at a glance is what makes the arrangement trustworthy.
 */

import { useMemo } from "react";
import { api, type EventRow } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Id, Spinner, Tooltip, when } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { setQuery } from "../lib/router";
import type { ScreenProps } from "../App";

const ACTORS = ["human", "claude", "github", "system"] as const;

export function ActivityScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const actor = route.query.actor;

  const { data, error, loading, reload } = useAsync(
    () => api.activity({ project, limit: 300 }),
    [project, generation],
  );

  const events = useMemo(() => {
    const all = (data?.events ?? []).slice().reverse();
    return actor ? all.filter((e) => e.actor === actor) : all;
  }, [data, actor]);

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="Activity" crumbs={project ? projectCrumbs(route, "Activity") : undefined}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  return (
    <Page
      title="Activity"
      crumbs={project ? projectCrumbs(route, "Activity") : undefined}
      meta={
        <span className="text-small text-ink-faint">
          {events.length}
          {data?.truncated ? ` of ${data.total}` : ""}
        </span>
      }
      toolbar={ACTORS.map((a) => (
        <Chip
          key={a}
          selected={actor === a}
          onClick={() => setQuery(route, { actor: actor === a ? undefined : a }, { replace: true })}
        >
          {a}
        </Chip>
      ))}
    >
      <div className="space-y-4">
        {events.length === 0 ? (
          <Empty message="Nothing has changed." />
        ) : (
          <ul className="space-y-1">
            {events.map((e: EventRow) => (
              <li
                key={e.id}
                className="flex items-start gap-3 rounded border border-transparent px-2 py-1.5 hover:border-border-subtle hover:bg-surface-raised"
              >
                <span className="w-16 shrink-0 pt-0.5 text-right text-micro tabular-nums text-ink-faint">
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
                  <p className="selectable truncate text-small" title={e.summary}>
                    {e.summary}
                  </p>
                  <div className="mt-0.5 flex items-center gap-2">
                    <Id value={e.entity_id} />
                    {e.session_id ? (
                      <Tooltip align="left" text="The conversation that made this change">
                        <span className="font-mono text-micro text-ink-faint">{e.session_id}</span>
                      </Tooltip>
                    ) : (
                      <Tooltip
                        align="left"
                        text="No session id. Attribution is cooperative (D-10), so this is legal — but a rising count here means the skill has stopped threading it."
                      >
                        <span className="text-micro text-warn">unattributed</span>
                      </Tooltip>
                    )}
                  </div>
                </div>
              </li>
            ))}
          </ul>
        )}

        {data?.truncated && (
          <p className="text-small text-ink-faint">
            Showing the most recent {events.length} of {data.total} changes.
          </p>
        )}
      </div>
    </Page>
  );
}
