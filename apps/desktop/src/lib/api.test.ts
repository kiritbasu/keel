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
import { api, subscribe, type ChangeEvent } from "./api";

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
      JSON.stringify({
        kind: "note",
        entity_id: "tsk_1",
        summary: "specline_note completed",
      }),
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
      JSON.stringify({
        kind: "entity",
        event_id: "evt_1",
        summary: "specline_update completed",
      }),
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

// --- Writing, and the token that has to be current ------------------------

describe("a write from the interface", () => {
  const TOKEN_META = '<meta name="specline-token" content="fresh-token">';

  function pageWithToken(token: string) {
    document.head.innerHTML = `<meta name="specline-token" content="${token}">`;
  }

  afterEach(() => {
    document.head.innerHTML = "";
    vi.unstubAllGlobals();
  });

  it("sends the token the daemon put in the page", async () => {
    pageWithToken("the-current-token");
    const calls: Array<{ url: string; headers: Record<string, string> }> = [];
    vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
      calls.push({
        url: String(url),
        headers: (init.headers ?? {}) as Record<string, string>,
      });
      return new Response(JSON.stringify({ data: { id: "nte_1" } }), {
        status: 200,
      });
    });

    await api.addNote("tsk_1", "something learned");

    expect(calls[0]?.headers["x-specline-token"]).toBe("the-current-token");
  });

  /**
   * A token lives as long as one daemon, and `specline update` restarts the daemon
   * — so a page left open across an update is holding an expired secret. Every
   * button on it would fail with a 401 that reads like a broken app.
   *
   * Re-reading the token from this origin is exactly as safe as the original
   * delivery: only a page already on this origin can read that response.
   */
  it("fetches a fresh token and retries when the daemon says the old one is dead", async () => {
    pageWithToken("a-token-from-a-dead-daemon");
    const sent: string[] = [];
    vi.stubGlobal("fetch", async (_url: string, init?: RequestInit) => {
      // The re-read of the served document.
      if (!init?.method) {
        return new Response(`<!doctype html><head>${TOKEN_META}</head>`, {
          status: 200,
        });
      }
      const token =
        (init.headers as Record<string, string>)["x-specline-token"] ?? "";
      sent.push(token);
      return token === "fresh-token"
        ? new Response(JSON.stringify({ data: { id: "nte_1" } }), {
            status: 200,
          })
        : new Response(JSON.stringify({ error: "stale" }), { status: 401 });
    });

    await api.addNote("tsk_1", "written across a restart");

    expect(sent).toEqual(["a-token-from-a-dead-daemon", "fresh-token"]);
  });

  /** Two 401s is a real refusal, not a stale secret. It must not loop. */
  it("gives up after one retry", async () => {
    pageWithToken("no-good");
    let posts = 0;
    vi.stubGlobal("fetch", async (_url: string, init?: RequestInit) => {
      if (!init?.method) {
        return new Response(
          `<!doctype html><head><meta name="specline-token" content="also-no-good"></head>`,
          { status: 200 },
        );
      }
      posts += 1;
      return new Response(JSON.stringify({ error: "nope" }), { status: 401 });
    });

    await expect(api.addNote("tsk_1", "x")).rejects.toThrow();
    expect(posts).toBe(2);
  });

  /**
   * A page the daemon did not serve has no token and must not pretend. This is
   * the dev server, and it is also any page that reached this origin some other
   * way — the message says what to do rather than failing as a 401.
   */
  it("refuses before sending anything when the page carries no token", async () => {
    document.head.innerHTML = "";
    let called = false;
    vi.stubGlobal("fetch", async () => {
      called = true;
      return new Response("{}", { status: 200 });
    });

    await expect(api.addNote("tsk_1", "x")).rejects.toThrow(
      /not served by the Specline daemon/,
    );
    expect(called).toBe(false);
  });
});
