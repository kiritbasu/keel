import { beforeEach, describe, expect, it } from "vitest";
import { defaultProject, readLastProject, rememberProject } from "./lastProject";

describe("the project you were last in", () => {
  beforeEach(() => localStorage.clear());

  it("has no memory before anything is remembered", () => {
    expect(readLastProject()).toBeNull();
  });

  it("round-trips a slug", () => {
    rememberProject("specline");
    expect(readLastProject()).toBe("specline");
  });

  it("opens what you were last in", () => {
    rememberProject("specline");
    expect(defaultProject(["audiosplatcamera", "specline"])).toBe("specline");
  });

  it("opens the first project when there is no memory", () => {
    expect(defaultProject(["audiosplatcamera", "specline"])).toBe("audiosplatcamera");
  });

  // Failure case, and the reason the live list is passed in rather than
  // trusted: a remembered project can be archived or renamed between sessions.
  // Honouring the memory would open an empty screen under an address that
  // promises content, which is worse than opening a different real project.
  it("ignores a memory of a project that no longer exists", () => {
    rememberProject("deleted-thing");
    expect(defaultProject(["specline"])).toBe("specline");
  });

  // Failure case: an empty store has no project to fall back on, and inventing
  // one would send the rail at an address that cannot render.
  it("returns null when there are no projects at all", () => {
    rememberProject("specline");
    expect(defaultProject([])).toBeNull();
  });
});
