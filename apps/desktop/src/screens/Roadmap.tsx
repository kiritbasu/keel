/**
 * Screen 5 — Roadmap. The phases of one project, or of all of them.
 *
 * Built from milestones because that is what they are for: SPEC §6 calls them
 * the planning unit and says the roadmap view is built from them. Nothing here
 * infers a timeline from task dates.
 *
 * **Phases only.** Releases used to share this screen and now have one of their
 * own (KEEL-336). A phase and a release are different nouns — a phase is a unit
 * of plan that holds tasks and has progress, a release is a unit of record that
 * went out on a date and holds nothing — and one list containing both implied a
 * relationship neither has to the other. Splitting them into two sections on
 * one page was the first attempt and did not fix it: two lists stacked still
 * read as one page about one thing.
 *
 * **Grouped by what a phase is doing, not by plan order.** `sort_order` gives
 * the list the order somebody typed; it does not answer "where is this project
 * now", which is the question the screen exists for. Fifteen phases in plan
 * order buried the three that are moving in the middle of the twelve that are
 * not. The groups run in the order a reader cares about them, and the manual
 * order still holds *within* a group.
 */

import { api, type Entity, type Page as PageOf } from "../lib/api";
import { href } from "../lib/router";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Spinner, When, statusTone } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

/** The order a human asked for, then a date, then the name. */
export function byPlan(a: Entity, b: Entity): number {
  // `sort_order` first, because SPEC §3.2 gives milestones that column
  // specifically for "manual ordering for the roadmap view" — a human who has
  // said what order they want should get it, within the group.
  const ao = a.sort_order as number | null;
  const bo = b.sort_order as number | null;
  if (ao != null && bo != null && ao !== bo) return ao - bo;
  if (ao != null && bo == null) return -1;
  if (ao == null && bo != null) return 1;

  // Then by target date, for the rare phase that has one. Dated before undated,
  // since a milestone with no target is unplanned rather than far-future.
  const at = a.target_date as string | null;
  const bt = b.target_date as string | null;
  if (at && bt && at !== bt) return at.localeCompare(bt);
  if (at && !bt) return -1;
  if (!at && bt) return 1;

  // Finally by name, so ties never fall back to insertion order. Without this
  // the four phases that shipped on the same day came back newest first, so the
  // roadmap read 3, 2, 1, 0.
  return String(a.name).localeCompare(String(b.name), undefined, { numeric: true });
}

/**
 * The groups, in the order a reader wants them.
 *
 * `complete` has a group to itself rather than being folded in with `shipped`,
 * because the difference is the whole of B-57: every task closed is a fact the
 * store can derive, and "it shipped" is a declaration only a person can make.
 * A phase sitting here is waiting on somebody to say which — and three of this
 * project's phases sat in exactly that state, unnoticed, until the digest grew
 * a section for it.
 *
 * Every derived state appears in exactly one group, so no phase can fall
 * through and vanish from a screen whose whole job is to list them. The test
 * asserting that is what makes this list safe to extend.
 */
const GROUPS: Array<{ states: string[]; title: string; hint?: string }> = [
  { states: ["active", "blocked"], title: "In flight" },
  {
    states: ["complete"],
    title: "Finished, not yet declared",
    hint: "Every task is closed. Whether that means shipped or cut is not derivable — say which.",
  },
  { states: ["planned"], title: "Planned" },
  { states: ["shipped"], title: "Shipped" },
  { states: ["paused", "cut"], title: "Set aside" },
];

