/**
 * Screen 4 — Board. Tasks by status, filterable, keyboard-driven.
 */

import { useMemo, useState } from "react";
import { api, type Entity, type Page } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Id, Spinner, cx, priorityTone } from "../components/ui";
import type { ScreenProps } from "../App";

/** Lifecycle order, left to right. Matches TaskStatus::ALL. */
const COLUMNS = ["todo", "in_progress", "blocked", "review", "done", "wont_do"] as const;

export function BoardScreen({ project, generation }: ScreenProps) {
  const [urgentOnly, setUrgentOnly] = useState(false);
  const [label, setLabel] = useState<string | null>(null);

  const { data, error, loading, reload } = useAsync<Page<Entity>>(
    () => api.entities({ project, type: "task", limit: 2000 }),
    [project, generation],
  );

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
      <div className="p-6">
        <ErrorBox error={error} retry={reload} />
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col p-6">
      <header className="mb-4 shrink-0">
        <div className="flex items-center gap-3">
          <h1 className="text-xl font-semibold tracking-tight">Board</h1>
          <span className="ml-auto text-[12px] text-ink-faint">
            {tasks.length} of {data?.total ?? 0}
          </span>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <button
          onClick={() => setUrgentOnly((v) => !v)}
          className={cx(
            "rounded border px-2 py-1 text-[12px]",
            urgentOnly
              ? "border-warn/50 bg-warn/10 text-warn"
              : "border-border-subtle text-ink-muted hover:bg-surface-hover",
          )}
        >
          urgent only
        </button>
        {labels.map((l) => (
          <button
            key={l}
            onClick={() => setLabel((v) => (v === l ? null : l))}
            className={cx(
              "rounded border px-2 py-1 text-[12px]",
              label === l
                ? "border-accent/50 bg-accent/10 text-accent"
                : "border-border-subtle text-ink-muted hover:bg-surface-hover",
            )}
          >
            {l}
          </button>
        ))}
        </div>
      </header>

      {tasks.length === 0 ? (
        <Empty message="No tasks match." hint={label || urgentOnly ? "Clear the filters above." : undefined} />
      ) : (
        // Flex with fixed-width columns and horizontal scroll, not a grid.
        // A six-column grid with a min-width per column resolves by overflowing
        // its tracks rather than scrolling, which puts each column's cards on
        // top of the next column's heading.
        <div className="flex flex-1 gap-3 overflow-x-auto pb-2">
          {COLUMNS.map((column) => {
            const inColumn = tasks.filter((t) => String(t.status) === column);
            return (
              <div key={column} className="flex w-[240px] shrink-0 flex-col">
                <div className="mb-2 flex items-baseline justify-between gap-2 px-1">
                  <span className="text-[12px] font-medium tracking-wide text-ink-muted uppercase">
                    {column.replace("_", " ")}
                  </span>
                  <span className="text-[11px] tabular-nums text-ink-faint">{inColumn.length}</span>
                </div>
                <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
                  {inColumn.map((t) => (
                    <article
                      key={t.id}
                      className="rounded-md border border-border-subtle bg-surface-raised p-2.5"
                    >
                      <p className="selectable text-[13px] leading-snug break-words">
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
                          className="mt-2 block truncate text-[11px] text-accent hover:underline"
                          title={String(t.external_ref)}
                        >
                          {String(t.external_ref)}
                        </a>
                      ) : null}
                      <div className="mt-1.5 truncate">
                        <Id value={t.id} />
                      </div>
                    </article>
                  ))}
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
