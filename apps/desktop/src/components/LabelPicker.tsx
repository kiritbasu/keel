/**
 * Choosing labels by typing, and creating one when none of them fit.
 *
 * This replaced ten chips and a line reading "the 10 most used of 64 — ask
 * Claude for any of the others", which is a cap standing in for a search box.
 * All of them are reachable now, and the way you reach one is the way you were
 * always going to try first.
 *
 * # Creating a label here
 *
 * For a while there was no "create «foo»", deliberately: a free-text label box
 * is how a set becomes `ui`, `UI` and `ui ` inside a month, and nothing
 * downstream can tell those apart — the board's facets, the filters and
 * `specline_next` all treat them as three labels. The answer was to refuse, and
 * send you to Claude for a one-word tag.
 *
 * That detour cost more than the thing it prevented (KEEL-304). So the box
 * creates labels now, and the fragmentation is handled where it actually
 * arises: `normaliseLabel` folds `Data Safety`, `DATA-SAFETY` and `data safety `
 * onto the one form, and a candidate that normalises onto a label that already
 * exists is not offered as new — the existing one is offered instead. Typing
 * cannot produce a twin.
 *
 * The normalisation is **only here**. `specline-core`, MCP and the CLI still take
 * a label exactly as given, because a store that quietly rewrites what a caller
 * asked for is the silent-correction shape this codebase keeps having to undo.
 * Claude can see the existing set and is trusted to match it; this box is for
 * the person who cannot.
 *
 * There is no label registry to add to, and that is why "so it autocompletes
 * next time" needs no code: `available` is derived from the labels the loaded
 * tasks carry, so a label exists exactly as long as something is tagged with it.
 */

import { useMemo, useRef, useState } from "react";
import { cx } from "./ui";

/**
 * The one form a typed label is allowed to take.
 *
 * Lowercase and hyphenated, which is what all 75 labels in use already are —
 * so this codifies the set rather than imposing on it, and no existing label
 * needs migrating. Punctuation is left alone on purpose: the rule exists to
 * stop case and spacing splitting one label into three, and stripping anything
 * else would be inventing policy the label set never asked for.
 *
 * Returns `""` for input with nothing label-shaped in it, which the caller
 * treats as "not creatable" rather than creating an empty label.
 */
