/**
 * One layout per kind of artifact.
 *
 * The Library used to render five genuinely different things as one flat list —
 * a type badge, a status badge and a title, in creation order. With forty-five
 * decisions and fifty questions in the store, a decision looked exactly like a
 * spec looked exactly like a design, and there was no way to see the shape of
 * any of them. Design images were invisible text rows.
 *
 * These are the index views. The document reader is unchanged and is still the
 * destination from every row here, so revision history and diff keep working
 * everywhere.
 */

import { Badge, Empty, When, cx, statusTone } from "./ui";
import { href } from "../lib/router";
import type { Entity } from "../lib/api";

export type LibraryType = "spec" | "decision" | "question" | "feedback" | "design";

export const LIBRARY_TYPES: Array<{ id: LibraryType; label: string }> = [
  { id: "spec", label: "Specs" },
  { id: "decision", label: "Decisions" },
  { id: "question", label: "Questions" },
  { id: "feedback", label: "Feedback" },
  { id: "design", label: "Designs" },
];

function label(e: Entity): string {
  return String(e.title ?? e.name ?? e.summary ?? "(unnamed)");
}

function updatedAt(e: Entity): string | undefined {
  const at = e.audit?.updated_at;
  return typeof at === "string" ? at : undefined;
}

/** The reader is the destination from every index row. */
function docHref(project: string | undefined, id: unknown): string {
  return href({ screen: "documents", project, documentId: String(id) });
}

export function LibraryIndex({
  type,
  items,
  project,
}: {
  type: LibraryType;
  items: Entity[];
  project: string | undefined;
}) {
  if (items.length === 0) {
    return (
      <Empty
        message={`No ${LIBRARY_TYPES.find((t) => t.id === type)?.label.toLowerCase() ?? type} yet.`}
        hint="Ask Claude to write one."
      />
    );
  }

  switch (type) {
    case "decision":
      return <DecisionRegister items={items} project={project} />;
    case "question":
      return <QuestionList items={items} project={project} />;
    case "feedback":
      return <FeedbackCards items={items} project={project} />;
    case "design":
      return <DesignGrid items={items} project={project} />;
    default:
      return <SpecList items={items} project={project} />;
  }
}

/**
 * Decisions as a table.
 *
 * A register is something you scan or look up by number, which is the whole
 * point of having numbered them in Phase 7 — and a numbered register rendered
 * as an unordered list of titles throws that away.
 *
 * `supersedes` is shown as the status rather than as a resolved link: the edge
 * lives in the graph and reading it per row would be one request per decision.
 * `superseded` in the status column answers "was this overturned"; which
 * decision replaced it is one click away in the reader.
 */
