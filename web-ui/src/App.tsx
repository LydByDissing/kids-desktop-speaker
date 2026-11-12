import { useEffect, useMemo, useState } from "preact/hooks";
import type { ApiClient } from "./api/client";
import { storeToken, storedToken } from "./api/client";
import { ApiError } from "./api/types";
import { AppContext, AppServices } from "./context";
import { Lang, savedLang, saveLang, translator } from "./i18n";
import { tokenFromHash } from "./share";
import { browserRecorder, Recorder } from "./wav";
import { Setup } from "./screens/Setup";
import { SignIn } from "./screens/SignIn";
import { Player } from "./screens/Player";
import { Radio } from "./screens/Radio";
import { Messages } from "./screens/Messages";
import { Settings } from "./screens/Settings";
import { TabBar, Tab } from "./components/TabBar";
import { RebootBanner } from "./components/RebootBanner";
import { Live, NO_LIVE } from "./live";

export interface AppProps {
  client: ApiClient;
  recorder?: Recorder;
  /** For stories: start on this tab. */
  initialTab?: Tab;
  /** For stories: treat the device as signed in (or not). */
  signedIn?: boolean;
  origin?: string;
}

type Phase = "loading" | "setup" | "signin" | "ready" | "offline";

export function App({ client, recorder = browserRecorder, initialTab = "player", signedIn, origin }: AppProps) {
  const [lang, setLangState] = useState<Lang>(savedLang());
  const [phase, setPhase] = useState<Phase>("loading");
  const [admin, setAdmin] = useState(false);
  const [tab, setTab] = useState<Tab>(initialTab);
  const [toast, setToast] = useState<string | null>(null);
  const [live, setLive] = useState<Live>(NO_LIVE);
  const t = useMemo(() => translator(lang), [lang]);

  const start = async () => {
    // A shared access link carries the token in the fragment (DEC-045).
    const shared = tokenFromHash(location.hash);
    if (shared) {
      storeToken(shared);
      history.replaceState(null, "", location.pathname);
    }
    try {
      const health = await client.health();
      if (!health.set_up) return setPhase("setup");
      if (signedIn === false || (signedIn === undefined && !storedToken())) return setPhase("signin");
      setAdmin(await client.isAdmin());
      setPhase("ready");
    } catch (e) {
      if (e instanceof ApiError && e.status === 401) {
        storeToken(null);
        setPhase("signin");
      } else {
        setPhase("offline");
      }
    }
  };

  useEffect(() => {
    start();
  }, []);

  // Live updates while signed in (DEC-046).
  useEffect(() => {
    if (phase !== "ready") return;
    const close = client.events(
      (topic, data) =>
        setLive((l) => ({ ...l, values: { ...l.values, [topic]: data }, at: { ...l.at, [topic]: Date.now() } })),
      (connected) => setLive((l) => ({ ...l, connected })),
    );
    return () => {
      close();
      setLive(NO_LIVE);
    };
  }, [phase, client]);

  const services: AppServices = {
    client,
    lang,
    setLang: (l) => {
      saveLang(l);
      setLangState(l);
    },
    t,
    admin,
    recorder,
    notify: (text) => {
      setToast(text);
      setTimeout(() => setToast(null), 3500);
    },
    signOut: () => {
      storeToken(null);
      setPhase("signin");
    },
    origin: origin ?? location.origin,
    live,
  };
  const signedInWith = (token: string, isAdmin: boolean) => {
    storeToken(token);
    setAdmin(isAdmin);
    setPhase("ready");
  };
  // A child's device sees only the Player and Radio.
  const tabs: Tab[] = admin ? ["player", "radio", "messages", "settings"] : ["player", "radio"];
  const shown = tabs.includes(tab) ? tab : "player";

  return (
    <AppContext.Provider value={services}>
      <div class="app">
        {phase === "loading" && <p class="center muted">{t("loading")}</p>}
        {phase === "offline" && (
          <div class="center">
            <p>{t("offline")}</p>
            <button onClick={() => { setPhase("loading"); start(); }}>{t("retry")}</button>
          </div>
        )}
        {phase === "setup" && <Setup onDone={(token) => signedInWith(token, true)} />}
        {phase === "signin" && <SignIn onDone={signedInWith} />}
        {phase === "ready" && (
          <>
            <RebootBanner />
            <main class="screen">
              {shown === "player" && <Player />}
              {shown === "radio" && <Radio />}
              {shown === "messages" && <Messages />}
              {shown === "settings" && <Settings />}
            </main>
            <TabBar tabs={tabs} current={shown} onSelect={setTab} />
          </>
        )}
        {toast && <div class="toast" role="status">{toast}</div>}
      </div>
    </AppContext.Provider>
  );
}
