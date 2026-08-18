/**
 * Ready — what can be worked on right now.
 *
 * The ranking has existed since TQ-16 and had no front door: the only way to
 * reach it was inside the digest, which nothing in the app showed. This is the
 * third door onto one computation — `specline ready` and `specline_next` are the other
 * two, and a daemon test asserts all of them return the same list in the same
 * order. That is the property worth protecting: an app that disagreed with the
 * session about what to do next would be worse than one that stayed silent.
 *
 * Read-only, like every other screen. Claiming a task is Claude's job, so the
 * row offers the prompt to paste rather than a button that cannot exist.
 */

import { api, type NextItem, type Entity } from "../lib/api";
import { href, setQuery } from "../lib/router";
import { useAsync } from "../lib/useAsync";
import {
  Badge,
  Chip,
  Empty,
  ErrorBox,
  Id,
  Spinner,
  TruncationNote,
  priorityTone,
} from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

interface ReadyResponse {
  ready: NextItem[];
  total: number;
  truncated: boolean;
}

/**
 * The part of a milestone's name people say out loud.
 *
 * "Phase 8 — The working loop" is a heading, not a chip, and the selected and
 * unselected chips have to agree about that — they sit next to each other, and
 * one reading "Phase 8" beside another reading the whole subtitle looks like two
 * different kinds of control. The full name stays on hover.
 */
function shortName(name: string): string {
  return name.split(/\s+[—–-]\s+/)[0]?.trim() ?? name;
}

/**
 * The buckets, in the order they are shown.
 *
 * Ready used to be one list numbered 1 to 29, which implied an ordering its
 * inputs could not support: `unblocks` was 0 on every open task and priority
 * was p2 on most of them (B-83). Grouping puts the judgement where it actually
 * is — which bucket leads — instead of pretending row 14 was ranked above row
 * 15.
 */
const GROUPS = [
  { id: "active", label: "In an active phase" },
  { id: "bug", label: "Bugs" },
  { id: "rest", label: "Everything else, oldest first" },
] as const;

/** How many lead the page. Enough to check, few enough to read. */
const NEXT_UP = 3;

export function NextScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  // The filters live in the address, so a filtered view is a link — the same
  // rule the board follows. "What is next in Phase 8, unclaimed" is something
  // you can bookmark.
  const unclaimed = route.query.unclaimed === "true";
  const milestone = route.query.milestone ?? "";

  const { data, error, loading, reload } = useAsync<ReadyResponse>(
    () =>
      api.ready({
        project: project ?? "",
        unclaimed: unclaimed ? "true" : undefined,
        milestone: milestone || undefined,
        // Higher than the tool's default of ten: a screen has room, and a
        // person scanning a list would rather scroll than raise a limit.
        limit: 50,
      }),
    [project, generation, unclaimed, milestone],
  );

  const milestones = useAsync<{ items: Entity[] }>(
    () => api.entities({ project, type: "milestone", limit: 200 }),
    [project, generation],
  );

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="What’s next" crumbs={projectCrumbs(route, "What’s next")}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const items = data?.ready ?? [];
  // The lead is the front of the same ordering, not a second computation — the
  // CLI, the MCP tool and this screen have to agree, and a separate rule here
  // is how they would stop agreeing.
  const nextUp = items.slice(0, NEXT_UP);
  const rest = items.slice(NEXT_UP);

  const row = (item: NextItem) => (
    <li key={item.id}>
      <a
        href={href({ screen: "task", project, taskId: item.reference || item.id })}
        className="block rounded-card border border-border-subtle bg-surface-raised px-3 py-2.5 hover:border-accent"
      >
        <div className="flex items-baseline gap-2">
          <Id value={item.reference} />
          <span className="min-w-0 flex-1 truncate font-medium">{item.title}</span>
          <Badge tone={priorityTone(item.priority)}>{item.priority}</Badge>
        </div>
        <div className="mt-0.5 text-small text-ink-muted">{item.why}</div>
      </a>
    </li>
  );
  const names = new Map(
    (milestones.data?.items ?? []).map((m) => [String(m.id), String(m.name ?? "")]),
  );

  return (
    <Page
      title="What’s next"
      crumbs={projectCrumbs(route, "What’s next")}
      meta={
        <span className="text-small text-ink-faint">
          {/* Not "ordered by what each one unblocks" any more. That was a claim
              the data could not support — `unblocks` is 0 on every open task
              here — and stating it was what made the list impossible to
              audit (B-83). */}
          {data?.total ?? 0} ready · grouped, then oldest first
        </span>
      }
    >
      <div className="mb-3 flex flex-wrap items-center gap-1.5">
        <Chip
          selected={unclaimed}
          onClick={() =>
            setQuery(route, { unclaimed: unclaimed ? undefined : "true" })
          }
        >
          Unclaimed only
        </Chip>
        {milestone ? (
          <Chip
            selected
            onClick={() => setQuery(route, { milestone: undefined })}
            title={names.get(milestone) ?? milestone}
          >
            {shortName(names.get(milestone) ?? milestone)} ✕
          </Chip>
        ) : (
          (milestones.data?.items ?? [])
            .filter((m) => String(m.status) === "active")
            .map((m) => (
              <Chip
                key={m.id}
                onClick={() => setQuery(route, { milestone: String(m.id) })}
                title={String(m.name ?? "")}
              >
                {shortName(String(m.name ?? ""))}
              </Chip>
            ))
        )}
      </div>

      {nextUp.length > 0 && (
        <section className="mb-5">
          <h2 className="mb-1.5 text-micro uppercase tracking-wide text-ink-faint">Next up</h2>
          <ol className="space-y-1.5">{nextUp.map(row)}</ol>
        </section>
      )}

      {items.length === 0 ? (
        <Empty
          message="Nothing is ready."
          hint={
            unclaimed || milestone
              ? "The filters may be narrower than the work. Clear them to see everything."
              : "Everything open is either blocked or waiting on a decision. The Overview says which."
          }
        />
      ) : (
        <div className="space-y-5">
          {GROUPS.map(({ id, label }) => {
            const rows = rest.filter((i) => i.group === id);
            // Counted over everything ready, not just the rows in this section.
            // Next up is drawn off the top of the same list, so counting `rest`
            // reported 8 for a group that had 11 in it — a cut that did not say
            // it had cut anything.
            const total = items.filter((i) => i.group === id).length;
            const promoted = total - rows.length;
            if (rows.length === 0) return null;
            return (
              <section key={id}>
                <h2 className="mb-1.5 flex items-baseline gap-2 text-micro uppercase tracking-wide text-ink-faint">
                  {label}
                  <span className="text-ink-faint">{total}</span>
                  {promoted > 0 && (
                    <span className="normal-case tracking-normal text-ink-faint">
                      {promoted} in next up
                    </span>
                  )}
                </h2>
                <ol className="space-y-1.5">{rows.map(row)}</ol>
              </section>
            );
          })}
        </div>
      )}

      {data?.truncated ? (
        <TruncationNote shown={items.length} total={data.total} />
      ) : null}
    </Page>
  );
}
