/**
 * Screen 6 — Search. Hybrid, cross-project, faceted by type.
 *
 * The query and the facets live in the address, so a search is a link. Scope is
 * the address too: `/search` is everything, `/projects/specline/search` is one
 * project — the same distinction the rest of the app already makes, rather than
 * a second mechanism that means the same thing.
 */

import { useEffect, useMemo, useState } from "react";
import { api, type Digest, type Entity, type SearchHit } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { Badge, Chip, Empty, ErrorBox, Input, Menu, MenuItem, Spinner, Tooltip } from "../components/ui";
import { Page, projectCrumbs } from "../components/Page";
import { href, navigate, setQuery } from "../lib/router";
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


/**
 * Three or four questions drawn from what this project actually contains.
 *
 * The screen used to suggest "why is billing slow" — copied from the MCP tool
 * description, which is written for a generic project. On the one screen whose
 * whole job is to invite a question, that taught the reader nothing and named
 * nothing they had ever seen.
 *
 * The three sources are chosen because each demonstrates a different thing
 * semantic search is for: an open question is prose you can search for by
 * meaning, a decision framed as a "why" finds reasoning whose title never uses
 * those words, and a glossary term shows that the store knows the project's own
 * vocabulary.
 *
 * Returns nothing for an empty project, so the caller can fall back to prose
 * rather than offering chips built from nothing.
 */
function starterQueries(digest: Digest | null | undefined): string[] {
  if (!digest) return [];
  const out: string[] = [];

  const open = digest.questions?.find((q) => q.status === "open");
  if (open) {
    // Verbatim, minus the "TQ-30 — " prefix: the identifier is how the row is
    // filed, not how anyone would ask about it.
    out.push(open.label.replace(/^[A-Z]+-\d+\s*[—–-]\s*/, ""));
  }

  const decision = digest.decisions?.[0];
  if (decision) {
    // "why did we decide that X", not "why did we X". Decision titles in this
    // store are statements — "The plain-English rule covers every prose field"
    // — so the shorter template produced "why did we the plain-English rule
    // covers…", which is not a sentence anyone would type.
    const title = decision.label.charAt(0).toLowerCase() + decision.label.slice(1);
    out.push(`why did we decide that ${title}`);
  }

  const term = digest.terms?.find((t) => !t.global) ?? digest.terms?.[0];
  // "what does X mean" rather than "what is a X", which produced "what is a
  // anchor". Choosing a/an correctly needs more than a vowel check, and the
  // phrasing that needs no article is the one that cannot be wrong.
  if (term) out.push(`what does "${term.term}" mean`);

  return out.filter(Boolean);
}

/** Shorten a chip's label without changing the query it runs. */
function chipLabel(query: string): string {
  return query.length > 70 ? `${query.slice(0, 67)}…` : query;
}

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

  // Starter queries come from the project's own content. The digest already
  // knows its open questions, recent decisions and glossary, so this costs one
  // call and teaches what semantic search is for using material the reader
  // recognises — rather than the billing example, which was lifted from a tool
  // description written for a generic project and had nothing to do with the
  // project in front of you.
  const digest = useAsync(() => api.context(project), [project, generation]);
  const starters = useMemo(() => starterQueries(digest.data), [digest.data]);

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
            placeholder={
              starters[0] ? `Ask a question — e.g. “${chipLabel(starters[0])}”` : "Ask a question"
            }
          />
        </form>

        {error && <ErrorBox error={error} />}
        {loading && query && <Spinner label="Searching…" />}

        {!query &&
          (starters.length > 0 ? (
            <div className="mt-6">
              <p className="mb-2 text-small text-ink-muted">
                Search understands meaning, not just words. Try one of these:
              </p>
              <div className="flex flex-wrap gap-1.5">
                {starters.map((q) => (
                  <Chip
                    key={q}
                    onClick={() => {
                      setInput(q);
                      setQuery(route, { q }, { replace: false });
                    }}
                  >
                    {chipLabel(q)}
                  </Chip>
                ))}
              </div>
            </div>
          ) : (
            <Empty
              message="Type a question."
              hint="Prefer a natural question over keywords — searching by meaning is what finds a decision whose title never uses your words."
            />
          ))}

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
                <li key={hit.entity_id}>
                  {/* A hit is a link. It used to be dead text: the search told
                      you what it had found and gave you no way to reach it,
                      which is most of a search engine missing. */}
                  <a
                    href={destination(hit, projectOf(hit, projects.data?.projects))}
                    className="block rounded-lg border border-border-subtle bg-surface-raised px-4 py-3 transition-colors hover:border-accent/50 hover:bg-surface-hover"
                  >
                  <div className="flex items-center gap-2">
                    <Badge>{hit.entity_type}</Badge>
                    <span className="truncate text-body font-medium">{hit.title}</span>
                    <Tooltip align="right" text={SOURCE_EXPLANATION[hit.source] ?? hit.source}>
                      <Badge
                        tone={hit.source === "both" ? "border-good/40 text-good bg-good/10" : undefined}
                      >
                        {hit.source}
                      </Badge>
                    </Tooltip>
                  </div>
                  {hit.excerpt && (
                    <p className="mt-1.5 text-small leading-relaxed text-ink-muted">
                      {hit.excerpt}
                    </p>
                  )}
                  </a>
                </li>
              ))}
            </ul>
          </>
        )}
      </div>
    </Page>
  );
}

/** The slug of the project a hit belongs to, or `undefined` if it names none. */
function projectOf(hit: SearchHit, projects: Entity[] | undefined): string | undefined {
  // A project hit is its own project; everything else points at one.
  const id = hit.entity_type === "project" ? hit.entity_id : hit.project_id;
  const match = (projects ?? []).find((p) => p.id === id);
  return match ? String(match.slug) : undefined;
}

/**
 * Where a hit goes.
 *
 * Five of the thirteen types have a page of their own; the rest are rendered
 * only as part of a project, so that is where they lead. Landing on the right
 * project is a worse answer than landing on the row, and a better one than
 * landing nowhere — which is what a hit used to do.
 */
function destination(hit: SearchHit, project: string | undefined): string {
  if (!project) return href({ screen: "home" });
  switch (hit.entity_type) {
    case "task":
      return href({ screen: "task", project, taskId: hit.entity_id });
    case "spec":
    case "decision":
    case "question":
    case "feedback":
    case "design":
      return href({ screen: "documents", project, documentId: hit.entity_id });
    case "milestone":
      return href({ screen: "roadmap", project });
    default:
      return href({ screen: "project", project });
  }
}
