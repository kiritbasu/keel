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
          {LABEL[t]}
        </button>
      ))}
    </div>
  );
}
