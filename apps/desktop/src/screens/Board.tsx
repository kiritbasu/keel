/**
 * Screen 4 — the tasks. Two layouts, one address.
 *
 * Board or list, grouped and sorted how you choose, filtered by anything the
 * rows carry — and all of it in the URL, so the view you are looking at is a
 * link you can send. That is also what gives saved views for free: bookmark it.
 *
 * The ranked "Next" panel stays visible whatever is filtered. It used to
 * disappear the moment you touched a filter, which meant the best thing in the
 * app was only ever there when you did not need it.
 */

import { useMemo } from "react";
import { api, type Digest, type Entity, type Note, type Page as PageOf } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Empty, ErrorBox, Spinner } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { FilterBar, type Facets, type View } from "../components/FilterBar";
import { TaskBoard } from "../components/TaskBoard";
import { TaskList } from "../components/TaskList";
import { setQuery } from "../lib/router";
import {
  GROUP_BY,
  SORT_BY,
  groupTasks,
  sortTasks,
  type GroupBy,
  type RankMap,
  type SortBy,
  type SortDir,
} from "../lib/tasks";
import { applyFilter, filterToQuery, isFiltering, parseFilter } from "../lib/filters";
import type { ScreenProps } from "../App";

export function BoardScreen({ route, generation }: ScreenProps) {
  const project = route.project;

  // The whole view comes out of the address. Anything unrecognised falls back
  // to the default rather than erroring — a hand-edited URL should degrade, not
  // break.
  const filter = parseFilter(route.query);
  const layout = route.query.view === "list" ? "list" : "board";
  const group: GroupBy = GROUP_BY.includes(route.query.group as GroupBy)
    ? (route.query.group as GroupBy)
    : "status";
  const sort: SortBy = SORT_BY.includes(route.query.sort as SortBy)
    ? (route.query.sort as SortBy)
    : "next";
  const dir: SortDir = route.query.dir === "desc" ? "desc" : "asc";

  const { data, error, loading, reload } = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "task", limit: 2000 }),
    [project, generation],
  );

  // The same ranking the digest gives an agent, rather than a second opinion
  // computed in the browser. A board that disagrees with what Claude was told
  // is worse than a board with no ordering at all.
  const digest = useAsync<Digest>(() => api.context(project), [project, generation]);

  const milestones = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "milestone", limit: 200 }),
    [project, generation],
  );

  // Every stream in one request. Seventy cards asking individually is seventy
  // round trips to render a count.
  const notes = useAsync<{ notes: Note[]; total: number }>(
    () => api.notes(project),
    [project, generation],
  );
  const noteCounts = useMemo(() => {
    const counts = new Map<string, number>();
    for (const note of notes.data?.notes ?? []) {
      counts.set(note.entity_id, (counts.get(note.entity_id) ?? 0) + 1);
    }
    return counts;
  }, [notes.data]);

  const ranked = digest.data?.next_up ?? null;
  const projectKey = digest.data?.project?.key;

  const rank = useMemo<RankMap>(() => {
    const m: RankMap = new Map();
    ranked?.ready.forEach((item, i) => m.set(item.id, { position: i + 1, why: item.why }));
    return m;
  }, [ranked]);

  // What "blocked" means here is what it means to the ranking: something is
  // linked to it as a blocker. The app must not grow a second opinion.
  const blockedIds = useMemo(
    () => new Set((ranked?.blocked ?? []).map((item) => item.id)),
    [ranked],
  );

  // Milestones and tasks share one lookup, because grouping needs a name for
  // whatever it grouped by and both kinds of key land in the same place.
  const groupNames = useMemo(() => {
    const names = new Map<string, string>();
    for (const m of milestones.data?.items ?? []) names.set(String(m.id), String(m.name));
    for (const t of data?.items ?? []) names.set(String(t.id), String(t.title));
    return names;
  }, [milestones.data, data]);

  const facets = useMemo<Facets>(() => {
    const labels = new Set<string>();
    for (const task of data?.items ?? []) {
      for (const label of (task.labels as string[] | undefined) ?? []) labels.add(label);
    }
    return {
      labels: [...labels].sort(),
      milestones: (milestones.data?.items ?? []).map((m) => ({
        id: String(m.id),
        name: String(m.name),
      })),
    };
  }, [data, milestones.data]);

  const filterKey = JSON.stringify(filter);
  const groups = useMemo(() => {
    const matching = applyFilter(data?.items ?? [], parseFilter(route.query), blockedIds);
    // A board with one column is not a board, so `none` degrades to status
    // there rather than being offered and then quietly ignored.
    const by = layout === "board" && group === "none" ? "status" : group;
    return groupTasks(matching, by, groupNames).map((g) => ({
      ...g,
      tasks: sortTasks(g.tasks, sort, dir, rank),
    }));
    // `filter` is rebuilt from the query on every render, so the memo compares
    // its serialised form rather than its identity.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, filterKey, blockedIds, group, layout, groupNames, sort, dir, rank]);

  // Counted as distinct tasks, not as the sum of the groups: grouping by label
  // puts a task with three labels in three groups, and adding the columns up
  // would report more tasks than exist.
  const shown = useMemo(
    () => new Set(groups.flatMap((g) => g.tasks.map((t) => String(t.id)))).size,
    [groups],
  );

  const view: View = { filter, group, sort, dir, layout };

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="Tasks" crumbs={projectCrumbs(route, "Board")}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  return (
    <Page
      title="Tasks"
      crumbs={projectCrumbs(route, "Board")}
      width="full"
      meta={
        <span className="text-small text-ink-faint">
          {shown} of {data?.total ?? 0}
        </span>
      }
      toolbar={
        <FilterBar
          view={view}
          facets={facets}
          onFilter={(next) => setQuery(route, filterToQuery(next), { replace: true })}
          onView={(next) =>
            setQuery(
              route,
              {
                // A default is written as the absence of a parameter, so the
                // unfiltered board has a clean address rather than one trailing
                // four parameters that say "as usual".
                ...(next.group ? { group: next.group === "status" ? undefined : next.group } : {}),
                ...(next.sort ? { sort: next.sort === "next" ? undefined : next.sort } : {}),
                ...(next.dir ? { dir: next.dir === "asc" ? undefined : next.dir } : {}),
                ...(next.layout
                  ? { view: next.layout === "board" ? undefined : next.layout }
                  : {}),
              },
              { replace: true },
            )
          }
        />
      }
    >
      <div className="flex h-full flex-col p-6">
        {/* Shown whatever is filtered. The ranking answers "what should I do
            next", and that does not stop being the question because you
            narrowed the board to look at something else. */}
        {ranked && ranked.ready.length > 0 && (
          <section className="mb-4 shrink-0 rounded-lg border border-accent/30 bg-accent/5 px-3 py-2.5">
            <h2 className="mb-1.5 text-micro font-semibold tracking-wide text-accent uppercase">
              Next
            </h2>
            <ol className="space-y-1">
              {ranked.ready.slice(0, 3).map((item, i) => (
                <li key={item.id} className="flex gap-2 text-small">
                  <span className="w-3 shrink-0 text-right tabular-nums text-ink-faint">
                    {i + 1}
                  </span>
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

        {shown === 0 ? (
          <Empty
            message="No tasks match."
            hint={isFiltering(filter) ? "Clear a filter above." : undefined}
          />
        ) : layout === "list" ? (
          <TaskList
            groups={groups}
            project={project ?? ""}
            projectKey={projectKey}
            rank={rank}
            sort={sort}
            dir={dir}
            onSort={(by) =>
              setQuery(
                route,
                {
                  sort: by === "next" ? undefined : by,
                  // Clicking the column already sorted by reverses it.
                  dir: sort === by && dir === "asc" ? "desc" : undefined,
                },
                { replace: true },
              )
            }
            showGroupHeadings={group !== "none"}
          />
        ) : (
          <TaskBoard
            groups={groups}
            project={project ?? ""}
            projectKey={projectKey}
            rank={rank}
            noteCounts={noteCounts}
          />
        )}
      </div>
    </Page>
  );
}