export function RoadmapScreen({ route, generation, milestoneNoun }: ScreenProps) {
  const noun = milestoneNoun ?? "milestone";
  const plural = `${noun.toLowerCase()}s`;
  const project = route.project;
  const { data, error, loading, reload } = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "milestone", limit: 500 }),
    [project, generation],
  );

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page title="Roadmap" crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const phases = (data?.items ?? []).filter((m) => String(m.kind) !== "release").sort(byPlan);
  const inFlight = phases.filter((m) => {
    const state = stateOf(m);
    return state === "active" || state === "blocked";
  });

  // Anything whose state is in no group. Zero today, and the point is that it
  // stays visible if a new state is ever added to the enum without being added
  // here — a phase silently missing from the roadmap is the failure this screen
  // cannot afford.
  const grouped = new Set(GROUPS.flatMap((g) => g.states));
  const ungrouped = phases.filter((m) => !grouped.has(stateOf(m)));

  return (
    <Page
      title="Roadmap"
      crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}
      meta={
        <span className="text-small text-ink-faint">
          {phases.length} {phases.length === 1 ? noun.toLowerCase() : plural}
          {inFlight.length > 0 ? ` · ${inFlight.length} in flight` : ""}
        </span>
      }
    >
      {phases.length === 0 ? (
        <Empty
          message={`No ${plural} yet.`}
          hint={`${noun[0]?.toUpperCase()}${noun.slice(1).toLowerCase()}s are what the roadmap is built from.`}
        />
      ) : (
        <div>
          {[...GROUPS, { states: [], title: "Everything else" }].map((group) => {
            const rows =
              group.states.length === 0
                ? ungrouped
                : phases.filter((m) => group.states.includes(stateOf(m)));
            if (rows.length === 0) return null;
            return (
              <section key={group.title}>
                <GroupHeading
                  title={group.title}
                  count={rows.length}
                  hint={"hint" in group ? group.hint : undefined}
                />
                <ol className="space-y-2">
                  {rows.map((m) => (
                    <li key={String(m.id)}>
                      <Row m={m} noun={noun} project={project} />
                    </li>
                  ))}
                </ol>
              </section>
            );
          })}
        </div>
      )}
    </Page>
  );
}

/**
 * `state`, not `status`.
 *
 * The column only holds what was declared — shipped, cut, paused, or nothing at
 * all — and what the phase is actually doing is worked out from its tasks by
 * the daemon (B-57). Reading `status` here is what showed a finished phase as
 * active for a week. Falling back keeps an older daemon readable.
 */
function stateOf(m: Entity): string {
  return String(m.state ?? m.status);
}

function GroupHeading({ title, count, hint }: { title: string; count: number; hint?: string }) {
  return (
    <div className="mt-6 mb-2 first:mt-0">
      <div className="flex items-center gap-2">
        <h2 className="text-micro font-medium tracking-wider text-ink-faint uppercase">{title}</h2>
        <span className="tabular text-micro text-ink-faint">{count}</span>
        <span className="h-px flex-1 bg-border-subtle" />
      </div>
      {hint ? <p className="mt-1 text-small text-ink-faint">{hint}</p> : null}
    </div>
  );
}

