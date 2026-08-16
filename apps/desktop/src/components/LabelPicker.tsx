/**
 * Choosing labels by typing.
 *
 * This replaced ten chips and a line reading "the 10 most used of 64 — ask
 * Claude for any of the others", which is a cap standing in for a search box.
 * All of them are reachable now, and the way you reach one is the way you were
 * always going to try first.
 *
 * # Only labels that already exist
 *
 * There is no "create «foo»" here, and that is deliberate rather than
 * unfinished. A free-text label box is how a set becomes `ui`, `UI` and `ui `
 * inside a month, and nothing downstream can tell those apart — the board's
 * facets, the filters and `specline_ready` all treat them as three labels. When
 * something genuinely needs a new one, Claude adds it in the conversation where
 * the reason for it exists, which is also where somebody will later ask what it
 * means.
 *
 * The empty state says so, rather than leaving a person typing into a box that
 * silently refuses them.
 */

import { useMemo, useRef, useState } from "react";
import { cx } from "./ui";

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

  const matches = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return (
      available
        .filter((label) => !chosen.includes(label))
        .filter((label) => !needle || label.toLowerCase().includes(needle))
        // Eight is what fits without the list becoming the dialog. Typing
        // narrows, so the way to see the ninth is to type — which is the whole
        // point of this being a search rather than a menu.
        .slice(0, 8)
    );
  }, [available, chosen, query]);

  function add(label: string) {
    if (!label || chosen.includes(label)) return;
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
              setHighlighted((h) => Math.min(h + 1, matches.length - 1));
            } else if (e.key === "ArrowUp") {
              e.preventDefault();
              setHighlighted((h) => Math.max(h - 1, 0));
            } else if (e.key === "Enter") {
              if (matches[highlighted]) {
                e.preventDefault();
                e.stopPropagation();
                add(matches[highlighted]);
              }
            } else if (e.key === "Backspace" && query === "" && chosen.length) {
              // The behaviour every chip input has, and its absence is felt.
              remove(chosen[chosen.length - 1]!);
            }
          }}
          placeholder="Type to find a label"
          aria-label="Find a label"
          role="combobox"
          aria-expanded={matches.length > 0}
          aria-controls="label-suggestions"
          className="w-full rounded-md border border-border-subtle bg-surface px-3 py-1.5 text-small text-ink placeholder:text-ink-faint"
        />

        {matches.length > 0 && (
          <ul
            id="label-suggestions"
            className="mt-1 flex flex-wrap gap-1.5"
            role="listbox"
          >
            {matches.map((label, i) => (
              <li key={label} role="option" aria-selected={i === highlighted}>
                <button
                  type="button"
                  onMouseEnter={() => setHighlighted(i)}
                  onClick={() => add(label)}
                  className={cx(
                    "rounded-full border px-2 py-0.5 text-micro transition-colors",
                    i === highlighted
                      ? "border-accent/60 bg-accent/10 text-accent"
                      : "border-border-subtle text-ink-faint hover:text-ink",
                  )}
                >
                  {label}
                </button>
              </li>
            ))}
          </ul>
        )}

        {query.trim() !== "" && matches.length === 0 && (
          <p className="mt-1 text-micro text-ink-faint">
            No label matches “{query.trim()}”. Ask Claude to add a new one —
            labels are only created where the reason for them is.
          </p>
        )}
      </div>
    </div>
  );
}
