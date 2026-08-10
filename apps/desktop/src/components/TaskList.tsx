/**
 * Tasks as rows.
 *
 * The layout most tracker work actually happens in: dense, scannable, and able
 * to show forty tasks at once where the board shows six. The board is better at
 * "where is the work piled up"; this is better at everything else.
 *
 * Column headers sort. Clicking the one already sorted by reverses it, which is
 * the behaviour every table in the world has and therefore the one nobody has
 * to be told about.
 */

import { Badge, cx, priorityTone, statusTone, when } from "./ui";
import { href } from "../lib/router";
import { taskRef, type Group, type RankMap, type SortBy, type SortDir } from "../lib/tasks";
import type { Entity } from "../lib/api";

const COLUMNS: Array<{ sort: SortBy; label: string; className: string }> = [
  { sort: "number", label: "Ref", className: "w-24" },
  { sort: "title", label: "Task", className: "" },
  { sort: "status", label: "Status", className: "w-28" },
  { sort: "priority", label: "Priority", className: "w-20" },
  { sort: "updated", label: "Updated", className: "w-24 text-right" },
];

export function TaskList({
  groups,
  project,
  projectKey,
  rank,
  sort,
  dir,
  onSort,
  showGroupHeadings,
}: {
  groups: Group[];
  project: string;
  projectKey: string | undefined;
  rank: RankMap;
  sort: SortBy;
  dir: SortDir;
  onSort: (by: SortBy) => void;
  showGroupHeadings: boolean;
}) {
  return (
    <div className="min-h-0 flex-1 overflow-y-auto">
      <table className="w-full border-collapse text-small">
        <thead className="sticky top-0 z-10 bg-surface">
          <tr className="border-b border-border-subtle text-micro tracking-wide text-ink-faint uppercase">
            {COLUMNS.map((column) => (
              <th key={column.sort} className={cx("px-2 py-1.5 text-left", column.className)}>
                <button
                  type="button"
                  onClick={() => onSort(column.sort)}
                  className="hover:text-ink"
                  aria-sort={
                    sort === column.sort
                      ? dir === "asc"
                        ? "ascending"
                        : "descending"
                      : undefined
                  }
                >
                  {column.label}
                  {sort === column.sort && <span className="ml-1">{dir === "asc" ? "↑" : "↓"}</span>}
                </button>
              </th>
            ))}
          </tr>
        </thead>

        {groups.map((group) => (
          <tbody key={group.key}>
            {showGroupHeadings && (
              <tr>
                <th
                  colSpan={COLUMNS.length}
                  className="bg-surface-raised px-2 py-1 text-left text-micro font-medium tracking-wide text-ink-muted uppercase"
                >
                  {group.label}
                  <span className="ml-2 tabular-nums text-ink-faint">{group.tasks.length}</span>
                </th>
              </tr>
            )}
            {group.tasks.map((task) => {
              const reference = taskRef(projectKey, task);
              const position = rank.get(String(task.id))?.position;
              return (
                <tr
                  key={String(task.id)}
                  className="border-b border-border-subtle/50 hover:bg-surface-hover"
                >
                  {/* The link is on the title cell rather than the row: a row
                      cannot be an anchor, and wrapping every cell in one makes
                      the whole table a tab stop per column. */}
                  <td className="px-2 py-1.5 align-top font-mono text-micro text-ink-faint">
                    {reference}
                  </td>
                  <td className="px-2 py-1.5 align-top">
                    <a
                      href={href({ screen: "task", project, taskId: reference })}
                      className="hover:underline"
                    >
                      {position !== undefined && (
                        <span className="mr-1.5 rounded bg-accent/15 px-1.5 py-0.5 text-micro font-semibold tabular-nums text-accent">
                          {position}
                        </span>
                      )}
                      {String(task.title)}
                    </a>
                    {((task.labels as string[] | undefined) ?? []).length > 0 && (
                      <span className="ml-2 inline-flex flex-wrap gap-1">
                        {((task.labels as string[] | undefined) ?? []).map((label) => (
                          <Badge key={label}>{label}</Badge>
                        ))}
                      </span>
                    )}
                  </td>
                  <td className="px-2 py-1.5 align-top">
                    <Badge tone={statusTone(String(task.status))}>{String(task.status)}</Badge>
                  </td>
                  <td className="px-2 py-1.5 align-top">
                    <Badge tone={priorityTone(String(task.priority))}>
                      {String(task.priority)}
                    </Badge>
                  </td>
                  <td className="px-2 py-1.5 text-right align-top text-micro tabular-nums text-ink-faint">
                    {task.audit?.updated_at ? when(String(task.audit.updated_at)) : "—"}
                  </td>
                </tr>
              );
            })}
          </tbody>
        ))}
      </table>
    </div>
  );
}

/** The task ordering a row-based layout walks, for J/K parity elsewhere. */
export function inListOrder(groups: Group[]): Entity[] {
  return groups.flatMap((group) => group.tasks);
}
