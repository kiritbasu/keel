/**
 * Finding the project a reference names.
 *
 * The daemon accepts four spellings of the same project — its id, its slug, its
 * name, or any of its aliases — and it does so case-insensitively
 * (`resolve_project` in `specline-mcp`). The app has to accept the same four,
 * because it is looking up the very reference it just sent to the daemon.
 *
 * It did not, and matched on `slug` alone. That is invisible until a reference
 * arrives that is not the slug, and then it fails in the worst available
 * direction: the fetch succeeds, so the screen fills with the right data, and
 * only the things that come off the *matched row* go missing. On
 * `/projects/keel/…` — the alias the rename left behind so old links keep
 * working — every task lost its `KEEL-311` and showed a raw ULID, and the
 * project's word for a milestone reverted from "Phase" to "Milestone"
 * (KEEL-312).
 *
 * So this exists to be the single answer, rather than four callers each
 * deciding. If the daemon's rule ever grows a fifth spelling, this is the
 * function that has to learn it.
 */

import type { Entity } from "./api";

/** Case-insensitive, and tolerant of the field being absent. */
function matches(value: unknown, needle: string): boolean {
  return typeof value === "string" && value.toLowerCase() === needle;
}

/**
 * The project row a reference names, or `undefined` if none does.
 *
 * `reference` is whatever the address, the stored last project, or a link
 * happened to carry — it is not assumed to be a slug.
 */
export function findProject(
  projects: Entity[] | undefined,
  reference: string | undefined,
): Entity | undefined {
  if (!reference) return undefined;
  const needle = reference.toLowerCase();

  return (projects ?? []).find(
    (p) =>
      // The id is exact rather than folded: it is a ULID, and a
      // case-insensitive comparison of one would only hide a malformed
      // reference.
      p.id === reference ||
      matches(p.slug, needle) ||
      matches(p.name, needle) ||
      (Array.isArray(p.aliases) &&
        p.aliases.some((alias) => matches(alias, needle))),
  );
}

/**
 * The `KEEL` of `KEEL-42`, for whichever project the reference names.
 *
 * `undefined` when the project has no key, which is a real state — a project
 * created without one shows its rows by id, and that is the fallback
 * `taskRef` already implements.
 */
export function keyOf(
  projects: Entity[] | undefined,
  reference: string | undefined,
): string | undefined {
  const key = findProject(projects, reference)?.key;
  return typeof key === "string" && key.trim() ? key.trim() : undefined;
}
