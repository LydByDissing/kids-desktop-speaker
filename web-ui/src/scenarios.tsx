import { App, AppProps } from "./App";
import type { ApiClient } from "./api/client";
import { MockClient, MockOptions } from "./api/mock";
import { fakeRecorder } from "./wav";

// The states of the app worth reviewing, against a mocked speaker (DEC-045).
// Storybook shows one story per scenario and the demo page lists them all.
// Every scenario is clickable: the mock keeps state, so playing a station
// shows it as playing and sending a message lists it. The setup code is 471293.

export type ScenarioArgs = MockOptions & Pick<AppProps, "initialTab" | "signedIn"> & { english?: boolean };

export interface Scenario {
  id: string;
  name: string;
  args: ScenarioArgs;
  /** A client other than the plain mock, such as one that never answers. */
  client?: () => ApiClient;
}

class OfflineClient extends MockClient {
  health = () => Promise.reject(new Error("offline"));
}

export const SCENARIOS: Scenario[] = [
  { id: "setup", name: "New speaker: setup with the spoken code", args: { setUp: false, code: "471293" } },
  { id: "signin", name: "Sign in on a new browser", args: { signedIn: false } },
  { id: "idle", name: "Player: nothing playing", args: {} },
  { id: "playing", name: "Player: DR P3 playing", args: { playing: "dr-p3", programme: "Go' Morgen P3", volume: 60 } },
  { id: "limit", name: "Player: volume at a low limit (40 %)", args: { playing: "dr-p1", programme: "P1 Morgen", volume: 40, limit: 40 } },
  { id: "radio", name: "Radio: stations and podcasts", args: { initialTab: "radio" } },
  { id: "messages", name: "Messages: record, read out, earlier messages", args: { initialTab: "messages" } },
  { id: "no-messages", name: "Messages: none yet", args: { initialTab: "messages", messages: [] } },
  { id: "settings", name: "Settings: Bluetooth, access, limit, alarms, software", args: { initialTab: "settings" } },
  { id: "update", name: "Settings: an update is ready for tonight", args: { initialTab: "settings", ota: { state: "done", detail: null, version: "v0.4.0" } } },
  { id: "child", name: "A child's device: Player and Radio only", args: { admin: false, playing: "dr-p3", programme: "Go' Morgen P3" } },
  { id: "english", name: "English", args: { english: true, playing: "dr-p2", programme: "Klassisk" } },
  { id: "slow", name: "A slow speaker (1.5 s per answer)", args: { delay: 1500 } },
  { id: "offline", name: "The speaker doesn't answer", args: {}, client: () => new OfflineClient() },
];

/** The app in a scenario, with a fresh mock each time it is rendered. */
export function renderScenario(s: Pick<Scenario, "args" | "client">, key = JSON.stringify(s.args)) {
  try {
    localStorage.setItem("kds-lang", s.args.english ? "en" : "da");
  } catch {
    /* storage unavailable */
  }
  const client = s.client ? s.client() : new MockClient(s.args);
  return (
    <App
      key={key}
      client={client}
      recorder={fakeRecorder}
      initialTab={s.args.initialTab}
      signedIn={s.client ? undefined : (s.args.signedIn ?? true)}
      origin="https://kidsspeaker-2df5.local"
    />
  );
}
