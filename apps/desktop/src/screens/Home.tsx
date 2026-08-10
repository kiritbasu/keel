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
import { Page } from "../components/Page";
import { href } from "../lib/router";
import type { ScreenProps } from "../App";

export function HomeScreen({ generation }: ScreenProps) {
  const { data, error, loading, reload } = useAsync<Digest>(() => api.context(), [generation]);

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
        {projects.length === 0 ? (
          <Empty
            message="Nothing here yet."
            hint="Talk to Claude about a project and it will appear here."
          />
        ) : (
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
                      <span className="truncate text-heading font-medium">{p.name}</span>
                      <span className="font-mono text-micro text-ink-faint">{p.slug}</span>
                    </div>
                    <div className="mt-1 truncate text-small text-ink-muted">
                      {p.active_milestone ? `→ ${p.active_milestone}` : `status: ${p.status}`}
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
        )}

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
                      <span className="ml-auto shrink-0 text-micro text-ink-faint">{q.detail}</span>
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
