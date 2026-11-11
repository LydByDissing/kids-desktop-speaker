import { useState } from "preact/hooks";
import { HttpClient } from "../api/client";
import { ApiError } from "../api/types";
import { useApp } from "../context";
import { tokenFromHash } from "../share";
import { Setup } from "./Setup";

/** A browser without a token: paste one (or open a shared link), or get back
 *  in with a spoken code (journey 8). */
export function SignIn({ onDone }: { onDone: (token: string, admin: boolean) => void }) {
  const { client, t } = useApp();
  const [token, setToken] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [recovering, setRecovering] = useState(false);

  if (recovering) {
    return <Setup recovering onDone={(tok) => onDone(tok, true)} />;
  }

  const submit = async (e: Event) => {
    e.preventDefault();
    setError(null);
    const value = tokenFromHash(new URL(token, "https://x").hash) ?? token.trim();
    // Check the token before keeping it. A real browser asks the speaker; in
    // stories the injected client answers.
    const check = client instanceof HttpClient ? new HttpClient(() => value) : client;
    try {
      const admin = await check.isAdmin();
      onDone(value, admin);
    } catch (err) {
      setError(err instanceof ApiError && err.status === 401 ? t("signin.invalid") : t("error.generic"));
    }
  };

  return (
    <form class="card narrow" onSubmit={submit}>
      <h1>{t("signin.title")}</h1>
      <p>{t("signin.explain")}</p>
      <label for="token">{t("signin.token")}</label>
      <input id="token" value={token} autoComplete="off" onInput={(e) => setToken((e.target as HTMLInputElement).value)} />
      {error && <p class="error" role="alert">{error}</p>}
      <button type="submit" class="primary" disabled={!token.trim()}>{t("signin.go")}</button>
      <button type="button" class="link" onClick={async () => { await client.setupStart().catch(() => {}); setRecovering(true); }}>
        {t("signin.forgot")}
      </button>
    </form>
  );
}
