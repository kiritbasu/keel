/**
 * Screen 5 — Documents. Reader, revision history, side-by-side diff, and the
 * link graph for the current document.
 *
 * The diff is the reason this screen exists rather than being a markdown
 * viewer. "What changed in this spec between v3 and v7, and who changed it" is
 * a question the folder-of-markdown-files arrangement could never answer, and
 * it is most of why prose is versioned at all.
 */

import { useEffect, useState } from "react";
import { api, type Entity, type Neighbour, type Page } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Id, Spinner, cx, statusTone, when } from "../components/ui";
import type { ScreenProps } from "../App";

/** The five prose-bearing types, in the order they are usually read. */
const PROSE_TYPES = "spec,decision,question,feedback,design";

export function DocumentsScreen({ project, generation }: ScreenProps) {
  const [selected, setSelected] = useState<string | null>(null);
  const [version, setVersion] = useState<number | undefined>();
  const [compare, setCompare] = useState<number | undefined>();

  const list = useAsync<Page<Entity>>(
    () => api.entities({ project, type: PROSE_TYPES, limit: 500 }),
    [project, generation],
  );

  // Selecting a different document must clear the version state, or the reader
  // asks for revision 7 of a document that has three.
  useEffect(() => {
    setVersion(undefined);
    setCompare(undefined);
  }, [selected]);

  const doc = useAsync(
    async () => (selected ? api.document(selected, version, compare) : null),
    [selected, version, compare, generation],
  );

  const graph = useAsync<{ neighbours: Neighbour[] } | null>(
    async () => (selected ? api.graph(selected, "both", 1) : null),
    [selected, generation],
  );

  if (list.loading && !list.data) return <Spinner />;
  if (list.error) {
    return (
      <div className="p-6">
        <ErrorBox error={list.error} retry={list.reload} />
      </div>
    );
  }

  const documents = list.data?.items ?? [];

  return (
    <div className="flex h-full">
      <aside className="w-72 shrink-0 overflow-y-auto border-r border-border-subtle p-3">
        {documents.length === 0 ? (
          <Empty message="No documents." hint="Specs, decisions, questions and feedback appear here." />
        ) : (
          <ul className="space-y-0.5">
            {documents.map((d) => {
              const label = String(d.title ?? d.name ?? d.summary ?? "(unnamed)");
              return (
                <li key={d.id}>
                  <button
                    onClick={() => setSelected(d.id)}
                    className={cx(
                      "w-full rounded px-2 py-1.5 text-left",
                      selected === d.id ? "bg-surface-hover" : "hover:bg-surface-hover",
                    )}
                  >
                    <div className="flex items-center gap-1.5">
                      <Badge>{String(d.type)}</Badge>
                      {d.status ? <Badge tone={statusTone(String(d.status))}>{String(d.status)}</Badge> : null}
                    </div>
                    <div className="mt-1 truncate text-[13px]">{label}</div>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </aside>

      <div className="flex-1 overflow-y-auto">
        {!selected ? (
          <Empty message="Pick a document." />
        ) : doc.loading && !doc.data ? (
          <Spinner />
        ) : doc.error ? (
          <div className="p-6">
            <ErrorBox error={doc.error} retry={doc.reload} />
          </div>
        ) : (
          <article className="mx-auto max-w-3xl p-6">
            <header className="mb-4">
              <h1 className="text-xl font-semibold tracking-tight">
                {doc.data?.document?.title ?? "Untitled"}
              </h1>
              <div className="mt-2 flex flex-wrap items-center gap-2 text-[12px] text-ink-faint">
                <Id value={selected} />
                {doc.data?.document && (
                  <>
                    <span>·</span>
                    <span>
                      revision {doc.data.document.version}, {when(doc.data.document.created_at)} by{" "}
                      {doc.data.document.author}
                    </span>
                  </>
                )}
              </div>
            </header>

            {(doc.data?.revisions.length ?? 0) > 1 && (
              <div className="mb-5 flex flex-wrap items-center gap-2 rounded-lg border border-border-subtle bg-surface-raised px-3 py-2">
                <span className="text-[12px] text-ink-muted">Revisions:</span>
                {doc.data?.revisions.map((r) => (
                  <button
                    key={r.version}
                    onClick={() => setVersion(r.version)}
                    title={`${r.author}${r.session_id ? ` · ${r.session_id}` : ""} · ${new Date(r.created_at).toLocaleString()}`}
                    className={cx(
                      "rounded border px-2 py-0.5 text-[12px] tabular-nums",
                      (version ?? doc.data?.document?.version) === r.version
                        ? "border-accent/50 bg-accent/10 text-accent"
                        : "border-border-subtle text-ink-muted hover:bg-surface-hover",
                    )}
                  >
                    v{r.version}
                  </button>
                ))}
                <span className="ml-2 text-[12px] text-ink-muted">compare with:</span>
                {doc.data?.revisions
                  .filter((r) => r.version !== (version ?? doc.data?.document?.version))
                  .map((r) => (
                    <button
                      key={r.version}
                      onClick={() => setCompare((c) => (c === r.version ? undefined : r.version))}
                      className={cx(
                        "rounded border px-2 py-0.5 text-[12px] tabular-nums",
                        compare === r.version
                          ? "border-warn/50 bg-warn/10 text-warn"
                          : "border-border-subtle text-ink-faint hover:bg-surface-hover",
                      )}
                    >
                      v{r.version}
                    </button>
                  ))}
              </div>
            )}

            {doc.data?.diff ? (
              <section className="mb-6">
                <h2 className="mb-2 text-[13px] font-semibold tracking-wide text-ink-muted uppercase">
                  v{doc.data.diff.from_version} → v{doc.data.diff.to_version}
                  <span className="ml-2 font-normal text-good">+{doc.data.diff.added}</span>
                  <span className="ml-1.5 font-normal text-bad">−{doc.data.diff.removed}</span>
                </h2>
                <pre className="selectable overflow-x-auto rounded-lg border border-border-subtle bg-surface-raised p-3 font-mono text-[12px] leading-relaxed">
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

            {doc.data?.document ? (
              <div className="selectable text-[14px] leading-relaxed whitespace-pre-wrap">
                {doc.data.document.body}
              </div>
            ) : (
              <Empty
                message="This artifact has no body yet."
                hint="Ask Claude to write one — keel_write_doc."
              />
            )}

            {(graph.data?.neighbours.length ?? 0) > 0 && (
              <section className="mt-8 border-t border-border-subtle pt-4">
                <h2 className="mb-2 text-[13px] font-semibold tracking-wide text-ink-muted uppercase">
                  Connected
                </h2>
                <ul className="space-y-1.5 text-[13px]">
                  {graph.data?.neighbours.map((n) => (
                    <li key={`${n.id}-${n.rel}`} className="flex items-center gap-2">
                      <Badge>{n.rel}</Badge>
                      <Badge>{n.entity_type}</Badge>
                      {n.anchor && <Badge tone="border-accent/40 text-accent">{n.anchor}</Badge>}
                      <Id value={n.id} />
                    </li>
                  ))}
                </ul>
              </section>
            )}
          </article>
        )}
      </div>
    </div>
  );
}
