/**
 * Screen 6 — Releases. What actually went out, and when.
 *
 * A screen of its own rather than a section of the roadmap (KEEL-336). A phase
 * and a release are different nouns: a phase is a unit of plan that holds tasks
 * and has progress, a release is a unit of record that went out on a date and
 * holds nothing. One list containing both implied a relationship neither has to
 * the other, and a release row rendered in a column about task progress could
 * only ever say "not scoped".
 *
 * **A table, not cards.** Ten versions of one product differ in their version
 * and their date and almost nothing else, so the useful shape is a column of
 * versions you can run your eye down — not ten cards of identical furniture.
 * That was the second thing wrong with them living on the roadmap: they had
 * been given a phase's clothes.
 *
 * Newest first, which is the opposite of the roadmap and deliberate. A plan is
 * read forwards from where you are; a changelog is read backwards from now.
 */

import { api, type Entity, type Page as PageOf } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Empty, ErrorBox, Spinner, When } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import type { ScreenProps } from "../App";

/** Newest first, with anything not yet cut at the top. */
export function byShippedDesc(a: Entity, b: Entity): number {
  const as = a.shipped_at as string | null;
  const bs = b.shipped_at as string | null;
  // An uncut version is the *next* one, so it belongs above everything that has
  // already gone out — the one place on this screen where "no date" means the
  // future rather than the unknown. The roadmap's table sorts oldest-first and
  // puts it last, which is the same rule read the other way up.
  if (!as && bs) return -1;
  if (as && !bs) return 1;
  if (as && bs && as !== bs) return bs.localeCompare(as);
  return String(b.name).localeCompare(String(a.name), undefined, {
    numeric: true,
  });
}

/**
 * The version a row carries, or null.
 *
 * Null rather than falling back to the name. The name is the fallback for the
 * *prose* column, and using it for both put the same string in two cells of one
 * row — which reads as a rendering bug rather than as a missing version.
 */
function versionOf(m: Entity): string | null {
  const v = m.version_string as string | null;
  return v && v.trim() !== "" ? v : null;
}

/**
 * The prose, with the version stripped off the front when it repeats.
 *
 * Every release in this store is named "0.3.0 — what to pick up next", so
 * printing the name beside the version column says the version twice. The
 * separator is an em dash because that is what the backfill wrote; a name that
 * does not follow the convention is shown whole rather than cut at a guess.
 */
export function describe(m: Entity): string {
  const name = String(m.name);
  const version = (m.version_string as string | null)?.trim();
  if (version && name.startsWith(version)) {
    const rest = name.slice(version.length).replace(/^\s*[—–-]\s*/, "");
    if (rest !== "") return rest;
  }
  return name;
}

export function ReleasesScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const { data, error, loading, reload } = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: "milestone", limit: 500 }),
    [project, generation],
  );

  if (loading && !data) return <Spinner />;
  if (error) {
    return (
      <Page
        title="Releases"
        crumbs={project ? projectCrumbs(route, "Releases") : undefined}
      >
        <ErrorBox error={error} retry={reload} />
      </Page>
    );
  }

  const releases = (data?.items ?? [])
    .filter((m) => String(m.kind) === "release")
    .sort(byShippedDesc);
  // The newest *shipped* one. An uncut version sorts to the top of the list
  // and must not be announced as the current release.
  const latest = releases.find((m) => m.shipped_at);

  return (
    <Page
      title="Releases"
      crumbs={project ? projectCrumbs(route, "Releases") : undefined}
      meta={
        <span className="text-small text-ink-faint">
          {releases.length} {releases.length === 1 ? "version" : "versions"}
          {latest && versionOf(latest) ? ` · latest v${versionOf(latest)}` : ""}
        </span>
      }
    >
      {releases.length === 0 ? (
        <Empty
          message="Nothing has shipped yet."
          hint="A release is a milestone with kind `release` and a version string. Cutting one is a task like any other — create the row before the tag is pushed."
        />
      ) : (
        <table className="w-full border-collapse">
          <thead>
            <tr className="border-b border-border-subtle text-left">
              <Th className="w-28">Version</Th>
              <Th>What went out</Th>
              <Th className="w-28 text-right">Shipped</Th>
            </tr>
          </thead>
          <tbody>
            {releases.map((m) => (
              <tr
                key={String(m.id)}
                className="align-baseline hover:bg-surface-hover"
              >
                <td className="py-2 pr-3 pl-2 font-mono text-small whitespace-nowrap">
                  {versionOf(m) ?? <span className="text-ink-faint">—</span>}
                </td>
                <td className="selectable py-2 pr-3">
                  <span className="text-small">{describe(m)}</span>
                  {m.summary ? (
                    <p className="mt-0.5 text-small text-ink-muted">
                      {String(m.summary)}
                    </p>
                  ) : null}
                </td>
                <td className="py-2 pr-2 text-right text-small whitespace-nowrap text-ink-faint">
                  {m.shipped_at ? (
                    <When iso={String(m.shipped_at)} />
                  ) : (
                    <span title="Named, but not cut yet">unreleased</span>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Page>
  );
}

function Th({
  children,
  className = "",
}: {
  children: React.ReactNode;
  className?: string;
}) {
  return (
    <th
      className={`py-2 pr-3 pl-2 text-micro font-medium tracking-wider text-ink-faint uppercase ${className}`}
    >
      {children}
    </th>
  );
}
