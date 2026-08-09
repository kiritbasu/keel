/**
 * Screen 2 — Project dashboard.
 *
 * The digest, rendered. Deliberately the same data `keel_context` gives an
 * agent: if a human and a model are looking at different summaries of the same
 * project, one of them is wrong and nobody knows which.
 */

import { api, type Digest } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Card, Empty, ErrorBox, Id, Spinner, Stat, statusTone } from "../components/ui";
import type { ScreenProps } from "../App";

export function ProjectScreen({ project, generation, setScreen }: ScreenProps) {
  const { data, error, loading, reload } = useAsync<Digest>(
    () => api.context(project),
    [project, generation],
  );

  if (!project) return <Empty message="Pick a project." />;
  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <div className="p-6">
        <ErrorBox error={error} retry={reload} />
      </div>
    );
  }
  if (!data?.project) return <Empty message="Project not found." />;

  const p = data.project;

  return (
    <div className="mx-auto max-w-6xl space-y-5 p-6">
      <header>
        <div className="flex items-baseline gap-3">
          <h1 className="text-xl font-semibold tracking-tight">{p.name}</h1>
          <Badge tone={statusTone(p.status)}>{p.status}</Badge>
        </div>
        {p.active_milestone && (
          <p className="mt-1 text-[13px] text-ink-muted">Active: {p.active_milestone}</p>
        )}
      </header>

      <div className="flex gap-8 rounded-lg border border-border-subtle bg-surface-raised px-5 py-4">
        <Stat value={p.open_tasks} label="open" />
        <Stat value={p.urgent_tasks} label="urgent" tone={p.urgent_tasks ? "text-warn" : undefined} />
        <Stat value={p.blocked_tasks} label="blocked" tone={p.blocked_tasks ? "text-bad" : undefined} />
        <Stat value={p.open_questions} label="questions" />
        <div className="ml-auto self-end text-[11px] text-ink-faint">
          digest ≈ {data.estimated_tokens.toLocaleString()} tokens
          {data.budget_exceeded && (
            <span className="ml-2 text-warn" title="Questions and glossary are never trimmed, so the digest exceeded its budget. That usually means the question register needs pruning.">
              over budget
            </span>
          )}
        </div>
      </div>

      <div className="grid gap-5 lg:grid-cols-2">
        <Card
          title="Needs attention"
          actions={
            <button onClick={() => setScreen("board")} className="text-[12px] text-accent hover:underline">
              board →
            </button>
          }
        >
          {data.attention.length === 0 ? (
            <Empty message="Nothing urgent or blocked." />
          ) : (
            <ul className="space-y-2">
              {data.attention.map((t) => (
                <li key={t.id} className="flex items-center gap-2 text-[13px]">
                  <Badge tone={statusTone(t.status)}>{t.status}</Badge>
                  <span className="selectable truncate">{t.label}</span>
                  {t.detail && <span className="ml-auto text-[11px] text-ink-faint">{t.detail}</span>}
                </li>
              ))}
            </ul>
          )}
          {data.truncated
            .filter((t) => t.section === "attention")
            .map((t) => (
              <p key={t.section} className="mt-3 text-[12px] text-ink-faint">
                Showing {t.shown} of {t.total}.
              </p>
            ))}
        </Card>

        <Card title="Open questions and risks">
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
        </Card>

        <Card title="Recent decisions">
          {data.decisions.length === 0 ? (
            <Empty message="None accepted yet." />
          ) : (
            <ul className="space-y-2 text-[13px]">
              {data.decisions.map((d) => (
                <li key={d.id} className="selectable">
                  {d.label}
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card
          title="Specs"
          actions={
            <button onClick={() => setScreen("documents")} className="text-[12px] text-accent hover:underline">
              read →
            </button>
          }
        >
          {data.specs.length === 0 ? (
            <Empty message="No specs yet." />
          ) : (
            <ul className="space-y-2 text-[13px]">
              {data.specs.map((s) => (
                <li key={s.id} className="flex items-center gap-2">
                  <Badge tone={statusTone(s.status)}>{s.status}</Badge>
                  <span className="selectable truncate">{s.label}</span>
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card title="What is live">
          {data.environments.length === 0 ? (
            <Empty message="No environments recorded." />
          ) : (
            <ul className="space-y-2 text-[13px]">
              {data.environments.map((e) => (
                <li key={e.id} className="flex items-center gap-2">
                  <Badge tone={statusTone(e.status)}>{e.status}</Badge>
                  <span>{e.label}</span>
                  {e.detail && <Id value={e.detail} />}
                </li>
              ))}
            </ul>
          )}
        </Card>

        <Card title="Glossary">
          {data.terms.length === 0 ? (
            <Empty message="No terms yet." hint="Terms are cheap to add and stop the next session guessing." />
          ) : (
            <dl className="space-y-2 text-[13px]">
              {data.terms.map((t) => (
                <div key={t.term} className="selectable">
                  <dt className="inline font-medium">{t.term}</dt>
                  {t.global && <span className="ml-1 text-[11px] text-ink-faint">(global)</span>}
                  <dd className="inline text-ink-muted"> — {t.definition}</dd>
                </div>
              ))}
            </dl>
          )}
        </Card>
      </div>

      {data.next_up && (
        <Card title="Next">
          {data.next_up.ready.length > 0 ? (
            <ol className="space-y-2">
              {data.next_up.ready.map((item, i) => (
                <li key={item.id} className="flex gap-2.5">
                  <span className="mt-0.5 w-4 shrink-0 text-right text-[12px] tabular-nums text-ink-faint">
                    {i + 1}
                  </span>
                  <div className="min-w-0">
                    <div className="text-[13px]">{item.title}</div>
                    <div className="mt-0.5 text-[12px] text-ink-faint">{item.why}</div>
                  </div>
                </li>
              ))}
            </ol>
          ) : (
            <Empty
              message="Nothing is ready to pick up."
              hint="Everything open is blocked or waiting on a decision — unblocking one is the work."
            />
          )}

          {data.next_up.waiting_on_you.length > 0 && (
            <div className="mt-4 border-t border-border-subtle pt-3">
              <h3 className="mb-1.5 text-[12px] font-semibold tracking-wide text-ink-muted uppercase">
                Waiting on you
              </h3>
              <ul className="space-y-1 text-[13px] text-ink-muted">
                {data.next_up.waiting_on_you.map((item) => (
                  <li key={item.id}>{item.title}</li>
                ))}
              </ul>
            </div>
          )}

          {data.next_up.blocked.length > 0 && (
            <div className="mt-4 border-t border-border-subtle pt-3">
              <h3 className="mb-1.5 text-[12px] font-semibold tracking-wide text-ink-muted uppercase">
                Blocked
              </h3>
              <ul className="space-y-1.5 text-[13px]">
                {data.next_up.blocked.map((item) => (
                  <li key={item.id}>
                    <div className="text-ink-muted">{item.title}</div>
                    <div className="text-[12px] text-ink-faint">{item.why}</div>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </Card>
      )}

      {data.next.length > 0 && (
        <Card title="Also worth noticing">
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
