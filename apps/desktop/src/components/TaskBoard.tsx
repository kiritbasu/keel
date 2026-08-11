/**
 * Tasks as columns.
 *
 * Kept, and no longer the only option — most real tracker work happens in a
 * list. What the board is good at is showing where the work is piled up, which
 * is a shape you cannot see in a table.
 */

import { Badge, MilestoneChip, cx, priorityTone } from "./ui";
import { href } from "../lib/router";
import { taskRef, type Group, type RankMap } from "../lib/tasks";
import type { Entity } from "../lib/api";

export function TaskBoard({
  groups,
  project,
  projectKey,
  rank,
  noteCounts,
  milestoneNames,
  onFilterMilestone,
}: {
  groups: Group[];
  project: string;
  projectKey: string | undefined;
  rank: RankMap;
  noteCounts: ReadonlyMap<string, number>;
  milestoneNames: ReadonlyMap<string, string>;
  onFilterMilestone: (id: string | "none") => void;
}) {
  return (
    // Flex with fixed-width columns and horizontal scroll, not a grid. A
    // six-column grid with a min-width per column resolves by overflowing its
    // tracks rather than scrolling, which puts each column's cards on top of
    // the next column's heading.
    <div className="flex min-h-0 flex-1 gap-3 overflow-x-auto pb-2">
      {groups.map((group) => (
        <div key={group.key} className="flex w-[240px] shrink-0 flex-col">
          <div className="mb-2 flex items-baseline justify-between gap-2 px-1">
            <span className="text-micro font-medium tracking-wide text-ink-muted uppercase">
              {group.label}
            </span>
            <span className="text-micro tabular-nums text-ink-faint">
              {doneCount(group.tasks) > 0
                ? `${doneCount(group.tasks)} of ${group.tasks.length}`
                : group.tasks.length}
            </span>
          </div>
          <div className="min-h-0 flex-1 space-y-2 overflow-y-auto pr-1">
            {group.tasks.map((task) => (
              <TaskCard
                key={String(task.id)}
                task={task}
                project={project}
                projectKey={projectKey}
                position={rank.get(String(task.id))?.position}
                notes={noteCounts.get(String(task.id)) ?? 0}
                milestoneName={milestoneNames.get(String(task.milestone_id ?? ""))}
                onFilterMilestone={onFilterMilestone}
              />
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}

/** How many of a group's tasks are finished, so a phase reads "4 of 15". */
function doneCount(tasks: Entity[]): number {
  return tasks.filter((t) => String(t.status) === "done").length;
}

/** How many external links a task carries. */
function links(task: Entity): number {
  return ((task.external_refs as string[] | undefined) ?? []).length;
}

function TaskCard({
  task,
  project,
  projectKey,
  position,
  notes,
  milestoneName,
  onFilterMilestone,
}: {
  task: Entity;
  project: string;
  projectKey: string | undefined;
  position: number | undefined;
  notes: number;
  milestoneName: string | undefined;
  onFilterMilestone: (id: string | "none") => void;
}) {
  const reference = taskRef(projectKey, task);
  const milestoneId = task.milestone_id as string | null;
  return (
    // A wrapper with a stretched link rather than an anchor around everything:
    // the milestone chip is a control, and interactive content nested inside an
    // anchor is invalid. The `after:` overlay keeps the whole card clickable,
    // and anything that needs to sit above it is `relative`.
    <div
      className={cx(
        "relative rounded-card border border-border-subtle bg-surface-raised p-2.5",
        "transition-colors hover:border-accent/50 hover:bg-surface-hover",
        "focus-within:ring-2 focus-within:ring-accent/60",
      )}
    >
      <p className="text-small leading-snug break-words">
        {position !== undefined && (
          <span className="mr-1.5 rounded bg-accent/15 px-1.5 py-0.5 text-micro font-semibold tabular-nums text-accent">
            {position}
          </span>
        )}
        <a
          href={href({ screen: "task", project, taskId: reference })}
          className="after:absolute after:inset-0 after:content-[''] focus-visible:outline-none"
        >
          {String(task.title)}
        </a>
      </p>
      <div className="mt-2 flex flex-wrap items-center gap-1.5">
        <Badge tone={priorityTone(String(task.priority))}>{String(task.priority)}</Badge>
        {String(task.kind) !== "task" && <Badge>{String(task.kind)}</Badge>}
        {((task.labels as string[] | undefined) ?? []).map((label) => (
          <Badge key={label}>{label}</Badge>
        ))}
      </div>
      <div className="relative mt-1.5 flex items-center gap-2 text-micro text-ink-faint">
        <span className="font-mono">{reference}</span>
        {notes > 0 && (
          <span>
            {notes} {notes === 1 ? "note" : "notes"}
          </span>
        )}
        {links(task) > 0 && (
          <span>
            {links(task)} {links(task) === 1 ? "link" : "links"}
          </span>
        )}
        <span className="ml-auto">
          <MilestoneChip
            name={milestoneName}
            onClick={() => onFilterMilestone(milestoneId ?? "none")}
          />
        </span>
      </div>
    </div>
  );
}
