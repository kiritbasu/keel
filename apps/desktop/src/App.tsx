/**
 * The shell: navigation, project selection, and live refresh.
 *
 * Read and search first, per the PRD. Writing is possible through Claude and is
 * never the fast path here — so there are no forms, and every screen is a view.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { api, subscribe, type Entity } from "./lib/api";
import { cx } from "./components/ui";
import { HomeScreen } from "./screens/Home";
import { ProjectScreen } from "./screens/Project";
import { RoadmapScreen } from "./screens/Roadmap";
import { BoardScreen } from "./screens/Board";
import { DocumentsScreen } from "./screens/Documents";
import { SearchScreen } from "./screens/Search";
import { ActivityScreen } from "./screens/Activity";

export type ScreenId =
  | "home"
  | "project"
  | "roadmap"
  | "board"
  | "documents"
  | "search"
  | "activity";

const SCREENS: Array<{ id: ScreenId; label: string; key: string; needsProject: boolean }> = [
  { id: "home", label: "Home", key: "1", needsProject: false },
  { id: "project", label: "Project", key: "2", needsProject: true },
  { id: "roadmap", label: "Roadmap", key: "3", needsProject: false },
  { id: "board", label: "Board", key: "4", needsProject: true },
  { id: "documents", label: "Documents", key: "5", needsProject: true },
  { id: "search", label: "Search", key: "6", needsProject: false },
  { id: "activity", label: "Activity", key: "7", needsProject: false },
];

export function App() {
  const [screen, setScreen] = useState<ScreenId>("home");
  const [project, setProject] = useState<string | undefined>();
  const [projects, setProjects] = useState<Entity[]>([]);
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

  // Keyboard-driven, per SPEC §10's note on the board. Digits switch screens;
  // `/` jumps to search, which is the one thing worth a dedicated key.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      if (target && ["INPUT", "TEXTAREA"].includes(target.tagName)) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;

      if (e.key === "/") {
        e.preventDefault();
        setScreen("search");
        return;
      }
      const match = SCREENS.find((s) => s.key === e.key);
      if (match && (!match.needsProject || project)) {
        // Consume the key. Without this, navigating to a screen whose input
        // autofocuses means the same physical keypress also lands *in* that
        // input — pressing 6 for Search left a stray "6" in the search box.
        e.preventDefault();
        setScreen(match.id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [project]);

  const openProject = useCallback((slug: string) => {
    setProject(slug);
    setScreen("project");
  }, []);

  const current = useMemo(() => {
    const shared = { project, generation, openProject, setScreen };
    switch (screen) {
      case "home":
        return <HomeScreen {...shared} />;
      case "project":
        return <ProjectScreen {...shared} />;
      case "roadmap":
        return <RoadmapScreen {...shared} />;
      case "board":
        return <BoardScreen {...shared} />;
      case "documents":
        return <DocumentsScreen {...shared} />;
      case "search":
        return <SearchScreen {...shared} />;
      case "activity":
        return <ActivityScreen {...shared} />;
    }
  }, [screen, project, generation, openProject]);

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-border-subtle bg-surface-raised">
        <div className="px-4 py-4">
          <div className="text-[15px] font-semibold tracking-tight">Keel</div>
          <div className="text-[11px] text-ink-faint">the project spine</div>
        </div>

        <div className="px-2">
          {SCREENS.map((s) => {
            const disabled = s.needsProject && !project;
            return (
              <button
                key={s.id}
                disabled={disabled}
                onClick={() => setScreen(s.id)}
                title={disabled ? "Pick a project first" : `Press ${s.key}`}
                className={cx(
                  "flex w-full items-center justify-between rounded px-2.5 py-1.5 text-left text-[13px]",
                  screen === s.id
                    ? "bg-surface-hover text-ink"
                    : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                  disabled && "cursor-not-allowed opacity-35 hover:bg-transparent",
                )}
              >
                {s.label}
                <kbd className="font-mono text-[10px] text-ink-faint">{s.key}</kbd>
              </button>
            );
          })}
        </div>

        <div className="mt-6 px-2">
          <div className="px-2.5 pb-1 text-[11px] tracking-wide text-ink-faint uppercase">
            Projects
          </div>
          {projects.length === 0 && (
            <div className="px-2.5 py-1 text-[12px] text-ink-faint">none yet</div>
          )}
          {projects.map((p) => {
            const slug = String(p.slug ?? "");
            return (
              <button
                key={p.id}
                onClick={() => openProject(slug)}
                className={cx(
                  "block w-full truncate rounded px-2.5 py-1 text-left text-[13px]",
                  project === slug
                    ? "bg-surface-hover text-ink"
                    : "text-ink-muted hover:bg-surface-hover hover:text-ink",
                )}
              >
                {String(p.name ?? slug)}
              </button>
            );
          })}
        </div>

        <div className="mt-auto px-4 py-3 text-[11px] text-ink-faint">
          <button onClick={refresh} className="hover:text-ink-muted">
            refresh
          </button>
          <span className="mx-1.5">·</span>
          <span title="Writing happens through Claude, not here">read-only</span>
        </div>
      </nav>

      <main className="flex-1 overflow-y-auto">{current}</main>
    </div>
  );
}

/** What every screen receives. */
export interface ScreenProps {
  project: string | undefined;
  generation: number;
  openProject: (slug: string) => void;
  setScreen: (screen: ScreenId) => void;
}
