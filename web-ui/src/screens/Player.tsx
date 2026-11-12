import { useCallback } from "preact/hooks";
import { useApp } from "../context";
import { formatDuration, useCountdown, usePoll, useTopic } from "../hooks";
import { VolumeSlider } from "../components/VolumeSlider";

const SOURCE_KEYS: Record<string, string> = {
  airplay: "source.airplay",
  spotify: "source.spotify",
  bluetooth: "source.bluetooth",
  radio: "source.radio",
};

/** What plays, its controls, the volume, the stations and the sleep timer
 *  (journey 2, UI_REQ_002). */
export function Player() {
  const { client, t, notify } = useApp();
  // Pushed live (DEC-046); polled only while live updates are down.
  const source = useTopic("source", useCallback(() => client.source(), [client]), 5000);
  const volume = useTopic("volume", useCallback(() => client.volume(), [client]), 10000);
  const stations = usePoll(useCallback(() => client.stations(), [client]));
  const timer = useTopic("timer", useCallback(() => client.timer(), [client]), 15000);
  const timerLeft = useCountdown(timer.data?.running ? timer.data.remaining_s : 0, timer.at);

  const active = source.data?.active ?? null;
  const radio = source.data?.sources.radio;
  const station = stations.data?.find((s) => s.id === radio?.station);
  const act = async (f: () => Promise<unknown>) => {
    try {
      await f();
    } catch {
      notify(t("error.generic"));
    }
    await source.reload();
  };

  return (
    <section>
      <h1>{t("tab.player")}</h1>
      <div class="card now-playing" aria-live="polite">
        {active ? (
          <>
            <p class="source">{t(SOURCE_KEYS[active] ?? "source.radio")}{station ? ` · ${station.name}` : ""}</p>
            {radio?.title && <p class="title">{radio.title}</p>}
            {active === "radio" && (
              <button class="big" onClick={() => act(() => client.stopRadio())} aria-label={t("player.stop")}>⏹</button>
            )}
          </>
        ) : (
          <p class="muted">{t("player.nothing")}</p>
        )}
      </div>
      {volume.data && (
        <VolumeSlider
          volume={volume.data.volume}
          limit={volume.data.limit}
          onChange={async (v) => {
            try {
              await client.setVolume(v);
            } catch {
              notify(t("error.generic"));
            }
            await volume.reload();
          }}
        />
      )}
      <h2>{t("tab.radio")}</h2>
      <div class="tiles">
        {(stations.data ?? []).map((s) => (
          <button key={s.id} class={radio?.station === s.id ? "tile active" : "tile"} onClick={() => act(() => client.playStation(s.id))}>
            {s.name}
          </button>
        ))}
      </div>
      <h2>{t("timer.title")}</h2>
      {timer.data?.running ? (
        <p>
          {t("timer.left", { time: formatDuration(timerLeft) })}{" "}
          <button class="link" onClick={async () => { await client.cancelTimer(); await timer.reload(); }}>{t("timer.cancel")}</button>
        </p>
      ) : (
        <div class="chips">
          {[15, 30, 60].map((m) => (
            <button key={m} class="chip" onClick={async () => { await client.startTimer(m); await timer.reload(); notify(t("timer.started", { n: m })); }}>
              {t("timer.minutes", { n: m })}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
