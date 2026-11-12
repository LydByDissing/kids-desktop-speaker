import { useCallback, useEffect, useState } from "preact/hooks";
import type { AlarmInput, NewToken } from "../api/types";
import { ApiError } from "../api/types";
import { useApp } from "../context";
import { formatDuration, usePoll } from "../hooks";
import { Lang } from "../i18n";
import { accessLink } from "../share";
import { QrCode } from "../components/QrCode";

/** Settings for parents (journeys 5-7): Bluetooth, access, the volume limit,
 *  alarms, software and language. */
export function Settings() {
  const { t, lang, setLang, signOut } = useApp();
  return (
    <section>
      <h1>{t("tab.settings")}</h1>
      <BluetoothSection />
      <AccessSection />
      <LimitSection />
      <AlarmsSection />
      <SoftwareSection />
      <div class="card">
        <h2>{t("settings.language")}</h2>
        <select aria-label={t("settings.language")} value={lang} onChange={(e) => setLang((e.target as HTMLSelectElement).value as Lang)}>
          <option value="da">{t("lang.da")}</option>
          <option value="en">{t("lang.en")}</option>
        </select>
      </div>
      <button class="link" onClick={signOut}>{t("settings.sign_out")}</button>
    </section>
  );
}

function BluetoothSection() {
  const { client, t, notify } = useApp();
  const status = usePoll(useCallback(() => client.bluetooth(), [client]), 5000);
  const open = status.data?.pairing.open;
  return (
    <div class="card">
      <h2>{t("bt.title")}</h2>
      {open ? (
        <p aria-live="polite">{t("bt.open", { time: formatDuration(status.data?.pairing.remaining_s ?? 0) })}</p>
      ) : (
        <button onClick={async () => { await client.pair().catch(() => notify(t("error.generic"))); await status.reload(); }}>{t("bt.pair")}</button>
      )}
    </div>
  );
}

function AccessSection() {
  const { client, t, lang, origin, notify } = useApp();
  const tokens = usePoll(useCallback(() => client.tokens(), [client]));
  const [name, setName] = useState("");
  const [admin, setAdmin] = useState(false);
  const [created, setCreated] = useState<NewToken | null>(null);
  const link = created ? accessLink(origin, created.token) : "";
  return (
    <div class="card">
      <h2>{t("access.title")}</h2>
      <ul class="list">
        {(tokens.data ?? []).map((tok) => (
          <li key={tok.id}>
            <span class="row-main">
              {tok.name} {tok.admin && <span class="badge">{t("access.admin")}</span>}
              <span class="muted small"> · {tok.last_used ? t("access.used", { when: new Date(tok.last_used * 1000).toLocaleDateString(lang === "en" ? "en-GB" : "da-DK") }) : t("access.never")}</span>
            </span>
            <button
              class="link danger"
              onClick={async () => {
                try {
                  await client.revokeToken(tok.id);
                } catch (e) {
                  notify(e instanceof ApiError && e.status === 409 ? t("access.last_admin") : t("error.generic"));
                }
                await tokens.reload();
              }}
            >
              {t("remove")}
            </button>
          </li>
        ))}
      </ul>
      {created ? (
        <div class="share">
          <p>{t("access.once")}</p>
          <QrCode text={link} label={t("access.qr")} />
          <code class="token">{created.token}</code>
          <div class="row">
            <button onClick={async () => { await navigator.clipboard?.writeText(link).catch(() => {}); notify(t("access.copied")); }}>{t("access.copy")}</button>
            <button onClick={() => setCreated(null)}>{t("done")}</button>
          </div>
        </div>
      ) : (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            try {
              setCreated(await client.createToken(name.trim(), admin));
              setName("");
              setAdmin(false);
              await tokens.reload();
            } catch {
              notify(t("error.generic"));
            }
          }}
        >
          <label for="tok-name">{t("access.name")}</label>
          <input id="tok-name" value={name} placeholder={t("access.name_hint")} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
          <label class="check">
            <input type="checkbox" checked={admin} onChange={(e) => setAdmin((e.target as HTMLInputElement).checked)} /> {t("access.may_change")}
          </label>
          <button type="submit" disabled={!name.trim()}>{t("access.new")}</button>
        </form>
      )}
    </div>
  );
}

function LimitSection() {
  const { client, t, notify } = useApp();
  const volume = usePoll(useCallback(() => client.volume(), [client]));
  const [limit, setLimit] = useState<number | null>(null);
  useEffect(() => {
    if (volume.data) setLimit(volume.data.limit);
  }, [volume.data]);
  if (limit === null) return null;
  return (
    <div class="card">
      <h2>{t("limit.title")}</h2>
      <label for="limit">{t("limit.explain")}</label>
      <input id="limit" type="range" min={10} max={100} step={5} value={limit} onInput={(e) => setLimit(Number((e.target as HTMLInputElement).value))} />
      <p>{limit} %</p>
      <button
        onClick={async () => {
          try {
            await client.setLimit(limit);
            notify(t("limit.saved", { n: limit }));
          } catch {
            notify(t("error.generic"));
          }
        }}
      >
        {t("save")}
      </button>
    </div>
  );
}

