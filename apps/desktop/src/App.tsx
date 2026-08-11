/**
 * The shell: navigation, live refresh, and the keyboard.
 *
 * Read and search first, per the PRD. Writing is possible through Claude and is
 * never the fast path here — so there are no forms, and every screen is a view.
 *
 * What the screen shows is now a function of the address rather than of two
 * variables in memory. That is what makes Back work, makes a reload land where
 * you were, and makes a link to a board a thing that can exist.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, subscribe, type Entity } from "./lib/api";
import {
  href,
  navigate,
  NEEDS_PROJECT,
  toHash,
  useRoute,
  type Route,
  type ScreenId,
} from "./lib/router";
import { Button, Tooltip, cx } from "./components/ui";
import { CommandPalette } from "./components/CommandPalette";
import { ThemeControl } from "./components/ThemeControl";
import { HomeScreen } from "./screens/Home";
import { ProjectScreen } from "./screens/Project";
import { MetricsScreen } from "./screens/Metrics";
import { RoadmapScreen } from "./screens/Roadmap";
import { BoardScreen } from "./screens/Board";
import { TaskScreen } from "./screens/Task";
import { DocumentsScreen } from "./screens/Documents";
import { SearchScreen } from "./screens/Search";
import { ActivityScreen } from "./screens/Activity";

export type { ScreenId } from "./lib/router";

const SCREENS: Array<{ id: ScreenId; label: string; key: string }> = [
  { id: "home", label: "Home", key: "1" },
  { id: "project", label: "Project", key: "2" },
  { id: "roadmap", label: "Roadmap", key: "3" },
  { id: "board", label: "Board", key: "4" },
  { id: "documents", label: "Documents", key: "5" },
  { id: "search", label: "Search", key: "6" },
  { id: "metrics", label: "Metrics", key: "7" },
  { id: "activity", label: "Activity", key: "8" },
];

export function App() {
  const route = useRoute();
  const [projects, setProjects] = useState<Entity[]>([]);
  const [paletteOpen, setPaletteOpen] = useState(false);
  // Bumping this re-runs every screen's fetch. One counter rather than
  // per-screen subscriptions: the daemon's change events say *something*
  // changed, not what, so a global refetch is both correct and the simplest
  // thing that is correct.
  const [generation, setGeneration] = useState(0);
  const refresh = useCallback(() => setGeneration((g) => g + 1), []);

  useEffect(() => {
    api
      .projects()
      .then((r) => setProjects(r.projects ?? []))
      .catch(() => setProjects([]));
  }, [generation]);

  useEffect(() => subscribe(refresh), [refresh]);

  // Keep the address honest.
  //
  // Two things can leave it disagreeing with what is on screen: an address that
  // names a project-scoped screen without naming a project, and an address that
  // matches no route at all and therefore fell back to Home. Both would sit in
  // the bar looking like a place you could send someone. Correcting them with
  // `replace` means neither becomes a Back destination — nobody chose to be
  // there.
  //
  // This cannot loop: `parseHash(toHash(r))` is `r`, so the corrected address
  // parses to the same route and the effect's inputs do not change again.
  const canonical =
    NEEDS_PROJECT[route.screen] && !route.project
      ? toHash({ screen: "home", query: route.query })
      : toHash(route);
  useEffect(() => {
    if (window.location.hash !== canonical) {
      window.history.replaceState(
        null,
        "",
        `${window.location.pathname}${window.location.search}${canonical}`,
      );
      window.dispatchEvent(new HashChangeEvent("hashchange"));
    }
  }, [canonical]);

  // Keyboard. Digits switch screens, `/` jumps to search, Cmd-K opens the
  // palette.
  //
  // The modifier check used to be `if (e.metaKey || e.ctrlKey || e.altKey)
  // return`, which discarded every modified keypress before anything could see
  // it — so Cmd-K was unreachable by construction. It is now narrowed to the
  // combinations this app does not claim, which leaves the browser's and the
  // system's shortcuts alone while letting ours through.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const isPaletteKey = (e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "k";
      if (isPaletteKey) {
        e.preventDefault();
        setPaletteOpen((open) => !open);
        return;
      }

      // Typing in a field is typing, not navigating — and the palette owns its
      // own keys while it is open.
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      if (e.key === "/") {
        e.preventDefault();
        navigate({ screen: "search", ...(route.project ? { project: route.project } : {}) });
        return;
      }

      const match = SCREENS.find((s) => s.key === e.key);
      if (match && (!NEEDS_PROJECT[match.id] || route.project)) {
        // Consume the key. Without this, navigating to a screen whose input
        // autofocuses means the same physical keypress also lands *in* that
        // input — pressing 6 for Search left a stray "6" in the search box.
        e.preventDefault();
        navigate({ screen: match.id, ...(NEEDS_PROJECT[match.id] ? { project: route.project } : {}) });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [route.project]);

  const current = useMemo(() => {
    const shared = { route, generation };
    switch (route.screen) {
      case "home":
        return <HomeScreen {...shared} />;
      case "project":
        return <ProjectScreen {...shared} />;
      case "roadmap":
        return <RoadmapScreen {...shared} />;
      case "metrics":
        return <MetricsScreen {...shared} />;
      case "board":
        return <BoardScreen {...shared} />;
      case "task":
        return <TaskScreen {...shared} />;
      case "documents":
        return <DocumentsScreen {...shared} />;
      case "search":
        return <SearchScreen {...shared} />;
      case "activity":
        return <ActivityScreen {...shared} />;
    }
  }, [route, generation]);

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-border-subtle bg-surface-sunken">
        <div className="px-4 py-4">
          <div className="text-heading font-semibold tracking-tight text-brand">Keel</div>
          <div className="text-micro text-ink-faint">the project spine</div>
        </div>

        <div className="px-2">
          {SCREENS.map((s) => {
            const disabled = NEEDS_PROJECT[s.id] && !route.project;
            const link = (
              <a
                href={disabled ? undefined : href({ screen: s.id, project: route.project })}
                aria-disabled={disabled}
                aria-current={route.screen === s.id ? "page" : undefined}
                onClick={(e) => disabled && e.preventDefault()}
                className={cx(
                  "flex w-full items-center justify-between rounded px-2.5 py-1.5 text-left text-small",
                  route.screen === s.id
                    ? "bg-surface-hover text-ink"
                    : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                  disabled && "cursor-not-allowed opacity-35 hover:bg-transparent",
                )}
              >
                {s.label}
                <kbd className="font-mono text-micro text-ink-faint">{s.key}</kbd>
              </a>
            );
            return (
              <div key={s.id}>
                {disabled ? <Tooltip text="Pick a project first" align="left">{link}</Tooltip> : link}
              </div>
            );
          })}
        </div>

        <div className="mt-6 px-2">
          <div className="px-2.5 pb-1 text-micro tracking-wide text-ink-faint uppercase">Projects</div>
          {projects.length === 0 && <div className="px-2.5 py-1 text-small text-ink-faint">none yet</div>}
          {projects.map((p) => {
            const slug = String(p.slug ?? "");
            return (
              <a
                key={p.id}
                href={href({ screen: "project", project: slug })}
                className={cx(
                  "block w-full truncate rounded px-2.5 py-1 text-left text-small",
                  route.project === slug
                    ? "bg-surface-hover text-ink"
                    : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                )}
              >
                {String(p.name ?? slug)}
              </a>
            );
          })}
        </div>

        <div className="mt-auto px-3 py-3">
          <div className="flex items-center gap-2">
            <Button size="sm" variant="ghost" onClick={() => setPaletteOpen(true)}>
              Jump to…
              <kbd className="font-mono text-micro text-ink-faint">⌘K</kbd>
            </Button>
            <Button size="sm" variant="ghost" onClick={refresh}>
              Refresh
            </Button>
          </div>
          <div className="mt-cosy">
            <ThemeControl />
          </div>
        </div>
      </nav>

      <main className="min-w-0 flex-1">{current}</main>

      <CommandPalette
        open={paletteOpen}
        onClose={() => setPaletteOpen(false)}
        route={route}
        generation={generation}
      />
    </div>
  );
}

/** What every screen receives. Where the reader is, and when to refetch. */
export interface ScreenProps {
  route: Route;
  generation: number;
}
