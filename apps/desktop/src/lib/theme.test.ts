import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it } from "vitest";
import { applyTheme, readTheme, setTheme, THEMES } from "./theme";

// Read from disk rather than importing. Vitest stubs CSS imports to the empty
// string, so `import styles from "../styles.css?raw"` yields "" and every
// assertion below would pass against nothing — which is the failure mode this
// file exists to catch, so it must not be the one it ships with.
const STYLES = readFileSync(resolve(process.cwd(), "src/styles.css"), "utf8");

describe("the colour tokens", () => {
  // The point of `light-dark()` is that a token cannot exist in one scheme and
  // be absent from the other. Before it, the light scheme was a second copy of
  // the palette inside a media query, and three status colours were simply
  // missing from that copy — invisible until someone opened the app in
  // daylight. Asserting on the mechanism rather than on a list of names is
  // what makes the guarantee survive the next token anyone adds.
  it("declare every colour in both schemes, via light-dark()", () => {
    const theme = STYLES.slice(STYLES.indexOf("@theme"));
    const declarations = [...theme.matchAll(/^\s*(--color-[\w-]+):\s*([^;]+);/gm)];

    expect(declarations.length).toBeGreaterThan(10);

    const oneSidedOnly = declarations
      .filter(([, , value]) => !value!.includes("light-dark("))
      .map(([, name]) => name);

    expect(oneSidedOnly).toEqual([]);
  });

  it("has no second palette hidden in a prefers-color-scheme block", () => {
    // The old shape. If one comes back, the light and dark halves can drift
    // again and this file stops being the single declaration it claims to be.
    //
    // Matching the at-rule, not the word: the file's own header explains what
    // it replaced, and a test that cannot tell a comment from a media query
    // would forbid writing down the reason.
    expect(STYLES).not.toMatch(/@media[^{]*prefers-color-scheme/);
  });

  it("switches every scheme through color-scheme, for all three choices", () => {
    for (const theme of THEMES) {
      expect(STYLES).toContain(`:root[data-theme="${theme}"]`);
    }
  });
});

describe("the theme the user chose", () => {
  beforeEach(() => {
    localStorage.clear();
    delete document.documentElement.dataset.theme;
  });

  it("defers to the system when nobody has chosen", () => {
    // Not `dark`. A first run should follow the machine rather than override
    // it; the app has no opinion until someone expresses one.
    expect(readTheme()).toBe("system");
  });

  it("remembers a choice and writes it where the CSS reads it", () => {
    setTheme("light");
    expect(readTheme()).toBe("light");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("falls back to system when the stored value is not a theme", () => {
    // A value left by an older build, or edited by hand. Rendering with no
    // colour scheme at all would be worse than ignoring it.
    localStorage.setItem("keel.theme", "solarized");
    expect(readTheme()).toBe("system");
  });

  it("applies without storing, so a read-only localStorage still themes", () => {
    applyTheme("dark");
    expect(document.documentElement.dataset.theme).toBe("dark");
  });
});
