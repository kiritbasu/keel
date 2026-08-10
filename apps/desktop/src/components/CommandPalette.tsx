/**
 * Cmd-K.
 *
 * Type three letters, land on the thing. This is the single feature that most
 * makes an app feel like a tracker rather than a set of screens, and it only
 * works because everything now has an address — the palette does nothing except
 * choose one and go there.
 *
 * Note for anyone adding a shortcut later: the app's global key handler used to
 * return early on *any* modified keypress, so Cmd-K never reached anything. That
 * restriction is now narrowed to the combinations the app does not claim, in
 * `App.tsx`.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Entity } from "../lib/api";
import { useAsync } from "../lib/useAsync";
import { navigate, NEEDS_PROJECT, type Route, type ScreenId } from "../lib/router";
import { Dialog, Input, cx, labelOf } from "./ui";

export type PaletteKind = "screen" | "project" | "document" | "task";

export interface PaletteItem {
  id: string;
  label: string;
  kind: PaletteKind;
  /** Secondary text: the project a thing belongs to, its status, its type. */
  hint?: string;
  route: Partial<Route> & { screen: ScreenId };
}

const KIND_LABEL: Record<PaletteKind, string> = {
  screen: "Go to",
  project: "Project",
  document: "Document",
  task: "Task",
};

/**
 * Score one candidate against what has been typed.
 *
 * Ranked rather than filtered, because "boa" should offer the Board before it
 * offers a task whose description happens to contain the letters b, o and a in
 * that order. The tiers, best first: the label starts with the query; a word in
 * the label starts with it; the label contains it anywhere; the letters appear
 * in order. Anything below that does not match at all.
 *
 * Returns `null` for no match so callers can filter and rank in one pass.
 */
export function score(label: string, query: string): number | null {
  if (!query) return 0;
  const haystack = label.toLowerCase();
  const needle = query.toLowerCase();

  if (haystack.startsWith(needle)) return 0;
  if (haystack.includes(` ${needle}`) || haystack.includes(`-${needle}`)) return 1;
  if (haystack.includes(needle)) return 2;

  // Subsequence: every letter of the query appears in order. This is what makes
  // "tdv" find "The detail view", and it is deliberately the weakest tier —
  // matched loosely enough to be useful, ranked low enough to stay out of the way.
  let at = 0;
  for (const character of needle) {
    const found = haystack.indexOf(character, at);
    if (found === -1) return null;
    at = found + 1;
  }
  return 3;
}

/** Filter and order the candidates. Stable within a tier, so the source order shows through. */
export function rank(items: PaletteItem[], query: string): PaletteItem[] {
  return items
    .map((item, index) => ({ item, index, tier: score(item.label, query) }))
    .filter((row): row is { item: PaletteItem; index: number; tier: number } => row.tier !== null)
    .sort((a, b) => a.tier - b.tier || a.index - b.index)
    .map((row) => row.item);
}

/** The destinations that are always available, given where you are. */
export function screenItems(project: string | undefined): PaletteItem[] {
  const screens: Array<{ screen: ScreenId; label: string }> = [
    { screen: "home", label: "Home — all projects" },
    { screen: "project", label: "Project dashboard" },
    { screen: "roadmap", label: "Roadmap" },
    { screen: "board", label: "Board" },
    { screen: "documents", label: "Documents" },
    { screen: "search", label: "Search" },
    { screen: "activity", label: "Activity" },
  ];
  return screens
    .filter((s) => !NEEDS_PROJECT[s.screen] || project)
    .map((s) => ({
      id: `screen:${s.screen}`,
      label: s.label,
      kind: "screen" as const,
      ...(NEEDS_PROJECT[s.screen] && project ? { hint: project } : {}),
      route: { screen: s.screen, ...(NEEDS_PROJECT[s.screen] ? { project } : {}) },
    }));
}

const PROSE_TYPES = "spec,decision,question,feedback,design";

