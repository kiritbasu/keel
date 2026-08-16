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
import { api, subscribe, type Entity, type FeedStatus } from "./lib/api";
import {
  href,
  navigate,
  NEEDS_PROJECT,
  toHash,
  useRoute,
  type Route,
  type ScreenId,
} from "./lib/router";
import { Button, Menu, MenuItem, cx } from "./components/ui";
import { defaultProject, rememberProject } from "./lib/lastProject";
import { CommandPalette } from "./components/CommandPalette";
import { ThemeControl } from "./components/ThemeControl";
import { VersionFooter } from "./components/VersionFooter";
import { HomeScreen } from "./screens/Home";
import { ProjectScreen } from "./screens/Project";
import { RoadmapScreen } from "./screens/Roadmap";
import { BoardScreen } from "./screens/Board";
import { ReadyScreen } from "./screens/Ready";
import { TaskScreen } from "./screens/Task";
import { DocumentsScreen } from "./screens/Documents";
import { SearchScreen } from "./screens/Search";
import { ChangedScreen } from "./screens/Changed";

export type { ScreenId } from "./lib/router";

type NavItem = { id: ScreenId; label: string; key: string };

/**
 * The screens that belong to the project you are in. Always live, because a
 * project is always selected — see `lib/lastProject`.
 *
 * Roadmap sits here even though it can render across every project, because
 * "the roadmap of the thing I am looking at" is what you want nine times out of
 * ten. The all-projects address still exists and the palette still reaches it.
 */
const PROJECT_SCREENS: NavItem[] = [
  { id: "project", label: "Overview", key: "1" },
  // Second, ahead of the board. The board is every task; this is the handful
  // that can be started now, which is the question actually being asked when
  // someone opens the app.
  { id: "ready", label: "Ready", key: "2" },
  { id: "board", label: "Board", key: "3" },
  { id: "roadmap", label: "Roadmap", key: "4" },
  { id: "documents", label: "Library", key: "5" },
];

/**
 * The screens that mean something without a project.
 *
 * "What changed" earned its name when it started answering the question. It was
 * called Activity while it was a feed of the last 300 events, deliberately —
 * the better name would have promised the grouped-by-session view it did not
 * have, and the screen already carried one header claiming a job it did not do.
 */
const GLOBAL_SCREENS: NavItem[] = [
  { id: "home", label: "All projects", key: "6" },
  { id: "search", label: "Search", key: "7" },
  { id: "changed", label: "What changed", key: "8" },
];

const SCREENS: NavItem[] = [...PROJECT_SCREENS, ...GLOBAL_SCREENS];

/**
 * Whether a nav item should carry the project in its address.
 *
 * Not the same as `NEEDS_PROJECT`, and the difference is Roadmap: it renders
 * happily without one, so the router does not require it, but in the rail it is
 * a project screen and should link to the project you are in.
 */
function isProjectScreen(id: ScreenId): boolean {
  return PROJECT_SCREENS.some((s) => s.id === id);
}

/** One row in the rail. Never disabled — see `lib/lastProject`. */
function NavLink({
  item,
  route,
  project,
}: {
  item: NavItem;
  route: Route;
  project?: string;
}) {
  const here = route.screen === item.id;
  return (
    <a
      href={href({ screen: item.id, project })}
      aria-current={here ? "page" : undefined}
      className={cx(
        "flex w-full items-center justify-between rounded-control px-2.5 py-1.5 text-left text-small",
        here
          ? "bg-accent-quiet text-accent"
          : "text-ink-muted hover:bg-surface-hover hover:text-ink",
      )}
    >
      {item.label}
      {/* A keycap, because the digit has to say "a key" and not "how many".
       *
       * A bare right-aligned digit beside a navigation label means a count —
       * that is the convention in every mail client, chat app and issue
       * tracker, and a faint monospace style does not overturn it. It was read
       * that way and reported as a bug: a brand new install with an empty store
       * showed "All projects 6, Search 7, What changed 8", which reads as data
       * appearing from nowhere. Every digit was also wrong as a count — "Ready
       * 2" against 21 ready tasks (KEEL-223).
       *
       * The first fix was a leading `·`, which cannot be a quantity and so did
       * solve that. It was then read as unclear by the first person to see it,
       * who expected `⌘` — because the header next to it writes `Jump to… ⌘K`
       * and that was the only shortcut vocabulary on screen.
       *
       * **`⌘` would be a lie.** These are bare keypresses: the handler below
       * returns early on `metaKey || ctrlKey || altKey`, deliberately, because
       * ⌘1–⌘9 are the browser's tab shortcuts and this app does not claim
       * them. Printing `⌘1` would advertise a combination that switches the
       * user's tab.
       *
       * So: no prefix character at all, and a border instead. A boxed glyph is
       * the standard way to draw a key, it cannot be read as a quantity, and it
       * claims no modifier — which is the distinction the rail actually needed
       * to draw, between "a key" and "a number", rather than between itself and
       * the ⌘K beside it.
       */}
      <kbd
        className="rounded border border-border-subtle px-1 font-mono text-micro text-ink-faint"
        aria-hidden="true"
        title={`Press ${item.key}`}
      >
        {item.key}
      </kbd>
    </a>
  );
}

