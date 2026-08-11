/**
 * The controls that narrow the task list.
 *
 * Every one of them writes to the address rather than to component state, so
 * the view you are looking at is always a link — and the Back button undoes a
 * filter change the way it undoes anything else.
 *
 * The free-text box is the exception in one respect: it is local while you
 * type, and only commits on submit. A history entry per keystroke would make
 * Back useless.
 */

import { useEffect, useState } from "react";
import { Button, Chip, Input, Menu, MenuItem } from "./ui";
import { COLUMNS, GROUP_BY, SORT_BY, type GroupBy, type SortBy, type SortDir } from "../lib/tasks";
import { activeCount, toggle, type TaskFilter } from "../lib/filters";

const PRIORITIES = ["p0", "p1", "p2", "p3"];
const KINDS = ["task", "bug", "chore", "spike"];

/** Every distinct value present in the data, so a facet never offers a dead end. */
export interface Facets {
  labels: string[];
  milestones: Array<{ id: string; name: string }>;
}

export interface View {
  filter: TaskFilter;
  group: GroupBy;
  sort: SortBy;
  dir: SortDir;
  layout: "board" | "list";
}

export function FilterBar({
  view,
  facets,
  total,
  onFilter,
  onView,
  milestoneNoun,
}: {
  view: View;
  facets: Facets;
  /** How many tasks the box is narrowing, so it can say so. */
  total: number;
  onFilter: (next: TaskFilter) => void;
  onView: (next: Partial<Omit<View, "filter">>) => void;
  /** The project's own word for a milestone, when it has one. */
  milestoneNoun?: string;
}) {
  const noun = (milestoneNoun ?? "milestone").toLowerCase();
  const { filter } = view;
  const [text, setText] = useState(filter.text);
  useEffect(() => setText(filter.text), [filter.text]);

  const count = activeCount(filter);

  return (
    <>
      <form
        onSubmit={(e) => {
          e.preventDefault();
          onFilter({ ...filter, text });
        }}
        className="mr-1"
      >
        <Input
          variant="sm"
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={`Filter ${total} ${total === 1 ? "task" : "tasks"}`}
          aria-label="Filter by text"
          className="w-40"
        />
      </form>

      <MultiMenu
        label="Status"
        options={COLUMNS.map((c) => ({ value: c, label: c.replace("_", " ") }))}
        selected={filter.status}
        onToggle={(value) => onFilter({ ...filter, status: toggle(filter.status, value) })}
      />
      <MultiMenu
        label="Priority"
        options={PRIORITIES.map((p) => ({ value: p, label: p }))}
        selected={filter.priority}
        onToggle={(value) => onFilter({ ...filter, priority: toggle(filter.priority, value) })}
      />
      <MultiMenu
        label="Kind"
        options={KINDS.map((k) => ({ value: k, label: k }))}
        selected={filter.kind}
        onToggle={(value) => onFilter({ ...filter, kind: toggle(filter.kind, value) })}
      />
      {facets.labels.length > 0 && (
        <MultiMenu
          label="Label"
          options={facets.labels.map((l) => ({ value: l, label: l }))}
          selected={filter.labels}
          onToggle={(value) => onFilter({ ...filter, labels: toggle(filter.labels, value) })}
        />
      )}
      {facets.milestones.length > 0 && (
        <Menu
          label={
            filter.milestone
              ? (facets.milestones.find((m) => m.id === filter.milestone)?.name ??
                `No ${noun}`)
              : noun.charAt(0).toUpperCase() + noun.slice(1)
          }
        >
          {(close) => (
            <>
              <MenuItem
                selected={!filter.milestone}
                onClick={() => {
                  close();
                  onFilter({ ...filter, milestone: undefined });
                }}
              >
                {`Any ${noun}`}
              </MenuItem>
              {facets.milestones.map((m) => (
                <MenuItem
                  key={m.id}
                  selected={filter.milestone === m.id}
                  onClick={() => {
                    close();
                    onFilter({ ...filter, milestone: m.id });
                  }}
                >
                  {m.name}
                </MenuItem>
              ))}
              <MenuItem
                selected={filter.milestone === "none"}
                onClick={() => {
                  close();
                  onFilter({ ...filter, milestone: "none" });
                }}
              >
                No milestone
              </MenuItem>
            </>
          )}
        </Menu>
      )}

      <Chip
        selected={filter.blocked}
        onClick={() => onFilter({ ...filter, blocked: !filter.blocked })}
        title="Only tasks something is linked to as a blocker — the one definition of blocked"
      >
        blocked
      </Chip>

      {count > 0 && (
        <Button size="sm" variant="ghost" onClick={() => onFilter({ ...filter, ...CLEARED })}>
          Clear {count}
        </Button>
      )}

      <span className="ml-auto flex items-center gap-1.5">
        <Menu label={`Group: ${view.group}`} align="right">
          {(close) =>
            GROUP_BY.map((g) => (
              <MenuItem
                key={g}
                selected={view.group === g}
                onClick={() => {
                  close();
                  onView({ group: g });
                }}
              >
                {g}
              </MenuItem>
            ))
          }
        </Menu>
        <Menu label={`Sort: ${view.sort}`} align="right">
          {(close) => (
            <>
              {SORT_BY.map((s) => (
                <MenuItem
                  key={s}
                  selected={view.sort === s}
                  onClick={() => {
                    close();
                    onView({ sort: s });
                  }}
                >
                  {s}
                </MenuItem>
              ))}
              <MenuItem
                onClick={() => {
                  close();
                  onView({ dir: view.dir === "asc" ? "desc" : "asc" });
                }}
              >
                {view.dir === "asc" ? "↓ reverse" : "↑ reverse"}
              </MenuItem>
            </>
          )}
        </Menu>
        <Chip selected={view.layout === "board"} onClick={() => onView({ layout: "board" })}>
          board
        </Chip>
        <Chip selected={view.layout === "list"} onClick={() => onView({ layout: "list" })}>
          list
        </Chip>
      </span>
    </>
  );
}

/** The parts of a filter "clear" resets. Text included — it is a filter too. */
const CLEARED = {
  status: [],
  priority: [],
  kind: [],
  labels: [],
  milestone: undefined,
  blocked: false,
  text: "",
} satisfies TaskFilter;

/**
 * A facet with more than a couple of values.
 *
 * A menu rather than a row of chips: eleven labels laid out as chips is the
 * wall the search screen used to be, and it pushes everything else off the row.
 * The count on the trigger is what keeps a closed menu honest about whether it
 * is doing anything.
 */
function MultiMenu({
  label,
  options,
  selected,
  onToggle,
}: {
  label: string;
  options: Array<{ value: string; label: string }>;
  selected: string[];
  onToggle: (value: string) => void;
}) {
  return (
    <Menu label={selected.length > 0 ? `${label} (${selected.length})` : label}>
      {() =>
        options.map((option) => (
          <MenuItem
            key={option.value}
            selected={selected.includes(option.value)}
            // Deliberately does not close: choosing several values from one
            // facet is the common case, and a menu that shuts after each pick
            // makes "todo and blocked" four clicks instead of two.
            onClick={() => onToggle(option.value)}
          >
            {selected.includes(option.value) ? "✓ " : "  "}
            {option.label}
          </MenuItem>
        ))
      }
    </Menu>
  );
}
