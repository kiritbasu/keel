/**
 * Resolving a project reference the way the daemon does.
 *
 * The bug this exists for was not that the lookup was wrong in an obvious way
 * — it was that it was a *subset* of the daemon's. The fetch used the daemon's
 * rule and succeeded; the lookup used a narrower one and failed; and the screen
 * filled with correct data in which every task reference had quietly become a
 * ULID (KEEL-312).
 *
 * So most of what follows is the four spellings, one test each, because any of
 * them silently going missing again looks exactly like nothing being wrong.
 */

import { describe, expect, it } from "vitest";
import { findProject, keyOf } from "./projects";
import type { Entity } from "./api";

const SPECLINE = {
  id: "prj_01KZKMPVHJ",
  type: "project",
  slug: "specline",
  name: "Specline",
  key: "KEEL",
  aliases: ["keel", "the project spine"],
  milestone_noun: "Phase",
  audit: {},
} as unknown as Entity;

const OTHER = {
  id: "prj_other",
  type: "project",
  slug: "tradecraft",
  name: "Tradecraft Academy",
  key: "TRAD",
  aliases: [],
  audit: {},
} as unknown as Entity;

const PROJECTS = [OTHER, SPECLINE];

describe("findProject", () => {
  it("finds it by slug", () => {
    expect(findProject(PROJECTS, "specline")?.id).toBe(SPECLINE.id);
  });

  it("finds it by id", () => {
    expect(findProject(PROJECTS, "prj_01KZKMPVHJ")?.id).toBe(SPECLINE.id);
  });

  it("finds it by name", () => {
    expect(findProject(PROJECTS, "Specline")?.id).toBe(SPECLINE.id);
  });

  /** The one that was missing, and the one the rename made load-bearing. */
  it("finds it by an alias", () => {
    expect(findProject(PROJECTS, "keel")?.id).toBe(SPECLINE.id);
    expect(findProject(PROJECTS, "the project spine")?.id).toBe(SPECLINE.id);
  });

  it("does not care about case, because the daemon does not", () => {
    for (const reference of ["SPECLINE", "KeEl", "SPECLINE", "Keel"]) {
      expect(findProject(PROJECTS, reference)?.id).toBe(SPECLINE.id);
    }
  });

  it("finds nothing for a reference no project answers to", () => {
    expect(findProject(PROJECTS, "nothing-by-that-name")).toBeUndefined();
    expect(findProject(PROJECTS, "")).toBeUndefined();
    expect(findProject(PROJECTS, undefined)).toBeUndefined();
    expect(findProject(undefined, "specline")).toBeUndefined();
  });

  /** A row missing the fields entirely must not throw on the way past. */
  it("survives a project row with nothing on it", () => {
    const bare = { id: "prj_bare", type: "project", audit: {} } as Entity;
    expect(findProject([bare], "specline")).toBeUndefined();
    expect(findProject([bare], "prj_bare")?.id).toBe("prj_bare");
  });
});

describe("keyOf", () => {
  it("gives the key whichever spelling was used to ask", () => {
    for (const reference of ["specline", "keel", "Specline", "prj_01KZKMPVHJ"]) {
      expect(keyOf(PROJECTS, reference)).toBe("KEEL");
    }
  });

  /**
   * A project without a key is a real state, and `taskRef` already falls back
   * to the id for it. What must not happen is the fallback firing for a project
   * that *has* a key, which is what KEEL-312 was.
   */
  it("gives nothing when the project has no key", () => {
    const keyless = {
      id: "prj_k",
      type: "project",
      slug: "keyless",
      key: "  ",
      audit: {},
    } as unknown as Entity;
    expect(keyOf([keyless], "keyless")).toBeUndefined();
    expect(keyOf(PROJECTS, "no-such-project")).toBeUndefined();
  });
});
