/**
 * Screen 3 — Roadmap. Milestones over time, one project or all.
 *
 * Built from milestones because that is what they are for: SPEC §6 calls them
 * the planning unit and says the roadmap view is built from them. Nothing here
 * infers a timeline from task dates.
 *
 * **Two strands, not one list.** A milestone is either a phase — the plan — or
 * a release, which is what actually went out, and they answer different
 * questions. Interleaving them by date reads badly here: the first ten phases
 * finished inside three days and the ten releases all landed the week after, so
 * one chronological list buries the plan in the middle of a changelog. The
 * phases keep their manual order; the releases go in the order they shipped.
 */

import { api, type Entity, type Page as PageOf } from "../lib/api";
import { href } from "../lib/router";
import { useAsync } from "../lib/useAsync";
import {
  Badge,
  Empty,
  ErrorBox,
  Spinner,
  When,
  statusTone,
} from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

/** The order a human asked for, then a date, then the name. */
function byPlan(a: Entity, b: Entity): number {
  // `sort_order` first, because SPEC §3.2 gives milestones that column
  // specifically for "manual ordering for the roadmap view" — a human who
  // has said what order they want should get it.
  const ao = a.sort_order as number | null;
  const bo = b.sort_order as number | null;
  if (ao != null && bo != null && ao !== bo) return ao - bo;
  if (ao != null && bo == null) return -1;
  if (ao == null && bo != null) return 1;

  // Then by target date, for the rare phase that has one. Dated before undated,
  // since a milestone with no target is unplanned rather than far-future. This
  // is close to dead code — four rows in this store carry a date and all four
  // say the day it was seeded — but a date somebody does set should still order
  // the roadmap (KEEL-332).
  const at = a.target_date as string | null;
  const bt = b.target_date as string | null;
  if (at && bt && at !== bt) return at.localeCompare(bt);
  if (at && !bt) return -1;
  if (!at && bt) return 1;

  // Finally by name, so ties never fall back to insertion order. Without this
  // the four phases that shipped on the same day came back newest first, so the
  // roadmap read 3, 2, 1, 0.
  return String(a.name).localeCompare(String(b.name), undefined, {
    numeric: true,
  });
}

/**
 * When it shipped, oldest first.
 *
 * A release's date is the one fact about it that cannot be wrong, so it beats
 * the manual ordering here — a version cut without a `sort_order` still lands
 * in the right place, which matters because the next one will be created by
 * whoever is cutting it rather than by a backfill that thought about ordering.
 */
function byShipped(a: Entity, b: Entity): number {
  const as = a.shipped_at as string | null;
  const bs = b.shipped_at as string | null;
  if (as && bs && as !== bs) return as.localeCompare(bs);
  if (as && !bs) return -1;
  if (!as && bs) return 1;
  return byPlan(a, b);
}

export function RoadmapScreen({
  route,
  generation,
  milestoneNoun,
}: ScreenProps) {
  const noun = milestoneNoun ?? "milestone";
  const plural = `${noun.toLowerCase()}s`;
  const project = route.project;
  const { data, error, loading, reload } = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "milestone" }),
    [project, generation],
  );

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page
        title="Roadmap"
        crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}
      >
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const all = data?.items ?? [];
  const phases = all.filter((m) => String(m.kind) !== "release").sort(byPlan);
  const releases = all
    .filter((m) => String(m.kind) === "release")
    .sort(byShipped);

  return (
    <Page
      title="Roadmap"
      crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}
      meta={
        <span className="text-small text-ink-faint">
          {project ? project : "all projects"}
        </span>
      }
    >
      {all.length === 0 ? (
        <Empty
          message={`No ${plural} yet.`}
          hint={`${noun[0]?.toUpperCase()}${noun.slice(1).toLowerCase()}s are what the roadmap is built from.`}
        />
      ) : (
        <>
          {phases.length > 0 ? (
            <Strand items={phases} noun={noun} project={project} />
          ) : null}
          {releases.length > 0 ? (
            <>
              <h2 className="mt-8 mb-3 font-medium">Released</h2>
              <Strand items={releases} noun={noun} project={project} />
            </>
          ) : null}
        </>
      )}
    </Page>
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

/** One vertical run of milestone rows against a timeline rule. */
function Strand({
  items,
  noun,
  project,
}: {
  items: Entity[];
  noun: string;
  project?: string;
}) {
  return (
    <ol className="relative space-y-3 border-l border-border-subtle pl-6">
      {items.map((m) => (
        <Row key={String(m.id)} m={m} noun={noun} project={project} />
      ))}
    </ol>
  );
}

