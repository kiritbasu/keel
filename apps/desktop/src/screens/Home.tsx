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
import type { ScreenProps } from "../App";

export function HomeScreen({ project: _project, generation, openProject }: ScreenProps) {
  const { data, error, loading, reload } = useAsync<Digest>(() => api.context(), [generation]);

  if (loading && !data) return <Spinner label="Reading every project…" />;
  if (error) {
    return (
      <div className="p-6">
        <ErrorBox error={error} retry={reload} />
      </div>
    );
  }
  if (!data) return null;

  const projects = data.projects ?? [];
  const shipped = (data.recent ?? []).filter(
    (line) => line.includes("shipped") || line.includes("→ done"),
  );

  return (
    <div className="mx-auto max-w-6xl space-y-5 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-xl font-semibold tracking-tight">All projects</h1>
        <span className="text-[12px] text-ink-faint">
          {projects.length} project{projects.length === 1 ? "" : "s"}
        </span>
      </header>

      {projects.length === 0 ? (
        <Empty
          message="Nothing here yet."
          hint="Talk to Claude about a project and it will appear. Or run `keel fixture` against a scratch store to see what it looks like full."
        />
      ) : (
        <div className="grid gap-3">
          {projects.map((p) => {
            const atRisk = p.blocked_tasks > 0 || p.urgent_tasks > 0;
            return (
              <button
                key={p.id}
                onClick={() => openProject(p.slug)}
                className={cx(
                  "flex items-center gap-6 rounded-lg border bg-surface-raised px-5 py-4 text-left transition-colors hover:bg-surface-hover",
                  atRisk ? "border-warn/30" : "border-border-subtle",
                )}
              >
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="truncate text-[15px] font-medium">{p.name}</span>
                    <span className="font-mono text-[11px] text-ink-faint">{p.slug}</span>
                  </div>
                  <div className="mt-1 truncate text-[12px] text-ink-muted">
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
              </button>
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
                <li key={q.id} className="flex items-start gap-2 text-[13px]">
                  <span className="mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full bg-warn" />
                  <span className="selectable">{q.label}</span>
                  {q.detail && (
                    <span className="ml-auto shrink-0 text-[11px] text-ink-faint">{q.detail}</span>
                  )}
                </li>
              ))}
            </ul>
          )}
          {/* Never truncated, by design (SPEC §6.3) — so no truncation note. */}
        </Card>

        <Card title="Recently">
          {shipped.length === 0 && data.recent.length === 0 ? (
            <Empty message="No activity yet." />
          ) : (
            <ul className="space-y-1.5 text-[13px] text-ink-muted">
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
          <ul className="space-y-1.5 text-[13px] text-ink-muted">
            {data.next.map((line, i) => (
              <li key={i}>{line}</li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
