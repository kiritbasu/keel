import { useState } from "react";
import { cx } from "./ui";
import { THEMES, readTheme, setTheme, type Theme } from "../lib/theme";

/**
 * The word each option would have carried, kept for the tooltip and for the
 * accessible name.
 *
 * The icons replaced the words on screen, not in the markup. A glyph with no
 * name is unreadable to a screen reader and unguessable to anyone who does not
 * already know the convention, so `aria-label` still says "Auto" and hovering
 * still shows the sentence.
 */
const LABEL: Record<Theme, string> = {
  system: "Auto",
  light: "Light",
  dark: "Dark",
};

const TITLE: Record<Theme, string> = {
  system: "Auto — follow the system",
  light: "Light — always light",
  dark: "Dark — always dark",
};

/**
 * Inline SVG rather than an icon dependency: three glyphs do not justify a
 * package, and these inherit `currentColor` so the selected state keeps
 * working without a second set of rules.
 */
const ICON: Record<Theme, React.ReactNode> = {
  // Half-filled circle: the usual "follows something else" mark.
  system: (
    <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden="true">
      <circle
        cx="8"
        cy="8"
        r="6"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
      />
      <path d="M8 2a6 6 0 0 0 0 12z" fill="currentColor" />
    </svg>
  ),
  light: (
    <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden="true">
      <circle cx="8" cy="8" r="3.25" fill="currentColor" />
      <g stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
        <path d="M8 1v1.75M8 13.25V15M1 8h1.75M13.25 8H15" />
        <path d="M3.05 3.05l1.24 1.24M11.71 11.71l1.24 1.24M12.95 3.05l-1.24 1.24M4.29 11.71l-1.24 1.24" />
      </g>
    </svg>
  ),
  dark: (
    <svg viewBox="0 0 16 16" className="size-3.5" aria-hidden="true">
      <path
        d="M13.5 9.9A5.8 5.8 0 0 1 6.1 2.5a5.8 5.8 0 1 0 7.4 7.4z"
        fill="currentColor"
      />
    </svg>
  ),
};

/**
 * Three-way theme switch for the navigation rail.
 *
 * A segmented control rather than a toggle, because there are genuinely three
 * states and a two-state toggle would have to hide "follow the system"
 * somewhere else or drop it.
 */
export function ThemeControl() {
  const [theme, setLocal] = useState<Theme>(readTheme);

  function choose(next: Theme) {
    setTheme(next);
    setLocal(next);
  }

  return (
    <div
      role="radiogroup"
      aria-label="Theme"
      className="flex gap-hair rounded-control border border-border-subtle p-hair"
    >
      {THEMES.map((t) => (
        <button
          key={t}
          type="button"
          role="radio"
          aria-checked={theme === t}
          aria-label={LABEL[t]}
          title={TITLE[t]}
          onClick={() => choose(t)}
          /* `min-h-8` — the label stays small, the target does not.
           *
           * Measured at 58×19 CSS pixels, which is well under the ~44px
           * recommended target and is the smallest interactive thing in the
           * app. It also sits at the very bottom edge of the window, where a
           * pointer is least precise and there is no forgiving margin below it.
           *
           * The type size is deliberate — this is chrome and should stay
           * quiet — so the padding grows rather than the text. `min-h` rather
           * than a taller `py` so the three stay aligned whatever the label. */
          className={cx(
            "flex min-h-8 flex-1 items-center justify-center rounded-control px-tight text-micro",
            theme === t
              ? "bg-surface-hover text-ink"
              : "text-ink-faint hover:text-ink-muted",
          )}
        >
          {ICON[t]}
        </button>
      ))}
    </div>
  );
}
