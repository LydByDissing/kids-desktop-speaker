import { createContext } from "preact";
import { useContext } from "preact/hooks";
import type { ApiClient } from "./api/client";
import type { Lang } from "./i18n";
import type { Recorder } from "./wav";

export interface AppServices {
  client: ApiClient;
  lang: Lang;
  setLang: (lang: Lang) => void;
  t: (key: string, vars?: Record<string, string | number>) => string;
  /** A parent's (admin) token; a child's device sees less (DEC-045). */
  admin: boolean;
  recorder: Recorder;
  /** Shows a short message at the bottom. */
  notify: (text: string) => void;
  signOut: () => void;
  /** The address other devices use, e.g. https://kidsspeaker-2df5.local */
  origin: string;
}

export const AppContext = createContext<AppServices | null>(null);

export function useApp(): AppServices {
  const app = useContext(AppContext);
  if (!app) throw new Error("useApp outside AppContext");
  return app;
}