function DecisionRegister({ items, project }: { items: Entity[]; project: string | undefined }) {
  const sorted = [...items].sort((a, b) => Number(b.number ?? 0) - Number(a.number ?? 0));
  return (
    <table className="w-full border-collapse text-small">
      <thead className="sticky top-0 bg-surface">
        <tr className="border-b border-border-subtle text-left text-micro tracking-wide text-ink-faint uppercase">
          <th className="w-20 px-2 py-1.5">Ref</th>
          <th className="px-2 py-1.5">Decision</th>
          <th className="w-32 px-2 py-1.5">Status</th>
          <th className="w-28 px-2 py-1.5 text-right">Decided</th>
        </tr>
      </thead>
      <tbody>
        {sorted.map((d) => {
          const at = (d.decided_at as string | null) ?? updatedAt(d);
          return (
            <tr key={String(d.id)} className="border-b border-border-subtle/60 hover:bg-surface-hover">
              <td className="px-2 py-1.5 align-top font-mono text-micro text-ink-faint">
                {d.number ? `B-${String(d.number)}` : "—"}
              </td>
              <td className="px-2 py-1.5 align-top">
                <a href={docHref(project, d.id)} className="hover:text-accent">
                  {label(d)}
                </a>
              </td>
              <td className="px-2 py-1.5 align-top">
                <Badge tone={statusTone(String(d.status))}>{String(d.status)}</Badge>
              </td>
              <td className="px-2 py-1.5 text-right align-top text-micro text-ink-faint">
                {at ? <When iso={at} /> : "—"}
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/**
 * Questions, open ones first.
 *
 * Grouped rather than sorted, because "what is still undecided" is a different
 * question from "what did we decide" and a single ordered list makes you do the
 * separating yourself.
 *
 * The answer is not inlined. It lives in the document body, and the list
 * endpoint does not carry bodies — inlining would mean one request per row.
 * Worth revisiting if the API ever grows a bulk body read.
 */
function QuestionList({ items, project }: { items: Entity[]; project: string | undefined }) {
  const open = items.filter((q) => String(q.status) === "open");
  const settled = items.filter((q) => String(q.status) !== "open");

  const section = (heading: string, rows: Entity[], tone: string) =>
    rows.length === 0 ? null : (
      <section className="mb-6">
        <h3 className={cx("mb-2 text-micro font-medium tracking-wide uppercase", tone)}>
          {heading} · {rows.length}
        </h3>
        <ul className="space-y-1">
          {rows.map((q) => (
            <li key={String(q.id)}>
              <a
                href={docHref(project, q.id)}
                className="block rounded-control px-2 py-1.5 text-small hover:bg-surface-hover"
              >
                <span className="mr-2">{label(q)}</span>
                <Badge tone={statusTone(String(q.status))}>{String(q.status)}</Badge>
              </a>
            </li>
          ))}
        </ul>
      </section>
    );

  return (
    <div>
      {section("Open — nothing here is decided", open, "text-bad")}
      {section("Settled", settled, "text-ink-faint")}
    </div>
  );
}

/** Feedback as chronological cards: who said it, when, how they felt. */
function FeedbackCards({ items, project }: { items: Entity[]; project: string | undefined }) {
  const sorted = [...items].sort((a, b) =>
    String(updatedAt(b) ?? "").localeCompare(String(updatedAt(a) ?? "")),
  );
  return (
    <ul className="space-y-2">
      {sorted.map((f) => {
        const at = updatedAt(f);
        return (
          <li key={String(f.id)}>
            <a
              href={docHref(project, f.id)}
              className="block rounded-card border border-border-subtle bg-surface-raised p-3 hover:border-accent/50"
            >
              <div className="flex items-center gap-2">
                {f.source ? <Badge>{String(f.source)}</Badge> : null}
                {f.sentiment ? (
                  <Badge tone={statusTone(String(f.sentiment))}>{String(f.sentiment)}</Badge>
                ) : null}
                {f.kind ? <Badge>{String(f.kind)}</Badge> : null}
                <span className="ml-auto text-micro text-ink-faint">
                  {at ? <When iso={at} /> : null}
                </span>
              </div>
              <p className="mt-1.5 text-small">{label(f)}</p>
            </a>
          </li>
        );
      })}
    </ul>
  );
}

/**
 * Designs as pictures.
 *
 * You look at a design; you do not read its title. `blob_id` is on the entity
 * itself, so the whole grid is one list call rather than one request per tile.
 */
function DesignGrid({ items, project }: { items: Entity[]; project: string | undefined }) {
  return (
    <ul className="grid grid-cols-2 gap-3 lg:grid-cols-3">
      {items.map((d) => {
        const blob = d.blob_id as string | null;
        return (
          <li key={String(d.id)}>
            <a
              href={docHref(project, d.id)}
              className="block overflow-hidden rounded-card border border-border-subtle bg-surface-raised hover:border-accent/50"
            >
              <div className="flex aspect-video items-center justify-center bg-surface-sunken">
                {blob ? (
                  // No `loading="lazy"`. It left `currentSrc` empty and the
                  // fetch unstarted even with the tile in the viewport, so the
                  // grid rendered empty boxes over a blob endpoint that was
                  // answering 200. At this scale deferring a handful of
                  // thumbnails buys nothing worth a rendering quirk; if the
                  // store ever holds hundreds, bring it back with a
                  // measurement.
                  <img src={`/api/blob/${blob}`} alt={label(d)} className="h-full w-full object-contain" />
                ) : (
                  <span className="text-micro text-ink-faint">no image</span>
                )}
              </div>
              <div className="flex items-center gap-2 p-2">
                <span className="truncate text-small">{label(d)}</span>
                {d.state ? (
                  <Badge tone={statusTone(String(d.state))}>{String(d.state)}</Badge>
                ) : null}
              </div>
            </a>
          </li>
        );
      })}
    </ul>
  );
}

/** Specs: long prose, read whole. A list of titles is the right shape. */
function SpecList({ items, project }: { items: Entity[]; project: string | undefined }) {
  return (
    <ul className="space-y-1">
      {items.map((s) => {
        const at = updatedAt(s);
        return (
          <li key={String(s.id)}>
            <a
              href={docHref(project, s.id)}
              className="flex items-center gap-2 rounded-control px-2 py-1.5 text-small hover:bg-surface-hover"
            >
              <span className="truncate">{label(s)}</span>
              {s.status ? (
                <Badge tone={statusTone(String(s.status))}>{String(s.status)}</Badge>
              ) : null}
              <span className="ml-auto shrink-0 text-micro text-ink-faint">
                {at ? <When iso={at} /> : null}
              </span>
            </a>
          </li>
        );
      })}
    </ul>
  );
}
