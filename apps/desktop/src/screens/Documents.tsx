/**
 * Screen 5 — Documents. Reader, revision history, side-by-side diff, and the
 * link graph for the current document.
 *
 * The diff is the reason this screen exists rather than being a markdown
 * viewer. "What changed in this spec between v3 and v7, and who changed it" is
 * a question the folder-of-markdown-files arrangement could never answer, and
 * it is most of why prose is versioned at all.
 *
 * Which document, which revision and which comparison all live in the address,
 * so the answer to that question is something you can send to someone.
 */

import { useState } from "react";
import { api, type Entity, type Neighbour, type Page as PageOf } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import {
  Badge,
  Chip,
  Empty,
  ErrorBox,
  Id,
  Input,
  Menu,
  MenuItem,
  Spinner,
  cx,
  exactly,
  statusTone,
  when,
} from "../components/ui";
import { Markdown } from "../components/Markdown";
import { LIBRARY_TYPES, LibraryIndex, type LibraryType } from "../components/LibraryIndex";
import { Page, projectCrumbs } from "../components/Page";
import { href, navigate, setQuery } from "../lib/router";
import type { ScreenProps } from "../App";

/** The five prose-bearing types, in the order they are usually read. */
const PROSE_TYPES = "spec,decision,question,feedback,design";

function asVersion(value: string | undefined): number | undefined {
  if (!value) return undefined;
  const parsed = Number(value);
  return Number.isInteger(parsed) && parsed > 0 ? parsed : undefined;
}

