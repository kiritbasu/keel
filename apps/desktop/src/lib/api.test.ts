/**
 * The change stream.
 *
 * Notes are announced under their own kind because they are not events: the
 * daemon announces an entity change when the latest event id advances, and a
 * note leaves no row there. Before that existed, writing a note refreshed
 * nothing and an open app showed a stale note stream with no sign it was stale
 * (TQ-29).
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { subscribe, type ChangeEvent } from "./api";

class FakeEventSource {
  static last: FakeEventSource | null = null;
  closed = false;
  listeners = new Map<string, Array<(e: unknown) => void>>();
  constructor(readonly url: string) {
    FakeEventSource.last = this;
  }
  addEventListener(type: string, fn: (e: unknown) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), fn]);
  }
  close() {
    this.closed = true;
  }
  emit(type: string, data?: string) {
    for (const fn of this.listeners.get(type) ?? []) fn({ data });
  }
}

vi.stubGlobal("EventSource", FakeEventSource);
afterEach(() => {
  FakeEventSource.last = null;
});

describe("subscribe", () => {
  it("reports a note under its own kind, with the row it is about", () => {
    const seen: ChangeEvent[] = [];
    subscribe((c) => seen.push(c));
    FakeEventSource.last!.emit(
      "change",
      JSON.stringify({ kind: "note", entity_id: "tsk_1", summary: "keel_note completed" }),
    );
    expect(seen).toHaveLength(1);
    expect(seen[0]!.kind).toBe("note");
    expect(seen[0]!.entity_id).toBe("tsk_1");
  });

  it("reports an ordinary write as an entity change", () => {
    const seen: ChangeEvent[] = [];
    subscribe((c) => seen.push(c));
    FakeEventSource.last!.emit(
      "change",
      JSON.stringify({ kind: "entity", event_id: "evt_1", summary: "keel_update completed" }),
    );
    expect(seen[0]!.kind).toBe("entity");
  });

  it("still fires when the payload cannot be parsed", () => {
    // Refetching on an unreadable change is the safe direction: the cost is one
    // wasted read, and the alternative is showing stale state because a payload
    // shape moved.
    const seen: ChangeEvent[] = [];
    subscribe((c) => seen.push(c));
    FakeEventSource.last!.emit("change", "{ not json");
    expect(seen).toHaveLength(1);
    expect(seen[0]!.kind).toBe("entity");
  });

  it("treats lagging as a change, because a client that missed messages must refetch", () => {
    const seen: ChangeEvent[] = [];
    subscribe((c) => seen.push(c));
    FakeEventSource.last!.emit("lagged", JSON.stringify({ missed: 12 }));
    expect(seen).toHaveLength(1);
  });

  it("closes the stream when unsubscribed", () => {
    const stop = subscribe(() => {});
    stop();
    expect(FakeEventSource.last!.closed).toBe(true);
  });
});

describe("subscribe, on reconnect", () => {
  it("refetches when the stream opens, because a reconnect announces nothing", () => {
    const seen: ChangeEvent[] = [];
    subscribe((c) => seen.push(c));

    // The first connect. Redundant with the initial load and harmless.
    FakeEventSource.last!.emit("open");
    expect(seen).toHaveLength(1);

    // A drop and a reconnect, with writes having happened in between that
    // nobody will ever announce. Without this the app sits on stale data until
    // some unrelated write arrives — which is how a task can exist in the
    // store, in the API and in the ranking, and not be on the board.
    FakeEventSource.last!.emit("error");
    FakeEventSource.last!.emit("open");
    expect(seen).toHaveLength(2);
  });

  it("says when the feed is down, and when it comes back", () => {
    const status: string[] = [];
    subscribe(
      () => {},
      (s) => status.push(s),
    );
    expect(status).toEqual(["connecting"]);

    FakeEventSource.last!.emit("open");
    FakeEventSource.last!.emit("error");
    FakeEventSource.last!.emit("open");

    expect(status).toEqual(["connecting", "live", "down", "live"]);
  });

  it("does not require a status callback", () => {
    const seen: ChangeEvent[] = [];
    expect(() => {
      subscribe((c) => seen.push(c));
      FakeEventSource.last!.emit("error");
      FakeEventSource.last!.emit("open");
    }).not.toThrow();
    expect(seen).toHaveLength(1);
  });
});
