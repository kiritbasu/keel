/**
 * A JSX attribute is not a JavaScript string.
 *
 * `title="What’s next"` looks like every other string in the file and is
 * not one: JSX treats a double-quoted attribute value like an HTML attribute,
 * so an escape sequence inside it is six characters of text. The same phrase
 * written `crumbs={projectCrumbs(route, "What’s next")}` *is* a string,
 * because the braces make it JavaScript again.
 *
 * Both spellings sat four lines apart in `Next.tsx` for months. The breadcrumb
 * read `What’s next` and the heading above it read `What’s next`, on the
 * screen a person opens second (KEEL-310).
 *
 * Nothing else catches this. It is valid JSX, it type-checks, it renders, and
 * it renders wrong — so the guard is a scan of the source, in the manner
 * `theme.test.ts` already uses for a property that is equally invisible to the
 * compiler.
 *
 * The fix in every case is to write the character itself. That is what the
 * codebase does elsewhere — `“{query.trim()}”` in `LabelPicker` — and it is
 * why this test bans the escape rather than teaching people which of the two
 * syntaxes processes it.
 */

import { readFileSync, readdirSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

function sourceFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    // Tests are excluded: one of them has to be able to write the escape in
    // order to assert that the rendered output does not contain it.
    return entry.name.endsWith(".tsx") && !entry.name.includes(".test.")
      ? [path]
      : [];
  });
}

/**
 * `foo="…\uXXXX…"`, and only in that position.
 *
 * A backslash-u inside braces or in a plain `.ts` file is an ordinary escape
 * and works, so flagging those would be noise that gets the rule switched off.
 */
const ESCAPE_IN_ATTRIBUTE = /[a-zA-Z-]+="[^"\n]*\\u[0-9a-fA-F]{4}/g;

describe("unicode escapes in JSX attributes", () => {
  it("do not appear, because they would print as text", () => {
    const offenders = sourceFiles(resolve(process.cwd(), "src")).flatMap(
      (path) => {
        const matches = readFileSync(path, "utf8").match(ESCAPE_IN_ATTRIBUTE);
        return matches ? matches.map((m) => `${path}: ${m}`) : [];
      },
    );

    expect(
      offenders,
      "a JSX attribute does not process escape sequences — write the character",
    ).toEqual([]);
  });

  /** The scan has to be able to find one, or it is asserting nothing. */
  it("is a check that would actually fire", () => {
    expect('title="What\\u2019s next"'.match(ESCAPE_IN_ATTRIBUTE)).toHaveLength(
      1,
    );
    // And leaves alone the two spellings that are real strings.
    expect("{fn(a, 'What\\u2019s next')}".match(ESCAPE_IN_ATTRIBUTE)).toBeNull();
    expect('const a = "What\\u2019s next";'.match(ESCAPE_IN_ATTRIBUTE)).toBeNull();
  });
});
