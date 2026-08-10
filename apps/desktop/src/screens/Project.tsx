/**
 * Screen 2 — Project dashboard.
 *
 * The digest, rendered. Deliberately the same data `keel_context` gives an
 * agent: if a human and a model are looking at different summaries of the same
 * project, one of them is wrong and nobody knows which.
 */

import { api, type Digest } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Card, Empty, ErrorBox, Id, Spinner, Stat, Tooltip, statusTone } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { href } from "../lib/router";
import type { ScreenProps } from "../App";

export function ProjectScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const { data, error, loading, reload } = useAsync<Digest>(
    () => api.context(project),
    [project, generation],
  );

  if (!project) return <Empty message="Pick a project." />;
  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title={project} crumbs={projectCrumbs(route)}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }
  if (!data?.project) return <Empty message="Project not found." />;

  const p = data.project;

  return (
    <Page
      title={p.name}
      crumbs={projectCrumbs(route)}
      width="wide"
      meta={<Badge tone={statusTone(p.status)}>{p.status}</Badge>}
      actions={
        p.active_milestone ? (
          <span className="text-small text-ink-muted">Active: {p.active_milestone}</span>
        ) : undefined
      }
    >
      <div className="space-y-5">
        <div className="flex gap-8 rounded-lg border border-border-subtle bg-surface-raised px-5 py-4">
          <Stat value={p.open_tasks} label="open" />
          <Stat value={p.urgent_tasks} label="urgent" tone={p.urgent_tasks ? "text-warn" : undefined} />
          <Stat value={p.blocked_tasks} label="blocked" tone={p.blocked_tasks ? "text-bad" : undefined} />
          <Stat value={p.open_questions} label="questions" />
          <div className="ml-auto self-end text-micro text-ink-faint">
            digest ≈ {data.estimated_tokens.toLocaleString()} tokens
            {data.budget_exceeded && (
              <Tooltip
                align="right"
                text="Questions and glossary are never trimmed, so the digest exceeded its budget. That usually means the question register needs pruning."
              >
                <span className="ml-2 text-warn">over budget</span>
              </Tooltip>
            )}
          </div>
        </div>

        <div className="grid gap-5 lg:grid-cols-2">
          <Card
            title="Needs attention"
            actions={
              <a
                href={href({ screen: "board", project })}
                className="text-small text-accent hover:underline"
              >
                board →
              </a>
            }
          >
            {data.attention.length === 0 ? (
              <Empty message="Nothing urgent or blocked." />
            ) : (
              <ul className="space-y-2">
                {data.attention.map((t) => (
                  <li key={t.id} className="flex items-center gap-2 text-small">
                    <Badge tone={statusTone(t.status)}>{t.status}</Badge>
                    <span className="selectable truncate">{t.label}</span>
                    {t.detail && <span className="ml-auto text-micro text-ink-faint">{t.detail}</span>}
                  </li>
                ))}
              </ul>
            )}
            {data.truncated
              .filter((t) => t.section === "attention")
              .map((t) => (
                <p key={t.section} className="mt-3 text-small text-ink-faint">
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
          </Card>

          <Card title="Recent decisions">
            {data.decisions.length === 0 ? (
              <Empty message="None accepted yet." />
            ) : (
              <ul className="space-y-2 text-small">
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
              <a
                href={href({ screen: "documents", project })}
                className="text-small text-accent hover:underline"
              >
                read →
              </a>
            }
          >
            {data.specs.length === 0 ? (
              <Empty message="No specs yet." />
            ) : (
              <ul className="space-y-2 text-small">
                {data.specs.map((s) => (
                  <li key={s.id}>
                    <a
                      href={href({ screen: "documents", project, documentId: s.id })}
                      className="flex items-center gap-2 hover:underline"
                    >
                      <Badge tone={statusTone(s.status)}>{s.status}</Badge>
                      <span className="truncate">{s.label}</span>
                    </a>
                  </li>
                ))}
              </ul>
            )}
          </Card>

          <Card title="What is live">
            {data.environments.length === 0 ? (
              <Empty message="No environments recorded." />
            ) : (
              <ul className="space-y-2 text-small">
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
              <Empty
                message="No terms yet."
                hint="Terms are cheap to add and stop the next session guessing."
              />
            ) : (
              <dl className="space-y-2 text-small">
                {data.terms.map((t) => (
                  <div key={t.term} className="selectable">
                    <dt className="inline font-medium">{t.term}</dt>
                    {t.global && <span className="ml-1 text-micro text-ink-faint">(global)</span>}
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
                    <span className="mt-0.5 w-4 shrink-0 text-right text-small tabular-nums text-ink-faint">
                      {i + 1}
                    </span>
                    <div className="min-w-0">
                      <div className="text-small">{item.title}</div>
                      <div className="mt-0.5 text-small text-ink-faint">{item.why}</div>
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
                <h3 className="mb-1.5 text-small font-semibold tracking-wide text-ink-muted uppercase">
                  Waiting on you
                </h3>
                <ul className="space-y-1 text-small text-ink-muted">
                  {data.next_up.waiting_on_you.map((item) => (
                    <li key={item.id}>{item.title}</li>
                  ))}
                </ul>
              </div>
            )}

            {data.next_up.blocked.length > 0 && (
              <div className="mt-4 border-t border-border-subtle pt-3">
                <h3 className="mb-1.5 text-small font-semibold tracking-wide text-ink-muted uppercase">
                  Blocked
                </h3>
                <ul className="space-y-1.5 text-small">
                  {data.next_up.blocked.map((item) => (
                    <li key={item.id}>
                      <div className="text-ink-muted">{item.title}</div>
                      <div className="text-small text-ink-faint">{item.why}</div>
                    </li>
                  ))}
                </ul>
              </div>
            )}
          </Card>
        )}

        {data.next.length > 0 && (
          <Card title="Also worth noticing">
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