export function normaliseLabel(raw: string): string {
  return raw
    .trim()
    .toLowerCase()
    .replace(/\s+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
}

type Suggestion = {
  label: string;
  /** True when taking this makes a label that does not exist yet. */
  create: boolean;
};

export function LabelPicker({
  available,
  chosen,
  onChange,
}: {
  /** Every label in use on this project. Scoped by the caller's task query. */
  available: string[];
  chosen: string[];
  onChange: (labels: string[]) => void;
}) {
  const [query, setQuery] = useState("");
  const [highlighted, setHighlighted] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  // What the typed text would become as a label, which is also what it is
  // matched by: typing "Data Safety" has to find `data-safety`, or the box
  // would offer to create the label you are looking at.
  const needle = normaliseLabel(query);
  const typed = query.trim() !== "";

  // What is on the task already, folded. Compared this way rather than by
  // string because `available` is not guaranteed normalised — MCP writes a
  // label verbatim, by design — so an `UI` from there and a `ui` chosen here
  // are one label, and an exact-match check would let both onto the one task.
  const chosenKeys = useMemo(
    () => new Set(chosen.map(normaliseLabel)),
    [chosen],
  );

  const suggestions = useMemo<Suggestion[]>(() => {
    // Typed text that normalises to nothing is not the same as nothing typed,
    // and the difference is not cosmetic: an empty needle matches everything,
    // so "---" would offer the unfiltered list and Enter would take whichever
    // label happened to be first.
    if (typed && needle === "") return [];

    const matches = available
      .filter((label) => !chosenKeys.has(normaliseLabel(label)))
      .filter((label) => !needle || normaliseLabel(label).includes(needle))
      // Eight is what fits without the list becoming the dialog. Typing
      // narrows, so the way to see the ninth is to type — which is the whole
      // point of this being a search rather than a menu.
      .slice(0, 8)
      .map((label) => ({ label, create: false }));

    // Offered last, so the labels that already exist are what the eye and the
    // first Enter land on. Suppressed when anything — chosen or not — already
    // folds onto it, which is what makes a twin unreachable.
    const taken =
      chosenKeys.has(needle) ||
      available.some((label) => normaliseLabel(label) === needle);
    return needle !== "" && !taken
      ? [...matches, { label: needle, create: true }]
      : matches;
  }, [available, chosenKeys, needle, typed]);

  function add(label: string) {
    if (!label || chosenKeys.has(normaliseLabel(label))) return;
    onChange([...chosen, label]);
    setQuery("");
    setHighlighted(0);
    inputRef.current?.focus();
  }

  function remove(label: string) {
    onChange(chosen.filter((l) => l !== label));
  }

  return (
    <div className="space-y-1">
      <span className="text-micro text-ink-muted">Labels</span>

      {chosen.length > 0 && (
        <div className="flex flex-wrap gap-1.5 pb-1">
          {chosen.map((label) => (
            <button
              key={label}
              type="button"
              onClick={() => remove(label)}
              aria-label={`Remove ${label}`}
              className="rounded-full border border-accent/60 bg-accent/15 px-2 py-0.5 text-micro text-accent hover:bg-accent/25"
            >
              {label} ×
            </button>
          ))}
        </div>
      )}

      <div className="relative">
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setHighlighted(0);
          }}
          // Arrow keys and Enter, because a list you can only reach with the
          // mouse is not much better than the chips it replaced. Enter must not
          // reach the dialog's submit while a suggestion is highlighted, or
          // picking a label would create the task.
          onKeyDown={(e) => {
            if (e.key === "ArrowDown") {
              e.preventDefault();
              setHighlighted((h) => Math.min(h + 1, suggestions.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setHighlighted((h) => Math.max(h - 1, 0));
            } else if (e.key === "Enter") {
              if (suggestions[highlighted]) {
                e.preventDefault();
                e.stopPropagation();
                add(suggestions[highlighted].label);
              }
            } else if (e.key === "Backspace" && query === "" && chosen.length) {
              // The behaviour every chip input has, and its absence is felt.
              remove(chosen[chosen.length - 1]!);
            }
          }}
          placeholder="Type to find or add a label"
          aria-label="Find or add a label"
          role="combobox"
          aria-expanded={suggestions.length > 0}
          aria-controls="label-suggestions"
          className="w-full rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-small text-ink placeholder:text-ink-faint"
        />

        {suggestions.length > 0 && (
          <ul
            id="label-suggestions"
            className="mt-1 flex flex-wrap gap-1.5"
            role="listbox"
          >
            {suggestions.map((suggestion, i) => (
              <li
                key={
                  suggestion.create
                    ? `create:${suggestion.label}`
                    : suggestion.label
                }
                role="option"
                aria-selected={i === highlighted}
              >
                <button
                  type="button"
                  onMouseEnter={() => setHighlighted(i)}
                  onClick={() => add(suggestion.label)}
                  className={cx(
                    "rounded-full border px-2 py-0.5 text-micro transition-colors",
                    suggestion.create && "border-dashed",
                    i === highlighted
                      ? "border-accent/60 bg-accent/10 text-accent"
                      : "border-border-subtle text-ink-faint hover:text-ink",
                  )}
                >
                  {/* The normalised form, shown rather than applied silently —
                      typing "Data Safety" and getting `data-safety` is only a
                      surprise if the button did not say so. */}
                  {suggestion.create
                    ? `Create “${suggestion.label}”`
                    : suggestion.label}
                </button>
              </li>
            ))}
          </ul>
        )}

        {/* Two ways to type something and be offered nothing, and they are not
            the same problem — one is already done, the other cannot be done. */}
        {needle !== "" && suggestions.length === 0 && (
          <p className="mt-1 text-micro text-ink-faint">
            “{needle}” is already on this task.
          </p>
        )}
        {typed && needle === "" && (
          <p className="mt-1 text-micro text-ink-faint">
            “{query.trim()}” has nothing in it that could be a label.
          </p>
        )}
      </div>
    </div>
  );
}
