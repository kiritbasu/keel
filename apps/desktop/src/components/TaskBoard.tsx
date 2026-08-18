/**
 * Tasks as columns.
 *
 * Kept, and no longer the only option — most real tracker work happens in a
 * list. What the board is good at is showing where the work is piled up, which
 * is a shape you cannot see in a table.
 *
 * # Dragging
 *
 * A card can be dragged to another column when the board is grouped by status,
 * because that is the only grouping where the column a card lands in is a
 * thing the card can be told to become. Grouped by label a card can legitimately
 * sit in three columns at once; grouped by phase the gesture would be useful
 * and is not what KEEL-308 asked for.
 *
 * `dropOnStatus` decides what a column does with a card, and three of the six
 * do not simply take it — see its doc comment. The refusals are *shown while
 * dragging* rather than discovered on release: a drop target that silently
 * does nothing reads as a broken app, and one that explains itself is the
 * whole reason those rules are worth having.
 *
 * Plain HTML5 drag and drop, no library. It is one gesture on one screen, and
 * the scale rule in the contract is explicit that a dependency wants a
 * measurement behind it. The cost is that dragging is a pointer gesture only —
 * the keyboard route to a status is the select on the task screen, which is
 * why the cards here stay ordinary focusable links and nothing steals a key.
 */

import { useState } from "react";
import { Badge, MilestoneChip, cx, priorityTone } from "./ui";
import { href } from "../lib/router";
import { dropOnStatus, taskRef, type Group, type RankMap } from "../lib/tasks";
import type { Entity } from "../lib/api";

export function TaskBoard({
  groups,
  project,
  projectKey,
  rank,
  noteCounts,
  milestoneNames,
  onFilterMilestone,
  onDropOnStatus,
}: {
  groups: Group[];
  project: string;
  projectKey: string | undefined;
  rank: RankMap;
  noteCounts: ReadonlyMap<string, number>;
  milestoneNames: ReadonlyMap<string, string>;
  onFilterMilestone: (id: string | "none") => void;
  /**
   * Where a dropped card goes. Absent when the board is grouped by anything
   * but status, which is also what turns dragging off.
   */
  onDropOnStatus?: (task: Entity, columnKey: string) => void;
}) {
  // The card being dragged, and the column under the pointer. Both are needed:
  // the first to know what was dropped, the second to say what this column
  // will do with it before the pointer is released.
  const [dragging, setDragging] = useState<Entity | null>(null);
  const [over, setOver] = useState<string | null>(null);
  const draggable = Boolean(onDropOnStatus);

  return (
    // Flex with fixed-width columns and horizontal scroll, not a grid. A
    // six-column grid with a min-width per column resolves by overflowing its
    // tracks rather than scrolling, which puts each column's cards on top of
    // the next column's heading.
    <div className="flex min-h-0 flex-1 gap-3 overflow-x-auto pb-2">
      {groups.map((group) => {
        const drop = draggable ? dropOnStatus(group.key) : null;
        const takes = drop !== null && drop.kind !== "refused";
        const refusing = dragging !== null && drop?.kind === "refused";
        return (
        <div
          key={group.key}
          className="flex w-[240px] shrink-0 flex-col"
          onDragOver={(e) => {
            if (!dragging || !takes) return;
            // Without this the browser treats the column as inert and the drop
            // never fires — preventDefault on dragover *is* "yes, drop here".
            e.preventDefault();
            setOver(group.key);
          }}
          onDragLeave={(e) => {
            // Only when the pointer has actually left the column, not when it
            // crosses from the column onto a card inside it.
            if (!e.currentTarget.contains(e.relatedTarget as Node | null))
              setOver((k) => (k === group.key ? null : k));
          }}
          onDrop={(e) => {
            if (!dragging || !takes) return;
            e.preventDefault();
            const task = dragging;
            setDragging(null);
            setOver(null);
            onDropOnStatus?.(task, group.key);
          }}
        >
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
          {/* Said while the card is still in the air, not after it lands on
              nothing. The reason is the point — "you cannot" without "because"
              is indistinguishable from a bug. */}
          {refusing && drop.kind === "refused" && (
            <p
              role="status"
              className="mb-2 rounded-card border border-dashed border-border-subtle px-2 py-1.5 text-micro text-ink-faint"
            >
              {drop.why}
            </p>
          )}
          <div
            className={cx(
              "min-h-0 flex-1 space-y-2 overflow-y-auto rounded-card pr-1 transition-colors",
              over === group.key && takes && "bg-accent/5 ring-1 ring-accent/40",
            )}
          >
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
                draggable={draggable}
                dragging={dragging !== null && String(dragging.id) === String(task.id)}
                onDragStart={() => setDragging(task)}
                onDragEnd={() => {
                  setDragging(null);
                  setOver(null);
                }}
              />
            ))}
          </div>
        </div>
        );
      })}
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
  draggable,
  dragging,
  onDragStart,
  onDragEnd,
}: {
  task: Entity;
  project: string;
  projectKey: string | undefined;
  position: number | undefined;
  notes: number;
  milestoneName: string | undefined;
  onFilterMilestone: (id: string | "none") => void;
  draggable: boolean;
  dragging: boolean;
  onDragStart: () => void;
  onDragEnd: () => void;
}) {
  const reference = taskRef(projectKey, task);
  const milestoneId = task.milestone_id as string | null;
  return (
    // A wrapper with a stretched link rather than an anchor around everything:
    // the milestone chip is a control, and interactive content nested inside an
    // anchor is invalid. The `after:` overlay keeps the whole card clickable,
    // and anything that needs to sit above it is `relative`.
    <div
      draggable={draggable}
      onDragStart={(e) => {
        // Firefox will not start a drag unless the event carries data, and
        // the id is the useful thing to carry for anything outside this app.
        e.dataTransfer.setData("text/plain", String(task.id));
        e.dataTransfer.effectAllowed = "move";
        onDragStart();
      }}
      onDragEnd={onDragEnd}
      className={cx(
        "relative rounded-card border border-border-subtle bg-surface-raised p-2.5",
        "transition-colors hover:border-accent/50 hover:bg-surface-hover",
        "focus-within:ring-2 focus-within:ring-accent/60",
        draggable && "cursor-grab active:cursor-grabbing",
        dragging && "opacity-40",
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
