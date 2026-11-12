import { useCallback, useEffect, useState } from "preact/hooks";

/** Loads `load()` now and every `every` ms; `reload()` refreshes at once. */
export function usePoll<T>(load: () => Promise<T>, every = 0): { data: T | null; error: unknown; reload: () => Promise<void> } {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const reload = useCallback(async () => {
    try {
      setData(await load());
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
  return { data, error, reload };
}

export function formatDuration(seconds: number): string {
  const s = Math.round(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const rest = String(s % 60).padStart(2, "0");
  return h ? `${h}:${String(m).padStart(2, "0")}:${rest}` : `${m}:${rest}`;
}
