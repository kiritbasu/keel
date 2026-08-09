/**
 * Screen 6 — Search. Hybrid, cross-project, faceted by type.
 */

import { useState } from "react";
import { api, type SearchHit } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Empty, ErrorBox, Spinner, cx } from "../components/ui";
import type { ScreenProps } from "../App";

const FACETS = [
  "spec",
  "decision",
  "question",
  "feedback",
  "task",
  "milestone",
  "term",
  "design",
  "environment",
  "artifact",
  "project",
];

export function SearchScreen({ project, generation }: ScreenProps) {
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const [types, setTypes] = useState<string[]>([]);
  const [scoped, setScoped] = useState(false);

  const { data, error, loading } = useAsync<{ hits: SearchHit[]; total: number; truncated: boolean }>(
    async () => {
      if (!query.trim()) return { hits: [], total: 0, truncated: false };
      return api.search(query, {
        project: scoped ? project : undefined,
        types: types.length ? types.join(",") : undefined,
        limit: 50,
      });
    },
    [query, types.join(","), scoped, project, generation],
  );

  return (
    <div className="mx-auto max-w-4xl space-y-4 p-6">
      <h1 className="text-xl font-semibold tracking-tight">Search</h1>

      <form
        onSubmit={(e) => {
          e.preventDefault();
          setQuery(input);
        }}
      >
        <input
          autoFocus
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Ask a question — 'why is billing slow', 'what did customers say about onboarding'"
          className="selectable w-full rounded-lg border border-border-subtle bg-surface-raised px-4 py-2.5 text-[14px] outline-none placeholder:text-ink-faint focus:border-accent/60"
        />
      </form>

      <div className="flex flex-wrap items-center gap-1.5">
        {project && (
          <button
            onClick={() => setScoped((v) => !v)}
            className={cx(
              "rounded border px-2 py-1 text-[12px]",
              scoped
                ? "border-accent/50 bg-accent/10 text-accent"
                : "border-border-subtle text-ink-muted hover:bg-surface-hover",
            )}
          >
            {project} only
          </button>
        )}
        {FACETS.map((t) => (
          <button
            key={t}
            onClick={() =>
              setTypes((prev) => (prev.includes(t) ? prev.filter((x) => x !== t) : [...prev, t]))
            }
            className={cx(
              "rounded border px-2 py-1 text-[12px]",
              types.includes(t)
                ? "border-accent/50 bg-accent/10 text-accent"
                : "border-border-subtle text-ink-faint hover:bg-surface-hover",
            )}
          >
            {t}
          </button>
        ))}
      </div>

      {error && <ErrorBox error={error} />}
      {loading && query && <Spinner label="Searching…" />}

      {!query && (
        <Empty
          message="Type a question."
          hint="Prefer a natural question over keywords — the semantic half is what makes 'why is billing slow' find a decision about aggregation granularity."
        />
      )}

      {query && !loading && data && data.hits.length === 0 && (
        <Empty message={`Nothing matches “${query}”.`} hint="Try fewer words, or clear the type filters." />
      )}

      {data && data.hits.length > 0 && (
        <>
          <p className="text-[12px] text-ink-faint">
            {data.hits.length} result{data.hits.length === 1 ? "" : "s"}
            {data.truncated && ` of ${data.total}`}
          </p>
          <ul className="space-y-2">
            {data.hits.map((hit) => (
              <li
                key={hit.entity_id}
                className="rounded-lg border border-border-subtle bg-surface-raised px-4 py-3"
              >
                <div className="flex items-center gap-2">
                  <Badge>{hit.entity_type}</Badge>
                  <span className="selectable truncate text-[14px] font-medium">{hit.title}</span>
                  <Badge
                    tone={hit.source === "both" ? "border-good/40 text-good bg-good/10" : undefined}
                    title={
                      hit.source === "both"
                        ? "Found independently by both the keyword and semantic indexes — the strongest signal available"
                        : hit.source === "semantic"
                          ? "Found by meaning rather than by words"
                          : "Found by keyword"
                    }
                  >
                    {hit.source}
                  </Badge>
                </div>
                {hit.excerpt && (
                  <p className="selectable mt-1.5 text-[13px] leading-relaxed text-ink-muted">
                    {hit.excerpt}
                  </p>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