/**
 * One row naming the project you are in, opening the full list.
 *
 * One row instead of N: the old rail listed every project, so the shell grew
 * with the store and the thing you needed on launch sank further down it.
 */
function ProjectSwitcher({
  projects,
  current,
}: {
  projects: Entity[];
  current: string;
}) {
  const name =
    projects.find((p) => String(p.slug ?? "") === current)?.name ?? current;
  return (
    <Menu label={<span className="truncate">{String(name)}</span>} align="left">
      {(close) =>
        projects.map((p) => {
          const slug = String(p.slug ?? "");
          return (
            <MenuItem
              key={p.id}
              selected={slug === current}
              onClick={() => {
                close();
                navigate({ screen: "project", project: slug });
              }}
            >
              {String(p.name ?? slug)}
            </MenuItem>
          );
        })
      }
    </Menu>
  );
}

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

  // Whether the project list has come back yet, which is not the same question
  // as whether it is empty. Before it has, "there is no project to fall back
  // on" is unknown rather than false, and acting on it would bounce a
  // project-scoped address to Home a beat before the answer arrived.
  const [projectsLoaded, setProjectsLoaded] = useState(false);

  useEffect(() => {
    api
      .projects()
      .then((r) => setProjects(r.projects ?? []))
      .catch(() => setProjects([]))
      .finally(() => setProjectsLoaded(true));
  }, [generation]);

  // Health, for the version in the footer and whether an update is waiting.
  // Failing quietly is right here: this is a caption on the rail, and a daemon
  // that cannot answer already shows up as the live-feed warning below it.
  const [health, setHealth] = useState<{
    version?: string;
    staged_version?: string | null;
    release_notes?: string;
    staged_release_notes?: string | null;
    // Absent on a daemon older than the updater, which is what tells the
    // footer that its silence about updates means nothing (KEEL-227).
    update_check?: {
      enabled?: boolean;
      last_checked_at?: string | null;
      last_error?: string | null;
    };
    executable?: string | null;
  } | null>(null);
  useEffect(() => {
    api
      .health()
      .then(setHealth)
      .catch(() => setHealth(null));
  }, [generation]);

  // `refresh` on every connect as well as every change, so a daemon restart
  // does not leave the page showing what it had before the drop. See
  // `subscribe` — the reconnect itself announces nothing about what it missed.
  const [feed, setFeed] = useState<FeedStatus>("connecting");
  useEffect(() => subscribe(refresh, setFeed), [refresh]);

  const slugs = useMemo(
    () => projects.map((p) => String(p.slug ?? "")).filter(Boolean),
    [projects],
  );

  // The project to fall back on when the address does not name one. Null only
  // when the store has no projects at all, which is the one case where a
  // project-scoped screen genuinely cannot be shown.
  const fallbackProject = useMemo(() => defaultProject(slugs), [slugs]);

  // What the navigation points at: the project in the address if there is one,
  // otherwise the one we would fall back to. This is what makes the project
  // section of the rail live even while you are on a global screen.
  const activeProject = route.project ?? fallbackProject ?? undefined;

  // Remember where you were, so the next cold launch opens here.
  useEffect(() => {
    if (route.project) rememberProject(route.project);
  }, [route.project]);

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
  // A project-scoped screen with no project used to fall back to Home. Now it
  // fills in the remembered project instead, and only falls back to Home when
  // there is genuinely no project to fill in. That is the difference between
  // "you cannot go there yet" and "you can, and here is where".
  const canonical =
    !NEEDS_PROJECT[route.screen] || route.project
      ? toHash(route)
      : fallbackProject
        ? toHash({ ...route, project: fallbackProject })
        : projectsLoaded
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
      const isPaletteKey =
        (e.metaKey || e.ctrlKey) && !e.altKey && e.key.toLowerCase() === "k";
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
        navigate({
          screen: "search",
          ...(route.project ? { project: route.project } : {}),
        });
        return;
      }

      const match = SCREENS.find((s) => s.key === e.key);
      if (match && (!NEEDS_PROJECT[match.id] || activeProject)) {
        // Consume the key. Without this, navigating to a screen whose input
        // autofocuses means the same physical keypress also lands *in* that
        // input — pressing 6 for Search left a stray "6" in the search box.
        e.preventDefault();
        navigate({
          screen: match.id,
          ...(isProjectScreen(match.id) ? { project: activeProject } : {}),
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [route.project, activeProject]);

  // The active project's own word for a milestone. `undefined` means it has no
  // opinion and the interface says "milestone".
  const milestoneNoun = useMemo(() => {
    const match = projects.find((p) => String(p.slug ?? "") === activeProject);
    const noun = match?.milestone_noun;
    return typeof noun === "string" && noun.trim() ? noun.trim() : undefined;
  }, [projects, activeProject]);

  // The `KEEL` of `KEEL-42`. Taken from the project list the shell already
  // holds rather than from the digest: the board used to fetch a whole project
  // briefing partly to learn this one string.
  const projectKey = useMemo(() => {
    const match = projects.find((p) => String(p.slug ?? "") === activeProject);
    const key = match?.key;
    return typeof key === "string" && key.trim() ? key.trim() : undefined;
  }, [projects, activeProject]);

  const current = useMemo(() => {
    const shared = { route, generation, milestoneNoun, projectKey };
    switch (route.screen) {
      case "home":
        return <HomeScreen {...shared} />;
      case "project":
        return <ProjectScreen {...shared} />;
      case "roadmap":
        return <RoadmapScreen {...shared} />;
      case "board":
        return <BoardScreen {...shared} />;
      case "ready":
        return <ReadyScreen {...shared} />;
      case "task":
        return <TaskScreen {...shared} />;
      case "documents":
        return <DocumentsScreen {...shared} />;
      case "search":
        return <SearchScreen {...shared} />;
      case "changed":
        return <ChangedScreen {...shared} />;
    }
  }, [route, generation, milestoneNoun, projectKey]);

  return (
    <div className="flex h-full">
      <nav className="flex w-52 shrink-0 flex-col border-r border-border-subtle bg-surface-sunken">
        <div className="px-4 py-4">
          <div className="text-heading font-semibold tracking-tight text-brand">
            Keel
          </div>
          <div className="text-micro text-ink-faint">the project spine</div>
        </div>

        {/* The project first, because five of the eight screens below are
            about one and choosing it used to be the last thing on the rail. */}
        <div className="flex items-center gap-1.5 px-2 pb-2">
          {activeProject && (
            <ProjectSwitcher projects={projects} current={activeProject} />
          )}
          <Button
            size="sm"
            variant="ghost"
            className="ml-auto"
            onClick={() => setPaletteOpen(true)}
            title="Jump to a project, screen, document or task"
          >
            Jump to…
            <kbd className="font-mono text-micro text-ink-faint">⌘K</kbd>
          </Button>
        </div>

        <div className="px-2">
          {activeProject &&
            PROJECT_SCREENS.map((s) => (
              <NavLink
                key={s.id}
                item={s}
                route={route}
                project={activeProject}
              />
            ))}

          {activeProject && <hr className="mx-2.5 my-2 border-border-subtle" />}

          {GLOBAL_SCREENS.map((s) => (
            <NavLink key={s.id} item={s} route={route} />
          ))}
        </div>

        {projectsLoaded && projects.length === 0 && (
          <div className="mt-4 px-4 text-small text-ink-faint">
            No projects yet. Ask Claude to make one.
          </div>
        )}

        <div className="mt-auto px-3 py-3">
          {feed === "down" && (
            <p
              role="status"
              className="mt-cosy px-2.5 text-micro text-ink-faint"
              title="The daemon is not reachable, so this page stops updating on its own. It will catch up by itself when the connection returns."
            >
              Live updates disconnected — this page may be out of date.
            </p>
          )}
          <VersionFooter
            version={health?.version}
            stagedVersion={health?.staged_version}
            releaseNotes={health?.release_notes}
            stagedReleaseNotes={health?.staged_release_notes}
            updateCheck={health?.update_check}
            executable={health?.executable}
            // A reload rather than `refresh`. The daemon serves this page, so
            // the binary it just restarted into serves a different bundle —
            // refetching data would leave the old interface running against the
            // new daemon, which works and is not the version just installed
            // (KEEL-259).
            onApplied={() => window.location.reload()}
          />
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
  /**
   * What this project calls a milestone, when it has a word of its own.
   *
   * Threaded from the shell rather than fetched per screen, because the shell
   * already holds the project list and a second request to learn one word would
   * be a request per screen for a label.
   */
  milestoneNoun?: string;
  /**
   * The prefix of this project's readable identifiers — the `KEEL` of `KEEL-42`.
   *
   * Threaded for the same reason as `milestoneNoun`, and it removed a real cost:
   * the board's only other source for it was the digest.
   */
  projectKey?: string;
}
