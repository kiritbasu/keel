/**
 * Screen 4 — Board. Tasks by status, filterable, keyboard-driven.
 */

import { useMemo, useState } from "react";
import { api, type Digest, type Entity, type Note, type Page as PageOf } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Spinner, cx, priorityTone } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { href } from "../lib/router";
import { COLUMNS, compareTasks, taskRef, type RankMap } from "../lib/tasks";
import type { ScreenProps } from "../App";

export function BoardScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const [urgentOnly, setUrgentOnly] = useState(false);
  const [label, setLabel] = useState<string | null>(null);

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

  // The project key, so a card can say KEEL-42. It arrives with the digest,
  // which this screen already fetches for the ranking.
  const key = digest.data?.project?.key;
  const ranked = digest.data?.next_up ?? null;
  const rank = useMemo<RankMap>(() => {
    const m: RankMap = new Map();
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
                    <span className="mr-1.5 font-mono text-micro text-ink-faint">
                      {item.reference}
                    </span>
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
              // The comparator is shared with the detail view, so `J` and `K`
              // walk exactly the sequence the board shows.
              const inColumn = tasks
                .filter((t) => String(t.status) === column)
                .sort(compareTasks(rank));
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
                      const notes = notesByTask.get(String(t.id))?.length ?? 0;
                      const reference = taskRef(key, t);
                      return (
                        // The whole card is the link. It used to be an
                        // `<article>` with no click handler, no hover state and
                        // no focus — the dead end this phase is named after.
                        //
                        // The note stream and the external link moved to the
                        // detail view rather than being duplicated here: an
                        // anchor cannot contain another anchor, and a paragraph
                        // of commentary inside a 240px column buried the board
                        // it was attached to. Both are one click away.
                        <a
                          key={t.id}
                          href={href({ screen: "task", project, taskId: reference })}
                          className={cx(
                            "block rounded-md border border-border-subtle bg-surface-raised p-2.5",
                            "transition-colors hover:border-accent/50 hover:bg-surface-hover",
                            "focus-visible:ring-2 focus-visible:ring-accent/60 focus-visible:outline-none",
                          )}
                        >
                          <p className="text-small leading-snug break-words">
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
                          <div className="mt-1.5 flex items-center gap-2 text-micro text-ink-faint">
                            <span className="font-mono">{reference}</span>
                            {notes > 0 && (
                              <span>
                                {notes} {notes === 1 ? "note" : "notes"}
                              </span>
                            )}
                            {t.external_ref ? <span>has a link</span> : null}
                          </div>
                        </a>
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
