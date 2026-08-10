/**
 * Screen 6 — Search. Hybrid, cross-project, faceted by type.
 *
 * The query and the facets live in the address, so a search is a link. Scope is
 * the address too: `/search` is everything, `/projects/keel/search` is one
 * project — the same distinction the rest of the app already makes, rather than
 * a second mechanism that means the same thing.
 */

import { useEffect, useState } from "react";
import { api, type Entity, type SearchHit } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Input, Menu, MenuItem, Spinner, Tooltip } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { navigate, setQuery } from "../lib/router";
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

const SOURCE_EXPLANATION: Record<string, string> = {
  both: "Found independently by both the keyword and semantic indexes — the strongest signal available",
  semantic: "Found by meaning rather than by words",
  keyword: "Found by keyword",
};

export function SearchScreen({ route, generation }: ScreenProps) {
  const project = route.project;
  const query = route.query.q ?? "";
  const types = (route.query.types ?? "").split(",").filter(Boolean);

  // The box is local so typing does not push a history entry per keystroke;
  // submitting is what makes a search an address.
  const [input, setInput] = useState(query);
  useEffect(() => setInput(query), [query]);

  const { data, error, loading } = useAsync<{ hits: SearchHit[]; total: number; truncated: boolean }>(
    async () => {
      if (!query.trim()) return { hits: [], total: 0, truncated: false };
      return api.search(query, {
        project,
        types: types.length ? types.join(",") : undefined,
        limit: 50,
      });
    },
    [query, types.join(","), project, generation],
  );

  const toggleType = (type: string) => {
    const next = types.includes(type) ? types.filter((t) => t !== type) : [...types, type];
    setQuery(route, { types: next.join(",") }, { replace: true });
  };

  // Scope is the address: `/search` is everything, `/projects/x/search` is one
  // project. The menu exists because that makes scope reachable from a global
  // search, which it never was — the old chip only appeared when a project
  // happened to be selected elsewhere in the app.
  const projects = useAsync<{ projects: Entity[] }>(() => api.projects(), [generation]);

  return (
    <Page
      title="Search"
      crumbs={project ? projectCrumbs(route, "Search") : undefined}
      toolbar={
        <>
          <Menu label={project ? `${project} only` : "All projects"}>
            {(close) => (
              <>
                <MenuItem
                  selected={!project}
                  onClick={() => {
                    close();
                    navigate({ screen: "search", query: route.query });
                  }}
                >
                  All projects
                </MenuItem>
                {(projects.data?.projects ?? []).map((p) => {
                  const slug = String(p.slug ?? "");
                  return (
                    <MenuItem
                      key={p.id}
                      selected={project === slug}
                      onClick={() => {
                        close();
                        navigate({ screen: "search", project: slug, query: route.query });
                      }}
                    >
                      {String(p.name ?? slug)}
                    </MenuItem>
                  );
                })}
              </>
            )}
          </Menu>
          {FACETS.map((t) => (
            <Chip key={t} selected={types.includes(t)} onClick={() => toggleType(t)}>
              {t}
            </Chip>
          ))}
        </>
      }
    >
      <div className="space-y-4">
        <form
          onSubmit={(e) => {
            e.preventDefault();
            setQuery(route, { q: input });
          }}
        >
          <Input
            autoFocus
            variant="lg"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            aria-label="Search"
            placeholder="Ask a question — 'why is billing slow', 'what did customers say about onboarding'"
          />
        </form>

        {error && <ErrorBox error={error} />}
        {loading && query && <Spinner label="Searching…" />}

        {!query && (
          <Empty
            message="Type a question."
            hint="Prefer a natural question over keywords — the semantic half is what makes 'why is billing slow' find a decision about aggregation granularity."
          />
        )}

        {query && !loading && data && data.hits.length === 0 && (
          <Empty
            message={`Nothing matches “${query}”.`}
            hint="Try fewer words, or clear the type filters."
          />
        )}

        {data && data.hits.length > 0 && (
          <>
            <p className="text-small text-ink-faint">
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
                    <span className="selectable truncate text-body font-medium">{hit.title}</span>
                    <Tooltip align="right" text={SOURCE_EXPLANATION[hit.source] ?? hit.source}>
                      <Badge
                        tone={hit.source === "both" ? "border-good/40 text-good bg-good/10" : undefined}
                      >
                        {hit.source}
                      </Badge>
                    </Tooltip>
                  </div>
                  {hit.excerpt && (
                    <p className="selectable mt-1.5 text-small leading-relaxed text-ink-muted">
                      {hit.excerpt}
                    </p>
                  )}
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </Page>
  );
}
