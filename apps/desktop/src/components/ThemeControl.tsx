import { useState } from "react";
import { cx } from "./ui";
import { THEMES, readTheme, setTheme, type Theme } from "../lib/theme";

const LABEL: Record<Theme, string> = {
  system: "Auto",
  light: "Light",
  dark: "Dark",
};

const TITLE: Record<Theme, string> = {
  system: "Follow the system",
  light: "Always light",
  dark: "Always dark",
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
          title={TITLE[t]}
          onClick={() => choose(t)}
          className={cx(
            "flex-1 rounded-control px-tight py-hair text-micro",
            theme === t
              ? "bg-surface-hover text-ink"
              : "text-ink-faint hover:text-ink-muted",
          )}
        >
          {LABEL[t]}
        </button>
      ))}
    </div>
  );
}