const DAYS = [1, 2, 3, 4, 5, 6, 7];

function AlarmsSection() {
  const { client, t, notify } = useApp();
  const alarms = usePoll(useCallback(() => client.alarms(), [client]));
  const stations = usePoll(useCallback(() => client.stations(), [client]));
  const blank = (): AlarmInput => ({ name: null, time: "07:00", days: [1, 2, 3, 4, 5], station: "", volume: 50, enabled: true });
  const [draft, setDraft] = useState<AlarmInput | null>(null);
  const days = (d: number[]) => (d.length === 0 || d.length === 7 ? t("alarm.every_day") : d.map((n) => t(`day.${n}`)).join(" "));
  return (
    <div class="card">
      <h2>{t("alarm.title")}</h2>
      <ul class="list">
        {(alarms.data ?? []).map((a) => (
          <li key={a.id}>
            <label class="row-main check">
              <input
                type="checkbox"
                checked={a.enabled}
                aria-label={t("alarm.on")}
                onChange={async () => { await client.updateAlarm(a.id, { ...a, enabled: !a.enabled }); await alarms.reload(); }}
              />
              <b>{a.time}</b> {days(a.days)} · {stations.data?.find((s) => s.id === a.station)?.name ?? a.station}
            </label>
            <button class="link danger" onClick={async () => { await client.removeAlarm(a.id); await alarms.reload(); }}>{t("remove")}</button>
          </li>
        ))}
      </ul>
      {draft ? (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            try {
              await client.addAlarm({ ...draft, station: draft.station || stations.data?.[0]?.id || "" });
              setDraft(null);
              await alarms.reload();
            } catch (err) {
              notify(err instanceof ApiError ? err.message : t("error.generic"));
            }
          }}
        >
          <label for="al-time">{t("alarm.time")}</label>
          <input id="al-time" type="time" value={draft.time} onInput={(e) => setDraft({ ...draft, time: (e.target as HTMLInputElement).value })} />
          <fieldset class="inline">
            <legend>{t("alarm.days")}</legend>
            {DAYS.map((d) => (
              <label key={d}>
                <input
                  type="checkbox"
                  checked={draft.days.includes(d)}
                  onChange={() => setDraft({ ...draft, days: draft.days.includes(d) ? draft.days.filter((x) => x !== d) : [...draft.days, d].sort() })}
                />{" "}
                {t(`day.${d}`)}
              </label>
            ))}
          </fieldset>
          <label for="al-station">{t("alarm.station")}</label>
          <select id="al-station" value={draft.station} onChange={(e) => setDraft({ ...draft, station: (e.target as HTMLSelectElement).value })}>
            {(stations.data ?? []).map((s) => <option key={s.id} value={s.id}>{s.name}</option>)}
          </select>
          <label for="al-volume">{t("alarm.volume", { n: draft.volume })}</label>
          <input id="al-volume" type="range" min={5} max={100} step={5} value={draft.volume} onInput={(e) => setDraft({ ...draft, volume: Number((e.target as HTMLInputElement).value) })} />
          <div class="row">
            <button type="submit" class="primary">{t("save")}</button>
            <button type="button" onClick={() => setDraft(null)}>{t("cancel")}</button>
          </div>
        </form>
      ) : (
        <button onClick={() => setDraft(blank())}>{t("alarm.new")}</button>
      )}
    </div>
  );
}

function SoftwareSection() {
  const { client, t, notify } = useApp();
  const ota = usePoll(useCallback(() => client.ota(), [client]), 15000);
  return (
    <div class="card">
      <h2>{t("ota.title")}</h2>
      <p>{t("ota.version", { v: ota.data?.version ?? "…" })}</p>
      <p class="muted">{t(`ota.state.${typeof ota.data?.state === "string" ? ota.data.state : ota.data ? "error" : "idle"}`)}</p>
      <div class="row">
        <button onClick={async () => { await client.otaCheck().catch(() => notify(t("error.generic"))); notify(t("ota.checking")); await ota.reload(); }}>{t("ota.check")}</button>
        <button
          class="danger"
          onClick={async () => {
            if (!confirm(t("ota.confirm"))) return;
            await client.otaReboot().catch(() => notify(t("error.generic")));
          }}
        >
          {t("ota.reboot")}
        </button>
      </div>
    </div>
  );
}
