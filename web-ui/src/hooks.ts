import { useCallback, useEffect, useState } from "preact/hooks";
import { useApp } from "./context";
import type { Topic } from "./live";

/** Loads `load()` now and every `every` ms; `reload()` refreshes at once.
 *  `at` is when `data` was loaded (ms). */
export function usePoll<T>(load: () => Promise<T>, every = 0): { data: T | null; at: number; error: unknown; reload: () => Promise<void> } {
  const [state, setState] = useState<{ data: T | null; at: number }>({ data: null, at: 0 });
  const [error, setError] = useState<unknown>(null);
  const reload = useCallback(async () => {
    try {
      const data = await load();
      setState({ data, at: Date.now() });
      setError(null);
    } catch (e) {
      setError(e);
    }
  }, [load]);
  useEffect(() => {
    reload();
    if (!every) return;
    const id = setInterval(reload, every);
    return () => clearInterval(id);
  }, [reload, every]);
  return { ...state, error, reload };
}

/** A topic the speaker pushes (DEC-046): loaded once, then kept up to date by
 *  its events. Without live updates it polls every `every` ms instead. The
 *  newer of the loaded and the pushed value wins. */
export function useTopic<T>(topic: Topic, load: () => Promise<T>, every: number) {
  const { live } = useApp();
  const poll = usePoll(load, live.connected ? 0 : every);
  const pushedAt = live.at[topic] ?? 0;
  if (pushedAt > poll.at && live.values[topic] !== undefined) {
    return { ...poll, data: live.values[topic] as T, at: pushedAt };
  }
  return poll;
}

/** Seconds left of a countdown that had `seconds` left at `at` (ms), ticking
 *  every second. */
export function useCountdown(seconds: number | null | undefined, at: number): number {
  const [now, setNow] = useState(Date.now());
  useEffect(() => {
    if (!seconds) return;
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, [seconds, at]);
  if (!seconds) return 0;
  return Math.max(0, Math.round(seconds - (Math.max(now, at) - at) / 1000));
}

export function formatDuration(seconds: number): string {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const rest = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${rest}` : `${m}:${rest}`;
}
