import { describe, expect, it } from "vitest";
import { groupRank, relationPhrase } from "./relations";

describe("relationPhrase", () => {
  // The property that matters. One stored verb, two sentences, and only one of
  // them is true at each end — printing `blocks` on both sides states half the
  // relationships backwards, which is the same failure mode as an inverted
  // traversal one layer up: confident, readable, and wrong.
  it("says the opposite thing at each end of the same edge", () => {
    expect(relationPhrase("blocks", "outbound")).toBe("Blocks");
    expect(relationPhrase("blocks", "inbound")).toBe("Blocked by");
    expect(relationPhrase("blocks", "outbound")).not.toBe(relationPhrase("blocks", "inbound"));
  });

  it("covers every relation that can be stored", () => {
    // `depends_on` is deliberately absent: specline-core normalises it to `blocks`
    // with the endpoints swapped, so it can never come back from a traversal.
    for (const rel of [
      "blocks",
      "implements",
      "supersedes",
      "derived_from",
      "resolves",
      "references",
      "duplicates",
      "informs",
    ]) {
      const out = relationPhrase(rel, "outbound");
      const inb = relationPhrase(rel, "inbound");
      expect(out).not.toBe(rel);
      expect(inb).not.toBe(out);
    }
  });

  // Failure case: a relation added later must look untranslated, not vanish.
  // Returning nothing would hide a real edge behind a blank heading.
  it("falls back to the stored verb for a relation it has never seen", () => {
    expect(relationPhrase("mentions", "outbound")).toBe("mentions");
    expect(relationPhrase("mentions", "inbound")).toBe("mentions (incoming)");
  });
});

describe("groupRank", () => {
  it("puts what is holding this up first", () => {
    expect(groupRank("blocks", "inbound")).toBeLessThan(groupRank("blocks", "outbound"));
    expect(groupRank("blocks", "outbound")).toBeLessThan(groupRank("implements", "outbound"));
  });

  it("sends anything unlisted to the end rather than to the front", () => {
    expect(groupRank("mentions", "outbound")).toBeGreaterThan(groupRank("duplicates", "inbound"));
  });
});
