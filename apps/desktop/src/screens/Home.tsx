/**
 * Screen 1 — Home. Every project at a glance.
 *
 * This is UC-6, the Sunday review: "what shipped this week, what's at risk,
 * what's blocked, which questions have been open longest". Its exit criterion
 * is that the whole thing is absorbable in under thirty seconds, which is a
 * constraint on *layout* rather than on the query — so at-risk projects sort to
 * the top, and nothing here needs a click to reveal a number.
 */

import { api, type Digest } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Card, Empty, ErrorBox, Spinner, Stat, cx } from "../components/ui";
import { FirstRun } from "../components/FirstRun";
import { Page } from "../components/Page";
import { href } from "../lib/router";
import type { ScreenProps } from "../App";

export function HomeScreen({ generation }: ScreenProps) {
  const { data, error, loading, reload } = useAsync<Digest>(
    () => api.context(),
    [generation],
  );
  // Only read for the first run, and deliberately not awaited: a slow or failed
  // health call must not hold up or break the roll-up, which is the screen's
  // actual job. `FirstRun` renders without any of these fields.
  const { data: health } = useAsync(() => api.health(), [generation]);

  if (loading && !data) return <Spinner label="Reading every project…" />;
  if (error) {
    return (
      <Page title="All projects">
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }
  if (!data) return null;

  const projects = data.projects ?? [];

  // A store with no projects in it is a new install, not an empty list, and the
  // two want different screens. The roll-up below is a scanning surface — it
  // assumes you know what Keel is and want the state of it. Somebody who has
  // just installed it wants neither, and giving them three "nothing yet"
  // messages inside two bordered panels answered no question they had.
  //
  // The whole roll-up is suppressed rather than shown empty, because empty
  // panels are the largest shapes on the page and hold the least.
  if (projects.length === 0) {
    return (
      <Page title="All projects" width="wide">
        <FirstRun
          version={health?.version}
          schema={health?.schema}
          home={health?.home}
        />
      </Page>
    );
  }

  return (
    <Page
      title="All projects"
      width="wide"
      meta={
        <span className="text-small text-ink-faint">
          {projects.length} project{projects.length === 1 ? "" : "s"}
        </span>
      }
    >
      <div className="space-y-5">
        {/* No empty branch here: zero projects returned `FirstRun` above, so
            this list always has something in it. */}
        <div className="grid gap-3">
          {projects.map((p) => {
            const atRisk = p.blocked_tasks > 0 || p.urgent_tasks > 0;
            return (
              <a
                key={p.id}
                href={href({ screen: "project", project: p.slug })}
                className={cx(
                  "flex items-center gap-6 rounded-lg border bg-surface-raised px-5 py-4 text-left transition-colors hover:bg-surface-hover",
                  atRisk ? "border-warn/30" : "border-border-subtle",
                )}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-heading font-medium">
                      {p.name}
                    </span>
                    <span className="font-mono text-micro text-ink-faint">
                      {p.slug}
                    </span>
                  </div>
                  <div className="mt-1 truncate text-small text-ink-muted">
                    {p.active_milestone
                      ? `→ ${p.active_milestone}`
                      : `status: ${p.status}`}
                  </div>
                </div>

                <Stat value={p.open_tasks} label="open" />
                <Stat
                  value={p.urgent_tasks}
                  label="urgent"
                  tone={p.urgent_tasks > 0 ? "text-warn" : "text-ink-faint"}
                />
                <Stat
                  value={p.blocked_tasks}
                  label="blocked"
                  tone={p.blocked_tasks > 0 ? "text-bad" : "text-ink-faint"}
                />
                <Stat
                  value={p.open_questions}
                  label="questions"
                  tone={p.open_questions > 0 ? "text-ink" : "text-ink-faint"}
                />
              </a>
            );
          })}
        </div>

        <div className="grid gap-5 lg:grid-cols-2">
          <Card title="Open questions, everywhere">
            {data.questions.length === 0 ? (
              <Empty message="Nothing unresolved." />
            ) : (
              <ul className="space-y-2">
                {data.questions.map((q) => (
                  <li key={q.id} className="flex items-start gap-2 text-small">
                    <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-warn" />
                    <span className="selectable">{q.label}</span>
                    {q.detail && (
                      <span className="ml-auto shrink-0 text-micro text-ink-faint">
                        {q.detail}
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            )}
            {/* Never truncated, by design (SPEC §6.3) — so no truncation note. */}
          </Card>

          <Card title="Recently">
            {data.recent.length === 0 ? (
              <Empty message="No activity yet." />
            ) : (
              <ul className="space-y-1.5 text-small text-ink-muted">
                {data.recent.slice(0, 12).map((line, i) => (
                  <li key={i} className="selectable truncate" title={line}>
                    {line}
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </div>

        {data.next.length > 0 && (
          <Card title="Suggested next">
            <ul className="space-y-1.5 text-small text-ink-muted">
              {data.next.map((line, i) => (
                <li key={i}>{line}</li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    </Page>
  );
}
