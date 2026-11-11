import { useState } from "preact/hooks";
import { ApiError } from "../api/types";
import { useApp } from "../context";

/** First-time setup and recovery: the speaker reads a 6-digit code aloud;
 *  typing it gives this device an admin token (journey 1, AUTH_REQ_003). */
export function Setup({ onDone, recovering = false }: { onDone: (token: string) => void; recovering?: boolean }) {
  const { client, t } = useApp();
  const [code, setCode] = useState("");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [asked, setAsked] = useState(false);

  const askAgain = async () => {
    setError(null);
    try {
      await client.setupStart();
      setAsked(true);
    } catch (e) {
      setError(e instanceof ApiError && e.status === 429 ? t("setup.too_soon", { s: Number(e.body.retry_after_s ?? 60) }) : t("error.generic"));
    }
  };

  const submit = async (e: Event) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const reply = await client.setupComplete(code, name.trim() || t("setup.default_name"));
      onDone(reply.token);
    } catch (err) {
      if (err instanceof ApiError && err.status === 403) setError(t("setup.wrong", { n: Number(err.body.attempts_left ?? 0) }));
      else if (err instanceof ApiError && err.status === 409) setError(t("setup.no_code"));
      else setError(t("error.generic"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <form class="card narrow" onSubmit={submit}>
      <h1>{recovering ? t("setup.recover_title") : t("setup.title")}</h1>
      <p>{recovering || asked ? t("setup.listen_now") : t("setup.listen")}</p>
      <label for="code">{t("setup.code")}</label>
      <input
        id="code"
        class="code"
        inputMode="numeric"
        autoComplete="one-time-code"
        maxLength={6}
        value={code}
        onInput={(e) => setCode((e.target as HTMLInputElement).value.replace(/\D/g, ""))}
      />
      <label for="name">{t("setup.name")}</label>
      <input id="name" value={name} placeholder={t("setup.name_hint")} onInput={(e) => setName((e.target as HTMLInputElement).value)} />
      {error && <p class="error" role="alert">{error}</p>}
      <button type="submit" class="primary" disabled={busy || code.length !== 6}>{t("setup.continue")}</button>
      <button type="button" class="link" onClick={askAgain}>{t("setup.again")}</button>
    </form>
  );
}