export function CommandPalette({
  open,
  onClose,
  route,
  generation,
}: {
  open: boolean;
  onClose: () => void;
  route: Route;
  generation: number;
}) {
  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);

  // Opening is the trigger, not mounting: the palette is rendered on every
  // screen, and fetching every task in the store on app start to populate a
  // dialog nobody has asked for would be a request per launch for nothing.
  const contents = useAsync<{ projects: Entity[]; documents: Entity[]; tasks: Entity[] }>(
    async () => {
      if (!open) return { projects: [], documents: [], tasks: [] };
      const [projects, documents, tasks] = await Promise.all([
        api.projects(),
        api.entities({ project: route.project, type: PROSE_TYPES, limit: 500 }),
        api.entities({ project: route.project, type: "task", limit: 2000 }),
      ]);
      return {
        projects: projects.projects ?? [],
        documents: documents.items ?? [],
        tasks: tasks.items ?? [],
      };
    },
    [open, route.project, generation],
  );

  const items = useMemo<PaletteItem[]>(() => {
    const projects = (contents.data?.projects ?? []).map((p) => ({
      id: p.id,
      label: labelOf(p),
      kind: "project" as const,
      hint: String(p.slug ?? ""),
      route: { screen: "project" as const, project: String(p.slug ?? "") },
    }));

    const documents = (contents.data?.documents ?? [])
      .filter(() => Boolean(route.project))
      .map((d) => ({
        id: d.id,
        label: labelOf(d),
        kind: "document" as const,
        hint: String(d.type ?? ""),
        route: { screen: "documents" as const, project: route.project, documentId: d.id },
      }));

    const tasks = (contents.data?.tasks ?? [])
      .filter(() => Boolean(route.project))
      .map((t) => ({
        id: t.id,
        label: labelOf(t),
        kind: "task" as const,
        hint: `${String(t.status ?? "")}${t.priority ? ` · ${String(t.priority)}` : ""}`,
        route: { screen: "task" as const, project: route.project, taskId: t.id },
      }));

    return [...screenItems(route.project), ...projects, ...documents, ...tasks];
  }, [contents.data, route.project]);

  const matches = useMemo(() => rank(items, query).slice(0, 40), [items, query]);

  // A fresh palette every time. Reopening with the last search still in it, and
  // the cursor halfway down a list of results for it, is the kind of memory
  // nobody asked for.
  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
    }
  }, [open]);

  useEffect(() => setCursor(0), [query]);

  useEffect(() => {
    listRef.current?.querySelector('[data-selected="true"]')?.scrollIntoView({ block: "nearest" });
  }, [cursor, matches.length]);

  const go = (item: PaletteItem | undefined) => {
    if (!item) return;
    onClose();
    navigate(item.route);
  };

  return (
    <Dialog open={open} onClose={onClose} label="Command palette" className="p-0">
      <div className="border-b border-border-subtle p-2">
        <Input
          autoFocus
          variant="md"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setCursor((c) => Math.min(c + 1, Math.max(matches.length - 1, 0)));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setCursor((c) => Math.max(c - 1, 0));
            } else if (e.key === "Enter") {
              e.preventDefault();
              go(matches[cursor]);
            }
          }}
          placeholder="Jump to a project, task, document or screen…"
          aria-label="Jump to"
          className="border-transparent bg-transparent focus:border-transparent"
        />
      </div>

      <ul ref={listRef} className="max-h-80 overflow-y-auto p-1" role="listbox" aria-label="Results">
        {matches.length === 0 && (
          <li className="px-3 py-6 text-center text-small text-ink-faint">
            {contents.loading ? "Looking…" : `Nothing matches “${query}”.`}
          </li>
        )}
        {matches.map((item, i) => (
          <li key={item.id}>
            <button
              type="button"
              role="option"
              aria-selected={i === cursor}
              data-selected={i === cursor}
              onMouseMove={() => setCursor(i)}
              onClick={() => go(item)}
              className={cx(
                "flex w-full items-center gap-2 rounded px-3 py-2 text-left",
                i === cursor ? "bg-surface-hover" : "hover:bg-surface-hover",
              )}
            >
              <span className="w-16 shrink-0 text-micro text-ink-faint">{KIND_LABEL[item.kind]}</span>
              <span className="min-w-0 flex-1 truncate text-small">{item.label}</span>
              {item.hint && <span className="shrink-0 text-micro text-ink-faint">{item.hint}</span>}
            </button>
          </li>
        ))}
      </ul>

      <div className="flex items-center gap-3 border-t border-border-subtle px-3 py-1.5 text-micro text-ink-faint">
        <span>↑↓ to move</span>
        <span>↵ to open</span>
        <span>esc to close</span>
      </div>
    </Dialog>
  );
}