export function DocumentsScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const selected = route.documentId ?? null;
  // Which kind you are looking at lives in the address, so a link to the
  // decision register is a link rather than a place you have to navigate to.
  const kind = (LIBRARY_TYPES.find((t) => t.id === route.query.kind)?.id ??
    "spec") as LibraryType;
  const [filter, setFilter] = useState("");
  const version = asVersion(route.query.v);
  const compare = asVersion(route.query.diff);

  const list = useAsync<PageOf<Entity>>(
    () => api.entities({ project, type: PROSE_TYPES, limit: 500 }),
    [project, generation],
  );

  const doc = useAsync(
    async () => (selected ? api.document(selected, version, compare) : null),
    [selected, version, compare, generation],
  );

  const graph = useAsync<{ neighbours: Neighbour[] } | null>(
    async () => (selected ? api.graph(selected, "both", 1) : null),
    [selected, generation],
  );

  // A design's image lives on the entity, not on the document, so it needs its
  // own read. Swallowing the failure is right here and only here: a missing
  // image should cost the image, not the page — the caption and the revision
  // history are still worth showing.
  const entity = useAsync(
    async () => (selected ? api.entity(selected).catch(() => null) : null),
    [selected, generation],
  );
  const blobId = entity.data?.artifacts?.[0]?.entity?.blob_id;
  const imageSrc = typeof blobId === "string" ? `/api/blob/${blobId}` : null;

  if (list.loading && !list.data) return <Spinner />;
  if (list.error) {
    return (
      <Page title="Documents" crumbs={projectCrumbs(route, "Documents")}>
        <ErrorBox error={list.error} retry={list.reload} />
      </Page>
    );
  }

  const documents = list.data?.items ?? [];
  const ofKind = documents.filter((d) => String(d.type) === kind);
  const needle = filter.trim().toLowerCase();
  // Filter narrows what is already on screen, instantly and literally. Search
  // queries the whole store and understands meaning. The words have to make
  // that difference obvious — see C6.
  const shown = needle
    ? ofKind.filter((d) =>
        String(d.title ?? d.name ?? d.summary ?? "").toLowerCase().includes(needle),
      )
    : ofKind;
  const current = doc.data?.document;
  const revisions = doc.data?.revisions ?? [];
  const showing = version ?? current?.version;

  return (
    <Page
      title="Library"
      crumbs={projectCrumbs(route, "Library")}
      width="full"
      toolbar={
        <div className="flex flex-wrap items-center gap-1.5">
          {LIBRARY_TYPES.map((t) => {
            const count = documents.filter((d) => String(d.type) === t.id).length;
            return (
              <Chip
                key={t.id}
                selected={kind === t.id}
                onClick={() => {
                  setFilter("");
                  navigate({ screen: "documents", project, query: { kind: t.id } });
                }}
              >
                {t.label}
                <span className="ml-1 tabular-nums text-ink-faint">{count}</span>
              </Chip>
            );
          })}
        </div>
      }
    >
      <div className="flex h-full">
        <aside className="flex w-72 shrink-0 flex-col border-r border-border-subtle p-3">
          <Input
            variant="sm"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder={`Filter ${ofKind.length} ${ofKind.length === 1 ? "document" : "documents"}`}
            aria-label="Filter this list"
            className="mb-2"
          />
          {shown.length === 0 ? (
            <p className="px-2 py-1 text-small text-ink-faint">
              {ofKind.length === 0 ? "None of this kind yet." : "Nothing matches that."}
            </p>
          ) : (
            <ul className="min-h-0 flex-1 space-y-0.5 overflow-y-auto">
              {shown.map((d) => {
                const label = String(d.title ?? d.name ?? d.summary ?? "(unnamed)");
                return (
                  <li key={d.id}>
                    <a
                      href={href({ screen: "documents", project, documentId: d.id })}
                      className={cx(
                        "block w-full rounded px-2 py-1.5 text-left",
                        selected === d.id ? "bg-surface-hover" : "hover:bg-surface-hover",
                      )}
                    >
                      {/* No type badge: the switcher above already says which
                          kind this list is, so repeating it on every row is
                          the same word sixty times. */}
                      <div className="truncate text-small">{label}</div>
                      {d.status ? (
                        <div className="mt-1">
                          <Badge tone={statusTone(String(d.status))}>{String(d.status)}</Badge>
                        </div>
                      ) : null}
                    </a>
                  </li>
                );
              })}
            </ul>
          )}
        </aside>

        <div className="min-w-0 flex-1 overflow-y-auto">
          {!selected ? (
            // Nothing picked: the index for this kind, laid out the way this
            // kind is actually read. The reader takes the pane the moment
            // something is selected, so nothing about it changes.
            <div className="p-6">
              <LibraryIndex type={kind} items={shown} project={project} />
            </div>
          ) : doc.loading && !doc.data ? (
            <Spinner />
          ) : doc.error ? (
            <div className="p-6">
              <ErrorBox error={doc.error} retry={doc.reload} />
            </div>
          ) : (
            <article className="mx-auto max-w-4xl p-6 pb-24">
              <header className="mb-4">
                <h1 className="text-title font-semibold tracking-tight">{current?.title ?? "Untitled"}</h1>
                <div className="mt-2 flex flex-wrap items-center gap-2 text-small text-ink-faint">
                  <Id value={selected} />
                  {current && (
                    <>
                      <span>·</span>
                      <span>
                        revision {current.version}, {when(current.created_at)} by {current.author}
                      </span>
                    </>
                  )}
                </div>
              </header>

              {revisions.length > 1 && (
                <div className="mb-5 flex flex-wrap items-center gap-2 rounded-lg border border-border-subtle bg-surface-raised px-3 py-2">
                  {/* A menu rather than two rows of chips. A document with a
                      dozen revisions produced a dozen chips of identical weight,
                      then a dozen more beside them for the comparison — twenty-four
                      controls to answer a two-part question. */}
                  <Menu label={`Revision v${showing ?? "?"}`}>
                    {(close) =>
                      revisions.map((r) => (
                        <MenuItem
                          key={r.version}
                          selected={showing === r.version}
                          title={`${r.author}${r.session_id ? ` · ${r.session_id}` : ""} · ${exactly(r.created_at)}`}
                          onClick={() => {
                            close();
                            setQuery(route, { v: String(r.version) });
                          }}
                        >
                          v{r.version} — {r.author}, {when(r.created_at)}
                        </MenuItem>
                      ))
                    }
                  </Menu>

                  <Menu label={compare ? `Compare with v${compare}` : "Compare with…"}>
                    {(close) => (
                      <>
                        <MenuItem
                          selected={!compare}
                          onClick={() => {
                            close();
                            setQuery(route, { diff: undefined });
                          }}
                        >
                          No comparison
                        </MenuItem>
                        {revisions
                          .filter((r) => r.version !== showing)
                          .map((r) => (
                            <MenuItem
                              key={r.version}
                              selected={compare === r.version}
                              onClick={() => {
                                close();
                                setQuery(route, { diff: String(r.version) });
                              }}
                            >
                              v{r.version} — {r.author}, {when(r.created_at)}
                            </MenuItem>
                          ))}
                      </>
                    )}
                  </Menu>
                </div>
              )}

              {doc.data?.diff ? (
                <section className="mb-6">
                  <h2 className="mb-2 text-small font-semibold tracking-wide text-ink-muted uppercase">
                    v{doc.data.diff.from_version} → v{doc.data.diff.to_version}
                    <span className="ml-2 font-normal text-good">+{doc.data.diff.added}</span>
                    <span className="ml-1.5 font-normal text-bad">−{doc.data.diff.removed}</span>
                  </h2>
                  <pre className="selectable overflow-x-auto rounded-lg border border-border-subtle bg-surface-raised p-3 font-mono text-small leading-relaxed">
                    {doc.data.diff.unified.split("\n").map((line, i) => (
                      <div
                        key={i}
                        className={cx(
                          line.startsWith("+") && "bg-good/10 text-good",
                          line.startsWith("-") && "bg-bad/10 text-bad",
                          !line.startsWith("+") && !line.startsWith("-") && "text-ink-muted",
                        )}
                      >
                        {line || " "}
                      </div>
                    ))}
                  </pre>
                </section>
              ) : null}

              {imageSrc && (
                <figure className="mb-6">
                  <img
                    src={imageSrc}
                    alt={current?.title ?? "Design"}
                    className="max-h-[70vh] w-auto max-w-full rounded-lg border border-border-subtle bg-surface-raised"
                  />
                </figure>
              )}

              {current ? (
                <Markdown>{current.body}</Markdown>
              ) : imageSrc ? null : (
                <Empty
                  message="Nothing has been written here yet."
                  hint="Ask Claude to write it."
                />
              )}

              {(graph.data?.neighbours.length ?? 0) > 0 && (
                <section className="mt-8 border-t border-border-subtle pt-4">
                  <h2 className="mb-2 text-small font-semibold tracking-wide text-ink-muted uppercase">
                    Connected
                  </h2>
                  <ul className="space-y-1.5 text-small">
                    {graph.data?.neighbours.map((n) => (
                      <li key={`${n.id}-${n.rel}`}>
                        <button
                          type="button"
                          onClick={() =>
                            navigate({ screen: "documents", project, documentId: n.id })
                          }
                          className="flex items-center gap-2 rounded hover:underline"
                        >
                          <Badge>{n.rel}</Badge>
                          <Badge>{n.entity_type}</Badge>
                          {/* The traversal carries the label now, so this reads
                              as a name. It showed a ULID because the id was all
                              a neighbour used to have. */}
                          <span className="min-w-0 truncate">{n.label || <Id value={n.id} />}</span>
                          {n.anchor && <Badge tone="border-accent/40 text-accent">{n.anchor}</Badge>}
                        </button>
                      </li>
                    ))}
                  </ul>
                </section>
              )}
            </article>
          )}
        </div>
      </div>
    </Page>
  );
}
