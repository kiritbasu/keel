/**
 * Which project you were last in.
 *
 * The navigation used to list eight screens with the project list *below*
 * them, and five of those eight need a project — so a cold launch put you in
 * front of a menu that was mostly dead, and the one control that would fix it
 * was the thing you had to scroll past them to reach.
 *
 * Remembering the project is what removes that state rather than styling it.
 * A project is always selected, so a screen that needs one always has one, and
 * the disabled branch stops being reachable instead of being drawn at 35%
 * opacity with a tooltip apologising for itself.
 */

const KEY = "specline.lastProject";

/** The slug you were last in, or null if there is no usable memory. */
export function readLastProject(): string | null {
  try {
    return localStorage.getItem(KEY) || null;
  } catch {
    // Private browsing, or storage disabled. Falling back to "no memory" is
    // correct; failing to render because we could not read a preference is not.
    return null;
  }
}

export function rememberProject(slug: string): void {
  try {
    localStorage.setItem(KEY, slug);
  } catch {
    // The session still works, it just will not be remembered next launch.
  }
}

/**
 * The project to open on a cold launch, given what exists right now.
 *
 * Checked against the live list rather than trusted: a remembered slug can name
 * a project that has since been archived or renamed, and navigating to one
 * would land on an empty screen under a URL that promises content. Falling
 * through to the first project is better than honouring a stale memory, and
 * returning null when there are none at all is what lets the caller show an
 * empty store honestly instead of inventing a destination.
 */
export function defaultProject(slugs: string[]): string | null {
  const remembered = readLastProject();
  if (remembered && slugs.includes(remembered)) return remembered;
  return slugs[0] ?? null;
}
