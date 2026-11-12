import { backoff, parseSse, same, subscribe } from "./live";

describe("server-sent events (DEC-046)", () => {
  it("parses complete events and keeps a partial one for later", () => {
    const { events, rest } = parseSse('event: volume\ndata: {"volume":60}\n\n: keep-alive\n\nevent: source\ndata: {"act');
    expect(events).toEqual([["volume", { volume: 60 }]]);
    expect(rest).toBe('event: source\ndata: {"act');
    expect(parseSse(rest + 'ive":null}\n\n').events).toEqual([["source", { active: null }]]);
  });

  it("copes with CRLF, data split over lines, unknown topics and broken JSON", () => {
    const { events } = parseSse(
      'event: timer\r\ndata: {"running":\r\ndata: false}\r\n\r\nevent: weather\ndata: {}\n\nevent: ota\ndata: {nope\n\n',
    );
    expect(events).toEqual([["timer", { running: false }]]);
  });

  it("backs off 1, 2, 4, 8 and then 15 s", () => {
    expect([0, 1, 2, 3, 4, 9].map(backoff)).toEqual([1000, 2000, 4000, 8000, 15000, 15000]);
  });

  it("treats a countdown that ticked as expected as unchanged", () => {
    const t = (s: number) => ({ running: true, minutes: 30, remaining_s: s });
    expect(same(t(1800), t(1790), 10_000)).toBe(true);
    expect(same(t(1790), t(1800), 10_000)).toBe(false);
    expect(same({ volume: 60 }, { volume: 61 }, 0)).toBe(false);
    expect(same({ a: [1, { b: 2 }] }, { a: [1, { b: 2 }] }, 0)).toBe(true);
  });
});

/** A fetch whose response streams `chunks`, then ends. */
function streamingFetch(chunks: string[]) {
  const calls: RequestInit[] = [];
  const impl = (async (_url: string, init: RequestInit) => {
    calls.push(init);
    const encoder = new TextEncoder();
    const body = new ReadableStream<Uint8Array>({
      start(controller) {
        for (const c of chunks) controller.enqueue(encoder.encode(c));
        controller.close();
      },
    });
    return new Response(body, { status: 200, headers: { "content-type": "text/event-stream" } });
  }) as unknown as typeof fetch;
  return { impl, calls };
}

describe("subscribe", () => {
  afterEach(() => vi.useRealTimers());

  it("sends the token in a header, delivers events across chunks and reconnects when the stream ends", async () => {
    vi.useFakeTimers();
    const { impl, calls } = streamingFetch(['event: volume\ndata: {"vol', 'ume":40}\n\n']);
    const events: unknown[] = [];
    const connected: boolean[] = [];
    const close = subscribe("/api/events", () => "kds_abc", (t, d) => events.push([t, d]), (c) => connected.push(c), impl);
    await vi.advanceTimersByTimeAsync(10);
    expect((calls[0].headers as Record<string, string>).Authorization).toBe("Bearer kds_abc");
    expect(events).toEqual([["volume", { volume: 40 }]]);
    expect(connected).toEqual([true, false]);
    await vi.advanceTimersByTimeAsync(1000);
    expect(calls.length).toBe(2);
    close();
    await vi.advanceTimersByTimeAsync(30_000);
    expect(calls.length).toBe(2);
  });

  it("retries a refused stream with back-off", async () => {
    vi.useFakeTimers();
    let n = 0;
    const impl = (async () => {
      n += 1;
      return new Response("{}", { status: 503 });
    }) as unknown as typeof fetch;
    const close = subscribe("/api/events", () => null, () => {}, () => {}, impl);
    await vi.advanceTimersByTimeAsync(10);
    expect(n).toBe(1);
    await vi.advanceTimersByTimeAsync(1000);
    expect(n).toBe(2);
    await vi.advanceTimersByTimeAsync(1000);
    expect(n).toBe(2);
    await vi.advanceTimersByTimeAsync(1000);
    expect(n).toBe(3);
    close();
  });
});
