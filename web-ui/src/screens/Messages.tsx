import { useCallback, useState } from "preact/hooks";
import { useApp } from "../context";
import { formatDuration, usePoll } from "../hooks";
import { RecorderPanel } from "../components/RecorderPanel";

/** Voice messages and text announcements (journeys 3 and 4). */
export function Messages() {
  const { client, t, lang, notify } = useApp();
  const messages = usePoll(useCallback(() => client.messages(), [client]));
  const [text, setText] = useState("");
  const [speechLang, setSpeechLang] = useState(lang === "en" ? "en-GB" : "da-DK");
  const [editing, setEditing] = useState<string | null>(null);
  const [newName, setNewName] = useState("");
  const when = (secs: number) =>
    new Date(secs * 1000).toLocaleString(lang === "en" ? "en-GB" : "da-DK", { weekday: "short", day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });

  return (
    <section>
      <h1>{t("tab.messages")}</h1>
      <RecorderPanel onSent={messages.reload} />

      <form
        class="card"
        onSubmit={async (e) => {
          e.preventDefault();
          try {
            await client.announce(text.trim(), speechLang);
            setText("");
            notify(t("messages.announced"));
          } catch {
            notify(t("error.generic"));
          }
        }}
      >
        <h2>{t("messages.text_title")}</h2>
        <label for="announce">{t("messages.text")}</label>
        <input id="announce" value={text} maxLength={300} onInput={(e) => setText((e.target as HTMLInputElement).value)} />
        <fieldset class="inline">
          <legend class="sr-only">{t("messages.language")}</legend>
          {[["da-DK", t("lang.da")], ["en-GB", t("lang.en")]].map(([value, label]) => (
            <label key={value}>
              <input type="radio" name="speech-lang" value={value} checked={speechLang === value} onChange={() => setSpeechLang(value)} /> {label}
            </label>
          ))}
        </fieldset>
        <button type="submit" disabled={!text.trim()}>{t("messages.speak")}</button>
      </form>

      <h2>{t("messages.earlier")}</h2>
      {(messages.data ?? []).length === 0 && <p class="muted">{t("messages.none")}</p>}
      <ul class="list">
        {(messages.data ?? []).map((m) => (
          <li key={m.id} class="stack">
            {editing === m.id ? (
              <form
                class="row"
                onSubmit={async (e) => {
                  e.preventDefault();
                  await client.renameMessage(m.id, newName).catch(() => notify(t("error.generic")));
                  setEditing(null);
                  await messages.reload();
                }}
              >
                <input aria-label={t("messages.name")} value={newName} onInput={(e) => setNewName((e.target as HTMLInputElement).value)} />
                <button type="submit">{t("save")}</button>
              </form>
            ) : (
              <div class="row">
                <button class="row-main" onClick={async () => { await client.playMessage(m.id).catch(() => notify(t("error.generic"))); notify(t("messages.playing")); }}>
                  ▶ {m.name ?? t("messages.unnamed")}
                  <span class="muted small"> · {formatDuration(m.duration_s)} · {when(m.recorded)}</span>
                </button>
                <button class="link" onClick={() => { setEditing(m.id); setNewName(m.name ?? ""); }}>{t("messages.rename")}</button>
                <button class="link danger" onClick={async () => { await client.deleteMessage(m.id); await messages.reload(); }}>{t("remove")}</button>
              </div>
            )}
          </li>
        ))}
      </ul>
      <p class="muted small">{t("messages.expiry")}</p>
    </section>
  );
}