function Row({ m, noun, project }: { m: Entity; noun: string; project?: string }) {
  const status = stateOf(m);
  const date = m.target_date as string | null;
  const shipped = m.shipped_at as string | null;
  // Counted by the daemon, beside the state it already derived. The browser has
  // only this project's milestones, so working it out here would mean fetching
  // every task to count it (KEEL-332).
  //
  // Absent and zero are different answers and must not collapse. A daemon that
  // predates these fields sends neither, and `?? 0` would turn that silence
  // into "not scoped" — a claim about the phase rather than an admission about
  // the reply.
  const counted = typeof m.tasks_total === "number";
  const total = counted ? (m.tasks_total as number) : 0;
  const closed = typeof m.tasks_closed === "number" ? (m.tasks_closed as number) : 0;
  const moved = m.last_activity as string | null;
  const live = status === "active" || status === "blocked";

  return (
    <div
      className={
        // A left edge on the phases that are moving. It is the only ornament on
        // the row and it earns its place: the group heading says which section
        // you are in, this says it again at the point your eye lands.
        live
          ? "rounded-card border border-l-2 border-border-subtle border-l-warn bg-surface-raised px-4 py-3"
          : "rounded-card border border-border-subtle bg-surface-raised px-4 py-3"
      }
    >
      <div className="flex items-center gap-2">
        {/* A milestone on the roadmap and a chip on a card describe the same
            thing and used not to know about each other. */}
        {project ? (
          <a
            href={`${href({ screen: "board", project })}?milestone=${encodeURIComponent(String(m.id))}`}
            className="font-medium hover:text-accent"
            title={`Show the tasks in this ${noun.toLowerCase()}`}
          >
            {String(m.name)}
          </a>
        ) : (
          <span className="font-medium">{String(m.name)}</span>
        )}
        <Badge tone={statusTone(status)}>{status}</Badge>
        <span className="ml-auto flex items-center gap-3 text-small text-ink-faint">
          {/* A target only while it is still a target. On a phase that shipped
              it is history, and printing "due Aug 9" beside "shipped Aug 9" is
              two dates where one is the answer. */}
          {!shipped && date ? <Due iso={date} /> : null}
          {counted ? <Progress closed={closed} total={total} /> : null}
          {/* When it shipped beats when it last moved: a finished phase someone
              left a note on last week did not move last week in any sense a
              reader cares about. */}
          {shipped ? (
            <When iso={shipped} prefix="shipped" />
          ) : moved ? (
            <When iso={moved} prefix="moved" />
          ) : null}
        </span>
      </div>
      {/* Every phase carries its summary in full, finished ones included. It is
          the sentence saying what the phase was *for*, and a roadmap of fifteen
          bare names answers that only for whoever wrote them. This was briefly
          clamped to one line to keep the page short; a short page is not worth
          unreadable phases, and grouping already did most of that work. */}
      {m.summary ? (
        <p className="selectable mt-1 text-small text-ink-muted">{String(m.summary)}</p>
      ) : null}
    </div>
  );
}

/**
 * A target date, for the rare phase that has one.
 *
 * `new Date(x).toISOString()` throws `RangeError` on anything it cannot parse,
 * and a throw in render unmounts the whole screen. Every other field on this
 * row is read defensively; this one was not, and the roadmap is the screen you
 * open when you want to know what is going on — the worst one to lose to a bad
 * cell.
 */
function Due({ iso }: { iso: string }) {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;
  return <When iso={at.toISOString()} prefix="due" />;
}

/**
 * How far through a phase is: the fraction, and a bar to read it at a glance.
 *
 * This is what the roadmap says instead of a target date. Seven of fifteen
 * phases rendered "no target" there, because `target_date` is only reachable
 * through an undocumented field bag and nobody had ever set one — so the column
 * promised a plan that did not exist and said nothing about whether the phase
 * was moving (KEEL-332).
 *
 * A phase with no tasks says so rather than showing `0 / 0` or an empty bar.
 * "Not scoped" is a real and useful state: it is a phase that has been named
 * and not yet broken down, which is different from one that has not started.
 */
export function Progress({ closed, total }: { closed: number; total: number }) {
  if (total === 0) {
    return <span title="No tasks filed under this one yet">not scoped</span>;
  }
  // Clamped. The bar is drawn from a number the daemon sends, and a bar wider
  // than its own track is a rendering artefact that looks like a styling bug
  // rather than the data problem it would actually be.
  const pct = Math.min(100, Math.max(0, Math.round((closed / total) * 100)));
  return (
    <span className="flex items-center gap-2 whitespace-nowrap">
      <span
        aria-hidden
        // Hidden on a narrow window. The bar is decoration — the fraction
        // beside it carries the same fact — and adding 64px to every row is
        // what pushed "Phase 0 — Spine" onto two lines in a half-width window.
        className="hidden h-1.5 w-16 shrink-0 overflow-hidden rounded-full bg-border-subtle md:block"
      >
        <span className="block h-full rounded-full bg-accent" style={{ width: `${pct}%` }} />
      </span>
      {/* The numbers, not the percentage. "29 / 35" says how much is left in the
          unit the board is counted in; "83%" needs the total to be useful and
          the total is the part somebody is about to act on. */}
      <span
        className="tabular whitespace-nowrap"
        title={`${pct}% of the tasks in this one are closed`}
      >
        {closed} / {total}
      </span>
    </span>
  );
}
