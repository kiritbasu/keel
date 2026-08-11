/**
 * Screen 3 — Roadmap. Milestones over time, one project or all.
 *
 * Built from milestones because that is what they are for: SPEC §6 calls them
 * the planning unit and says the roadmap view is built from them. Nothing here
 * infers a timeline from task dates.
 */

import { api, type Entity, type Page as PageOf } from "../lib/api";
import { href } from "../lib/router";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Spinner, When, statusTone } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

export function RoadmapScreen({ route, generation, milestoneNoun }: ScreenProps) {
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
      <Page title="Roadmap" crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}>
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const milestones = (data?.items ?? []).slice().sort((a, b) => {
    // `sort_order` first, because SPEC §3.2 gives milestones that column
    // specifically for "manual ordering for the roadmap view" — a human who
    // has said what order they want should get it.
    const ao = a.sort_order as number | null;
    const bo = b.sort_order as number | null;
    if (ao != null && bo != null && ao !== bo) return ao - bo;
    if (ao != null && bo == null) return -1;
    if (ao == null && bo != null) return 1;

    // Then by target date. Dated before undated, since a milestone with no
    // target is unplanned rather than far-future.
    const at = a.target_date as string | null;
    const bt = b.target_date as string | null;
    if (at && bt && at !== bt) return at.localeCompare(bt);
    if (at && !bt) return -1;
    if (!at && bt) return 1;

    // Finally by name, so ties never fall back to insertion order. Without
    // this the four phases that shipped on the same day came back newest
    // first, so the roadmap read 3, 2, 1, 0.
    return String(a.name).localeCompare(String(b.name), undefined, { numeric: true });
  });

  return (
    <Page
      title="Roadmap"
      crumbs={project ? projectCrumbs(route, "Roadmap") : undefined}
      meta={<span className="text-small text-ink-faint">{project ? project : "all projects"}</span>}
    >
      {milestones.length === 0 ? (
        <Empty
          message={`No ${plural} yet.`}
          hint={`${noun[0]?.toUpperCase()}${noun.slice(1).toLowerCase()}s are what the roadmap is built from.`}
        />
      ) : (
        <ol className="relative space-y-3 border-l border-border-subtle pl-6">
          {milestones.map((m) => {
            const status = String(m.status);
            const date = m.target_date as string | null;
            const shipped = m.shipped_at as string | null;
            return (
              <li key={m.id} className="relative">
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
                    {/* A milestone on the roadmap and a chip on a card describe
                        the same thing and used not to know about each other. */}
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
                    <span className="ml-auto text-small text-ink-faint">
                      {shipped ? (
                        <When iso={shipped} prefix="shipped" />
                      ) : date ? (
                        // A target is a future date, which is exactly the case
                        // the old helper rendered as "-3d ago".
                        <When iso={new Date(date).toISOString()} prefix="due" />
                      ) : (
                        "no target"
                      )}
                    </span>
                  </div>
                  {m.summary ? (
                    <p className="selectable mt-1 text-small text-ink-muted">{String(m.summary)}</p>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </Page>
  );
}
