import { beforeEach, describe, expect, it } from "vitest";
import { defaultProject, readLastProject, rememberProject } from "./lastProject";

describe("the project you were last in", () => {
  beforeEach(() => localStorage.clear());

  it("has no memory before anything is remembered", () => {
    expect(readLastProject()).toBeNull();
  });

  it("round-trips a slug", () => {
    rememberProject("keel");
    expect(readLastProject()).toBe("keel");
  });

  it("opens what you were last in", () => {
    rememberProject("keel");
    expect(defaultProject(["audiosplatcamera", "keel"])).toBe("keel");
  });

  it("opens the first project when there is no memory", () => {
    expect(defaultProject(["audiosplatcamera", "keel"])).toBe("audiosplatcamera");
  });

  // Failure case, and the reason the live list is passed in rather than
  // trusted: a remembered project can be archived or renamed between sessions.
  // Honouring the memory would open an empty screen under an address that
  // promises content, which is worse than opening a different real project.
  it("ignores a memory of a project that no longer exists", () => {
    rememberProject("deleted-thing");
    expect(defaultProject(["keel"])).toBe("keel");
  });

  // Failure case: an empty store has no project to fall back on, and inventing
  // one would send the rail at an address that cannot render.
  it("returns null when there are no projects at all", () => {
    rememberProject("keel");
    expect(defaultProject([])).toBeNull();
  });
});
