/**
 * Screen 4 — Board. Tasks by status, filterable, keyboard-driven.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Digest, type Entity, type Note, type Page as PageOf } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Id, Spinner, cx, priorityTone } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

/** Lifecycle order, left to right. Matches TaskStatus::ALL. */
const COLUMNS = ["todo", "in_progress", "blocked", "review", "done", "wont_do"] as const;

export function BoardScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const [urgentOnly, setUrgentOnly] = useState(false);
  const [label, setLabel] = useState<string | null>(null);
  // Which cards are expanded. A note is a paragraph and a column is 240px, so
  // showing every stream at once would bury the board it is attached to.
  const [open, setOpen] = useState<Set<string>>(new Set());

  // The command palette can name a task in the address. Until a task has a page
  // of its own, this is how "jump to that task" lands somewhere real: the card
  // is highlighted and scrolled to.
  const focused = route.query.task;
  const focusedCard = useRef<HTMLElement>(null);

  const { data, error, loading, reload } = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "task", limit: 2000 }),
    [project, generation],
  );

  // The same ranking the digest gives an agent, rather than a second opinion
  // computed in the browser. A board that disagrees with what Claude was told
  // is worse than a board with no ordering at all.
  const digest = useAsync<Digest>(() => api.context(project), [project, generation]);

  // Every stream in one request. Seventy cards asking individually is seventy
  // round trips to render a count.
  const notes = useAsync<{ notes: Note[]; total: number }>(
    () => api.notes(project),
    [project, generation],
  );
  const notesByTask = useMemo(() => {
    const m = new Map<string, Note[]>();
    for (const n of notes.data?.notes ?? []) {
      const list = m.get(n.entity_id);
      if (list) list.push(n);
      else m.set(n.entity_id, [n]);
    }
    return m;
  }, [notes.data]);

  const ranked = digest.data?.next_up ?? null;
  const rank = useMemo(() => {
    const m = new Map<string, { position: number; why: string }>();
    ranked?.ready.forEach((item, i) => m.set(item.id, { position: i + 1, why: item.why }));
    return m;
  }, [ranked]);

  const tasks = useMemo(() => {
    let items = data?.items ?? [];
    if (urgentOnly) items = items.filter((t) => ["p0", "p1"].includes(String(t.priority)));
    if (label) items = items.filter((t) => (t.labels as string[] | undefined)?.includes(label));
    return items;
  }, [data, urgentOnly, label]);

  const labels = useMemo(() => {
    const seen = new Set<string>();
    for (const t of data?.items ?? []) {
      for (const l of (t.labels as string[] | undefined) ?? []) seen.add(l);
    }
    return [...seen].sort();
  }, [data]);

  useEffect(() => {
    focusedCard.current?.scrollIntoView({ block: "center" });
  }, [focused, tasks]);

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="Board" crumbs={projectCrumbs(route, "Board")}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  return (
    <Page
      title="Board"
      crumbs={projectCrumbs(route, "Board")}
      width="full"
      meta={
        <span className="text-small text-ink-faint">
          {tasks.length} of {data?.total ?? 0}
        </span>
      }
      toolbar={
        <>
          <Chip selected={urgentOnly} onClick={() => setUrgentOnly((v) => !v)}>
            urgent only
          </Chip>
          {labels.map((l) => (
            <Chip key={l} selected={label === l} onClick={() => setLabel((v) => (v === l ? null : l))}>
              {l}
            </Chip>
          ))}
        </>
      }
    >
      <div className="flex h-full flex-col p-6">
        {ranked && ranked.ready.length > 0 && !label && !urgentOnly && (
          <section className="mb-4 shrink-0 rounded-lg border border-accent/30 bg-accent/5 px-3 py-2.5">
            <h2 className="mb-1.5 text-micro font-semibold tracking-wide text-accent uppercase">Next</h2>
            <ol className="space-y-1">
              {ranked.ready.map((item, i) => (
                <li key={item.id} className="flex gap-2 text-small">
                  <span className="w-3 shrink-0 text-right tabular-nums text-ink-faint">{i + 1}</span>
                  <span className="min-w-0">
                    {item.title} <span className="text-ink-faint">— {item.why}</span>
                  </span>
                </li>
              ))}
            </ol>
          </section>
        )}

        {tasks.length === 0 ? (
          <Empty
            message="No tasks match."
            hint={label || urgentOnly ? "Clear the filters above." : undefined}
          />
        ) : (
          // Flex with fixed-width columns and horizontal scroll, not a grid.
          // A six-column grid with a min-width per column resolves by overflowing
          // its tracks rather than scrolling, which puts each column's cards on
          // top of the next column's heading.
          <div className="flex min-h-0 flex-1 gap-3 overflow-x-auto pb-2">
            {COLUMNS.map((column) => {
              // Ranked work sorts to the top, in rank order. A column that
              // displays "3" above "1" is showing a ranking and contradicting
              // it in the same breath.
              const inColumn = tasks
                .filter((t) => String(t.status) === column)
                .sort((a, b) => {
                  const ra = rank.get(String(a.id))?.position ?? Infinity;
                  const rb = rank.get(String(b.id))?.position ?? Infinity;
                  if (ra !== rb) return ra - rb;
                  return String(a.priority).localeCompare(String(b.priority));
                });
              return (
                <div key={column} className="flex w-[240px] shrink-0 flex-col">
                  <div className="mb-2 flex items-baseline justify-between gap-2 px-1">
                    <span className="text-micro font-medium tracking-wide text-ink-muted uppercase">
                      {column.replace("_", " ")}
                    </span>
                    <span className="text-micro tabular-nums text-ink-faint">{inColumn.length}</span>
                  </div>
                  <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
                    {inColumn.map((t) => {
                      const isFocused = focused === String(t.id);
                      return (
                        <article
                          key={t.id}
                          ref={isFocused ? focusedCard : undefined}
                          className={cx(
                            "rounded-md border bg-surface-raised p-2.5",
                            isFocused
                              ? "border-accent ring-1 ring-accent/40"
                              : "border-border-subtle",
                          )}
                        >
                          <p className="selectable text-small leading-snug break-words">
                            {rank.has(String(t.id)) && (
                              <span className="mr-1.5 rounded bg-accent/15 px-1.5 py-0.5 text-micro font-semibold tabular-nums text-accent">
                                {rank.get(String(t.id))?.position}
                              </span>
                            )}
                            {String(t.title)}
                          </p>
                          <div className="mt-2 flex flex-wrap items-center gap-1.5">
                            <Badge tone={priorityTone(String(t.priority))}>{String(t.priority)}</Badge>
                            {String(t.kind) !== "task" && <Badge>{String(t.kind)}</Badge>}
                            {((t.labels as string[] | undefined) ?? []).map((l) => (
                              <Badge key={l}>{l}</Badge>
                            ))}
                          </div>
                          {t.external_ref ? (
                            <a
                              href={String(t.external_ref)}
                              target="_blank"
                              rel="noreferrer"
                              className="mt-2 block truncate text-micro text-accent hover:underline"
                              title={String(t.external_ref)}
                            >
                              {String(t.external_ref)}
                            </a>
                          ) : null}
                          <div className="mt-1.5 truncate">
                            <Id value={t.id} />
                          </div>

                          {(() => {
                            const stream = notesByTask.get(String(t.id)) ?? [];
                            if (stream.length === 0) return null;
                            const isOpen = open.has(String(t.id));
                            return (
                              <div className="mt-2 border-t border-border-subtle pt-2">
                                <button
                                  type="button"
                                  onClick={() =>
                                    setOpen((prev) => {
                                      const next = new Set(prev);
                                      if (next.has(String(t.id))) next.delete(String(t.id));
                                      else next.add(String(t.id));
                                      return next;
                                    })
                                  }
                                  className="text-micro text-ink-muted hover:text-ink"
                                >
                                  {isOpen ? "▾" : "▸"} {stream.length}{" "}
                                  {stream.length === 1 ? "note" : "notes"}
                                </button>
                                {isOpen && (
                                  <ul className="mt-1.5 space-y-1.5">
                                    {stream.map((n) => (
                                      <li
                                        key={n.id}
                                        className="selectable text-micro leading-snug text-ink-muted"
                                      >
                                        {n.body}
                                        <span className="mt-0.5 block text-ink-faint">
                                          {n.author} · {n.created_at.slice(0, 10)}
                                        </span>
                                      </li>
                                    ))}
                                  </ul>
                                )}
                              </div>
                            );
                          })()}
                        </article>
                      );
                    })}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </Page>
  );
}
