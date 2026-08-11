/**
 * The theme the user chose, as opposed to the one the operating system
 * happens to be in.
 *
 * Before this, the only thing deciding light or dark was a
 * `prefers-color-scheme` block, so the choice belonged to macOS and there was
 * no control anywhere in the app. Three values rather than two, because
 * "follow the system" is a real preference and not the absence of one.
 *
 * The mechanism is `color-scheme` rather than a class: every colour token is
 * declared once with `light-dark()`, and `light-dark()` reads whichever half
 * matches the element's used colour scheme. So switching `color-scheme` on
 * `:root` switches every token at once, and a token cannot exist in one scheme
 * and be missing from the other — which is how three status colours went
 * missing from the light palette before KEEL-73 found them by hand.
 */
export type Theme = "system" | "light" | "dark";

export const THEMES: Theme[] = ["system", "light", "dark"];

const KEY = "keel.theme";

/** Whether a stored string is still a theme we recognise. */
function isTheme(value: string | null): value is Theme {
  return value === "system" || value === "light" || value === "dark";
}

/**
 * The stored choice, or `system` when there is none.
 *
 * Defaults to `system` rather than `dark` so a first run defers to the machine
 * instead of overriding it — the app has no opinion until someone expresses
 * one.
 */
export function readTheme(): Theme {
  try {
    const stored = localStorage.getItem(KEY);
    return isTheme(stored) ? stored : "system";
  } catch {
    // Private browsing, or storage disabled. Falling back is correct; failing
    // to render because we could not read a preference is not.
    return "system";
  }
}

/** Write the choice to `<html data-theme>`, which is what the CSS reads. */
export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
}

/** Remember the choice, and apply it. */
export function setTheme(theme: Theme): void {
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // An unstorable preference should still take effect for this session.
  }
  applyTheme(theme);
}
