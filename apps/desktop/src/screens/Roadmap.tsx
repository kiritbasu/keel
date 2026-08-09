/**
 * Screen 3 — Roadmap. Milestones over time, one project or all.
 *
 * Built from milestones because that is what they are for: SPEC §6 calls them
 * the planning unit and says the roadmap view is built from them. Nothing here
 * infers a timeline from task dates.
 */

import { api, type Entity, type Page } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Spinner, statusTone } from "../components/ui";
import type { ScreenProps } from "../App";

export function RoadmapScreen({ project, generation }: ScreenProps) {
  const { data, error, loading, reload } = useAsync<Page<Entity>>(
    () => api.entities({ project, type: "milestone" }),
    [project, generation],
  );

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <div className="p-6">
        <ErrorBox error={error} retry={reload} />
      </div>
    );
  }

  const milestones = (data?.items ?? []).slice().sort((a, b) => {
    // Dated first, in date order; undated after, since a milestone with no
    // target is not "far future", it is unplanned.
    const at = a.target_date as string | null;
    const bt = b.target_date as string | null;
    if (at && bt) return at.localeCompare(bt);
    if (at) return -1;
    if (bt) return 1;
    return String(a.name).localeCompare(String(b.name));
  });

  return (
    <div className="mx-auto max-w-4xl space-y-5 p-6">
      <header className="flex items-baseline justify-between">
        <h1 className="text-xl font-semibold tracking-tight">Roadmap</h1>
        <span className="text-[12px] text-ink-faint">
          {project ? project : "all projects"}
        </span>
      </header>

      {milestones.length === 0 ? (
        <Empty message="No milestones yet." hint="Milestones are what the roadmap is built from." />
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
                <div className="rounded-lg border border-border-subtle bg-surface-raised px-4 py-3">
                  <div className="flex items-center gap-2">
                    <span className="font-medium">{String(m.name)}</span>
                    <Badge tone={statusTone(status)}>{status}</Badge>
                    {String(m.kind) === "release" && m.version_string ? (
                      <Badge>v{String(m.version_string)}</Badge>
                    ) : null}
                    <span className="ml-auto text-[12px] text-ink-faint">
                      {shipped
                        ? `shipped ${new Date(shipped).toLocaleDateString()}`
                        : date
                          ? `target ${date}`
                          : "no target"}
                    </span>
                  </div>
                  {m.summary ? (
                    <p className="selectable mt-1 text-[13px] text-ink-muted">{String(m.summary)}</p>
                  ) : null}
                </div>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
