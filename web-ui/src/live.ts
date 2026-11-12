// Live updates from the speaker (UI_REQ_004, DEC-046): Server-Sent Events at
// /api/events, read with fetch so the token goes in the Authorization header.

export type Topic = "source" | "volume" | "timer" | "bluetooth" | "ota";
export const TOPICS: Topic[] = ["source", "volume", "timer", "bluetooth", "ota"];

export type OnEvent = (topic: Topic, data: unknown) => void;
export type OnConnected = (connected: boolean) => void;

/** The latest pushed value of each topic, when it arrived, and whether the
 *  stream is up. */
export interface Live {
  values: Partial<Record<Topic, unknown>>;
  at: Partial<Record<Topic, number>>;
  connected: boolean;
}

export const NO_LIVE: Live = { values: {}, at: {}, connected: false };

/** Splits server-sent events off the front of `buffer`. Returns the complete
 *  events and what is left for the next chunk. Comments (keep-alives) and
 *  unknown topics are skipped. */
export function parseSse(buffer: string): { events: [Topic, unknown][]; rest: string } {
  const events: [Topic, unknown][] = [];
  const text = buffer.replace(/\r\n/g, "\n");
  const frames = text.split("\n\n");
  const rest = frames.pop() ?? "";
  for (const frame of frames) {
    let topic = "";
    const data: string[] = [];
    for (const line of frame.split("\n")) {
      if (line.startsWith("event:")) topic = line.slice(6).trim();
      else if (line.startsWith("data:")) data.push(line.slice(5).replace(/^ /, ""));
    }
    if (!(TOPICS as string[]).includes(topic) || data.length === 0) continue;
    try {
      events.push([topic as Topic, JSON.parse(data.join("\n"))]);
    } catch {
      // A broken event is skipped; the next one replaces it anyway.
    }
  }
  return { events, rest };
}

/** Delays before reconnecting: 1, 2, 4, 8, then 15 s. */
export function backoff(attempt: number): number {
  return Math.min(1000 * 2 ** attempt, 15_000);
}

/** Subscribes to `url` and keeps the subscription up until the returned
 *  function is called. */
export function subscribe(
  url: string,
  token: () => string | null,
  onEvent: OnEvent,
  onConnected: OnConnected,
  fetchImpl: typeof fetch = (...args) => fetch(...args),
): () => void {
  let stopped = false;
  let attempt = 0;
  let controller: AbortController | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;

  const run = async () => {
    controller = new AbortController();
    try {
      const headers: Record<string, string> = { Accept: "text/event-stream" };
      const t = token();
      if (t) headers.Authorization = `Bearer ${t}`;
      const response = await fetchImpl(url, { headers, signal: controller.signal, cache: "no-store" });
      if (!response.ok || !response.body) throw new Error(`events: ${response.status}`);
      attempt = 0;
      onConnected(true);
      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });
        const parsed = parseSse(buffer);
        buffer = parsed.rest;
        for (const [topic, data] of parsed.events) onEvent(topic, data);
      }
    } catch {
      // Dropped or refused: try again below.
    }
    if (stopped) return;
    onConnected(false);
    timer = setTimeout(run, backoff(attempt++));
  };
  run();

  return () => {
    stopped = true;
    if (timer) clearTimeout(timer);
    controller?.abort();
  };
}

/** Whether `next` says the same as `prev`, `elapsedMs` later: countdowns
 *  (`remaining_s`) that ticked as expected count as the same. The server
 *  compares the same way. */
export function same(prev: unknown, next: unknown, elapsedMs: number): boolean {
  if (Array.isArray(prev) && Array.isArray(next)) {
    return prev.length === next.length && prev.every((p, i) => same(p, next[i], elapsedMs));
  }
  if (prev && next && typeof prev === "object" && typeof next === "object") {
    const a = prev as Record<string, unknown>;
    const b = next as Record<string, unknown>;
    const keys = Object.keys(a);
    if (keys.length !== Object.keys(b).length) return false;
    return keys.every((k) => {
      if (!(k in b)) return false;
      if (k === "remaining_s" && typeof a[k] === "number" && typeof b[k] === "number") {
        const expected = Math.max(0, (a[k] as number) - elapsedMs / 1000);
        return Math.abs((b[k] as number) - expected) <= 3;
      }
      return same(a[k], b[k], elapsedMs);
    });
  }
  return prev === next;
}
