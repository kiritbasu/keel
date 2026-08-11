/**
 * The page frame every screen sits in.
 *
 * Before this existed, seven screens were built to seven different rules: the
 * board full-bleed with horizontal scroll, Home and Project centred at one
 * width, Roadmap, Search and What changed at a narrower one, Documents a two-pane
 * split — and no two of them put the title in the same place. That is most of
 * what made the app read as seven applications sharing a sidebar.
 *
 * The frame owns three things and screens own none of them: where the title
 * goes, where you are (the breadcrumb), and how wide the content is allowed to
 * get.
 */

import type { ReactNode } from "react";
import { cx } from "./ui";
import { href, type Route } from "../lib/router";

/**
 * Three widths, not seven.
 *
 * `standard` is a reading measure and is the default: prose stops being
 * readable somewhere past 80 characters. `wide` is for dashboards, where the
 * unit is a card rather than a sentence. `full` is for the two screens that
 * genuinely own their own scrolling — the board's columns and the document
 * reader's split — and those get no padding or scroll container from here.
 */
export type PageWidth = "standard" | "wide" | "full";

const WIDTHS: Record<PageWidth, string> = {
  standard: "mx-auto w-full max-w-4xl p-6",
  wide: "mx-auto w-full max-w-6xl p-6",
  full: "h-full",
};

export interface Crumb {
  label: string;
  route?: Parameters<typeof href>[0];
}

export function Page({
  title,
  crumbs,
  meta,
  actions,
  toolbar,
  width = "standard",
  children,
}: {
  title: ReactNode;
  /** Where you are. The last crumb is the current page and is never a link. */
  crumbs?: Crumb[];
  /** A count, a status — the one fact that belongs beside the title. */
  meta?: ReactNode;
  /** Controls that act on the whole screen, right-aligned in the title row. */
  actions?: ReactNode;
  /** Filters. A second row, because they wrap and a title should not. */
  toolbar?: ReactNode;
  width?: PageWidth;
  children: ReactNode;
}) {
  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0 border-b border-border-subtle px-6 pt-4 pb-3">
        {crumbs && crumbs.length > 0 && (
          <nav aria-label="Breadcrumb" className="mb-1 flex items-center gap-1.5 text-micro text-ink-faint">
            {crumbs.map((crumb, i) => (
              <span key={`${crumb.label}-${i}`} className="flex items-center gap-1.5">
                {i > 0 && <span aria-hidden>/</span>}
                {crumb.route ? (
                  <a href={href(crumb.route)} className="hover:text-ink-muted">
                    {crumb.label}
                  </a>
                ) : (
                  <span>{crumb.label}</span>
                )}
              </span>
            ))}
          </nav>
        )}
        <div className="flex items-baseline gap-3">
          <h1 className="text-title font-semibold tracking-tight">{title}</h1>
          {meta}
          {actions && <div className="ml-auto flex items-center gap-2">{actions}</div>}
        </div>
        {toolbar && <div className="mt-2.5 flex flex-wrap items-center gap-1.5">{toolbar}</div>}
      </header>

      <div className={cx("min-h-0 flex-1", width === "full" ? "overflow-hidden" : "overflow-y-auto")}>
        <div className={WIDTHS[width]}>{children}</div>
      </div>
    </div>
  );
}

/**
 * The crumb trail for a project-scoped screen.
 *
 * Kept here rather than repeated per screen so that renaming the top level is
 * one edit, and so no screen can quietly disagree about what the trail says.
 */
export function projectCrumbs(route: Route, leaf?: string): Crumb[] {
  const crumbs: Crumb[] = [{ label: "Projects", route: { screen: "home" } }];
  if (route.project) {
    crumbs.push({
      label: route.project,
      route: leaf ? { screen: "project", project: route.project } : undefined,
    });
  }
  if (leaf) crumbs.push({ label: leaf });
  return crumbs;
}
