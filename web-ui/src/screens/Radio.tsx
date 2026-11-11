import { useCallback, useState } from "preact/hooks";
import type { Episode } from "../api/types";
import { ApiError } from "../api/types";
import { useApp } from "../context";
import { formatDuration, usePoll } from "../hooks";

/** Stations and podcasts (RAD_REQ_001, POD_REQ_001/002). Parents can add and
 *  remove them; everyone can play. */
export function Radio() {
  const { client, t, admin, notify } = useApp();
  const stations = usePoll(useCallback(() => client.stations(), [client]));
  const feeds = usePoll(useCallback(() => client.feeds(), [client]));
  const [open, setOpen] = useState<string | null>(null);
  const [episodes, setEpisodes] = useState<Episode[] | null>(null);
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [feedUrl, setFeedUrl] = useState("");

  const fail = (e: unknown) => notify(e instanceof ApiError ? e.message : t("error.generic"));
  const play = async (f: () => Promise<void>) => {
    try {
      await f();
      notify(t("radio.playing"));
    } catch (e) {
      fail(e);
    }
  };
  const toggleFeed = async (id: string) => {
    if (open === id) return setOpen(null);
    setOpen(id);
    setEpisodes(null);
    try {
      setEpisodes(await client.episodes(id));
    } catch (e) {
      fail(e);
    }
  };

  return (
    <section>
      <h1>{t("tab.radio")}</h1>
      <ul class="list">
        {(stations.data ?? []).map((s) => (
          <li key={s.id}>
            <button class="row-main" onClick={() => play(() => client.playStation(s.id))}>▶ {s.name}</button>
            {admin && (
              <button class="link danger" aria-label={t("remove")} onClick={async () => { await client.removeStation(s.id).catch(fail); await stations.reload(); }}>
                {t("remove")}
              </button>
            )}
          </li>
        ))}
      </ul>
      {admin && (
        <form
          class="card"
          onSubmit={async (e) => {
            e.preventDefault();
            try {
              await client.addStation(name.trim(), url.trim());
              setName("");
              setUrl("");
              await stations.reload();
            } catch (err) {
              fail(err);
            }
          }}
        >
          <h2>{t("radio.add")}</h2>
          <label for="st-name">{t("radio.name")}</label>
          <input id="st-name" value={name} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
          <label for="st-url">{t("radio.url")}</label>
          <input id="st-url" value={url} inputMode="url" placeholder="https://" onInput={(e) => setUrl((e.target as HTMLInputElement).value)} />
          <button type="submit" disabled={!name.trim() || !url.trim()}>{t("add")}</button>
        </form>
      )}

      <h2>{t("podcasts.title")}</h2>
      {(feeds.data ?? []).length === 0 && <p class="muted">{t("podcasts.none")}</p>}
      <ul class="list">
        {(feeds.data ?? []).map((f) => (
          <li key={f.id} class="stack">
            <div class="row">
              <button class="row-main" aria-expanded={open === f.id} onClick={() => toggleFeed(f.id)}>
                {open === f.id ? "▾" : "▸"} {f.title}
              </button>
              {admin && (
                <button class="link danger" onClick={async () => { await client.removeFeed(f.id).catch(fail); await feeds.reload(); }}>{t("remove")}</button>
              )}
            </div>
            {open === f.id && (
              <ul class="episodes">
                {episodes === null && <li class="muted">{t("loading")}</li>}
                {episodes?.map((ep) => (
                  <li key={ep.id}>
                    <button class="row-main" onClick={() => play(() => client.playEpisode(f.id, ep.id))}>
                      ▶ {ep.title}
                      <span class="muted small">
                        {ep.duration_s ? ` · ${formatDuration(ep.duration_s)}` : ""}
                        {ep.position_s ? ` · ${t("podcasts.resume", { time: formatDuration(ep.position_s) })}` : ""}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      {admin && (
        <form
          class="card"
          onSubmit={async (e) => {
            e.preventDefault();
            try {
              await client.addFeed(feedUrl.trim());
              setFeedUrl("");
              await feeds.reload();
            } catch (err) {
              fail(err);
            }
          }}
        >
          <h2>{t("podcasts.add")}</h2>
          <label for="feed-url">{t("podcasts.url")}</label>
          <input id="feed-url" value={feedUrl} inputMode="url" placeholder="https://" onInput={(e) => setFeedUrl((e.target as HTMLInputElement).value)} />
          <button type="submit" disabled={!feedUrl.trim()}>{t("add")}</button>
        </form>
      )}
    </section>
  );
}
