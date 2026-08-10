/**
 * Relations, said in English.
 *
 * A graph edge stored as `blocks` means two different sentences depending on
 * which end you are standing at, and only one of them is ever true for the
 * reader: walking *out* of a task, `blocks` means "this must happen before";
 * walking *in*, it means "this is waiting on". Rendering the stored verb in
 * both places — which is what showing a `blocks` badge does — states the
 * relationship backwards half the time.
 *
 * This is the same hazard as an inverted traversal (see SPEC §3.3), one layer
 * up: it produces something that reads as confident and correct and is exactly
 * wrong.
 */

/** Which way an edge was walked to reach the neighbour. */
export type Direction = "outbound" | "inbound";

/**
 * The heading a group of neighbours sits under.
 *
 * Outbound is the subject doing the verb; inbound is the subject receiving it.
 * `depends_on` never appears — `keel-core` normalises it to `blocks` with the
 * endpoints swapped, so it can never be stored and can never come back.
 */
const PHRASES: Record<string, { outbound: string; inbound: string }> = {
  blocks: { outbound: "Blocks", inbound: "Blocked by" },
  implements: { outbound: "Implements", inbound: "Implemented by" },
  supersedes: { outbound: "Supersedes", inbound: "Superseded by" },
  derived_from: { outbound: "Derived from", inbound: "Basis for" },
  resolves: { outbound: "Resolves", inbound: "Resolved by" },
  references: { outbound: "References", inbound: "Referenced by" },
  duplicates: { outbound: "Duplicates", inbound: "Duplicated by" },
  informs: { outbound: "Informs", inbound: "Informed by" },
};

/**
 * How to head a group of neighbours reached by `rel` in `direction`.
 *
 * An unknown relation falls back to the stored verb rather than to nothing: a
 * relation added later should look untranslated, not invisible.
 */
export function relationPhrase(rel: string, direction: Direction): string {
  const phrase = PHRASES[rel];
  if (!phrase) return direction === "outbound" ? rel : `${rel} (incoming)`;
  return phrase[direction];
}

/**
 * The order relationship groups appear in.
 *
 * What is holding this up comes first, because that is the question a reader
 * opening a task most often has.
 */
const ORDER = [
  "blocks:inbound",
  "blocks:outbound",
  "implements:outbound",
  "implements:inbound",
  "duplicates:outbound",
  "duplicates:inbound",
];

export function groupRank(rel: string, direction: Direction): number {
  const at = ORDER.indexOf(`${rel}:${direction}`);
  return at === -1 ? ORDER.length : at;
}