function Row({
  m,
  noun,
  project,
}: {
  m: Entity;
  noun: string;
  project?: string;
}) {
  // `state`, not `status`. The column only holds what was declared — shipped,
  // cut, paused, or nothing at all — and what the phase is actually doing is
  // worked out from its tasks by the daemon (B-57). Reading `status` here is
  // what showed a finished phase as active for a week. Falling back keeps an
  // older daemon readable.
  const status = String(m.state ?? m.status);
  const date = m.target_date as string | null;
  const shipped = m.shipped_at as string | null;
  // What a row says on the right is decided by what it *is*, not by whether it
  // happens to carry a date. Keying this off `shipped_at` meant the eight
  // phases that have one showed no progress at all — the thing this column
  // exists to show, hidden on more than half the phases — while an unshipped
  // release fell through to the progress branch and was labelled "not scoped",
  // the exact false claim the rest of this file is careful to avoid.
  const isRelease = String(m.kind) === "release";
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
  const closed =
    typeof m.tasks_closed === "number" ? (m.tasks_closed as number) : 0;
  const moved = m.last_activity as string | null;

  return (
    <li className="relative">
      <span
        className="absolute top-4 -left-[26px] h-2.5 w-2.5 rounded-full ring-4 ring-surface"
        style={{
          background:
            status === "shipped"
              ? "var(--color-good)"
              : status === "active"
                ? "var(--color-warn)"
                : status === "blocked"
                  ? "var(--color-bad)"
                  : "var(--color-border-subtle)",
        }}
      />
      <div className="rounded-card border border-border-subtle bg-surface-raised px-4 py-3">
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
          {String(m.kind) === "release" && m.version_string ? (
            <Badge>v{String(m.version_string)}</Badge>
          ) : null}
          <span className="ml-auto flex items-center gap-3 text-small text-ink-faint">
            {isRelease ? (
              // A release carries no tasks, so its date is the whole story. An
              // undated one is a version somebody has named and not yet cut.
              shipped ? (
                <When iso={shipped} prefix="shipped" />
              ) : (
                <span title="Named, but not cut yet">unreleased</span>
              )
            ) : (
              <>
                {/* A target is a future date, which is exactly the case the old
                    helper rendered as "-3d ago". Kept for a phase that
                    genuinely has one; it is no longer what the column falls
                    back to. */}
                {/* A target only while it is still a target. On a phase
                    that shipped it is history, and printing "due Aug 9"
                    beside "shipped Aug 9" is two dates where one is the
                    answer. */}
                {!shipped && date ? <Due iso={date} /> : null}
                {counted ? <Progress closed={closed} total={total} /> : null}
                {/* When it shipped beats when it last moved: a finished phase
                    someone left a note on last week did not move last week in
                    any sense a reader cares about. */}
                {shipped ? (
                  <When iso={shipped} prefix="shipped" />
                ) : moved ? (
                  <When iso={moved} prefix="moved" />
                ) : null}
              </>
            )}
          </span>
        </div>
        {m.summary ? (
          <p className="selectable mt-1 text-small text-ink-muted">
            {String(m.summary)}
          </p>
        ) : null}
      </div>
    </li>
  );
}

/**
 * How far through a phase is: the fraction, and a bar to read it at a glance.
 *
 * This is what the roadmap's right-hand column says instead of a target date.
 * Seven of fifteen phases rendered "no target" there, because `target_date` is
 * only reachable through an undocumented field bag and nobody had ever set one
 * — so the column promised a plan that did not exist and said nothing about
 * whether the phase was moving (KEEL-332).
 *
 * A phase with no tasks says so rather than showing `0 / 0` or an empty bar.
 * "Not scoped" is a real and useful state: it is a phase that has been named
 * and not yet broken down, which is different from one that has not started.
 */
function Progress({ closed, total }: { closed: number; total: number }) {
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
        // what pushed "Phase 0 — Spine" onto two lines in a half-width
        // window. Information first, then the picture of it.
        className="hidden h-1.5 w-16 shrink-0 overflow-hidden rounded-full bg-border-subtle md:block"
      >
        <span
          className="block h-full rounded-full bg-accent"
          style={{ width: `${pct}%` }}
        />
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
