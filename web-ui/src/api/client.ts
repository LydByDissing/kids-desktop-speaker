import {
  Alarm, AlarmInput, ApiError, BluetoothStatus, Episode, Feed, Health, Message, NewToken, OtaStatus,
  Source, Station, Timer, TokenInfo, Volume,
} from "./types";

/** Everything the UI asks of the speaker. HttpClient talks to the real API;
 *  MockClient (mock.ts) stands in for it in Storybook and the tests. */
export interface ApiClient {
  health(): Promise<Health>;
  setupStart(): Promise<void>;
  setupComplete(code: string, name: string): Promise<NewToken>;
  /** Whether the stored token is an admin token (it may list tokens). */
  isAdmin(): Promise<boolean>;

  source(): Promise<Source>;
  volume(): Promise<Volume>;
  setVolume(volume: number): Promise<Volume>;
  setLimit(limit: number): Promise<Volume>;

  stations(): Promise<Station[]>;
  playStation(id: string): Promise<void>;
  stopRadio(): Promise<void>;
  addStation(name: string, url: string): Promise<Station>;
  removeStation(id: string): Promise<void>;

  feeds(): Promise<Feed[]>;
  addFeed(url: string): Promise<Feed>;
  removeFeed(id: string): Promise<void>;
  episodes(feed: string): Promise<Episode[]>;
  playEpisode(feed: string, episode: string): Promise<void>;
  stopPodcast(): Promise<void>;

  messages(): Promise<Message[]>;
  sendMessage(wav: Blob, play: boolean): Promise<Message>;
  renameMessage(id: string, name: string): Promise<Message>;
  playMessage(id: string): Promise<void>;
  deleteMessage(id: string): Promise<void>;
  messageAudio(id: string): Promise<Blob>;
  announce(text: string, lang: string): Promise<void>;

  bluetooth(): Promise<BluetoothStatus>;
  pair(): Promise<BluetoothStatus>;

  tokens(): Promise<TokenInfo[]>;
  createToken(name: string, admin: boolean): Promise<NewToken>;
  revokeToken(id: string): Promise<void>;

  ota(): Promise<OtaStatus>;
  otaCheck(): Promise<void>;
  otaReboot(): Promise<void>;

  timer(): Promise<Timer>;
  startTimer(minutes: number): Promise<Timer>;
  cancelTimer(): Promise<Timer>;
  alarms(): Promise<Alarm[]>;
  addAlarm(alarm: AlarmInput): Promise<Alarm>;
  updateAlarm(id: string, alarm: AlarmInput): Promise<Alarm>;
  removeAlarm(id: string): Promise<void>;
}

const TOKEN_KEY = "kds-token";

export function storedToken(): string | null {
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

export function storeToken(token: string | null): void {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    // Private mode: the token lasts for this page only.
  }
}

/** Talks to the speaker's API on the same origin, with the stored token. */
export class HttpClient implements ApiClient {
  constructor(private token: () => string | null = storedToken, private base = "") {}

  private async request<T>(method: string, path: string, body?: unknown, raw?: Blob): Promise<T> {
    const headers: Record<string, string> = {};
    const token = this.token();
    if (token) headers.Authorization = `Bearer ${token}`;
    let payload: BodyInit | undefined;
    if (raw) {
      headers["Content-Type"] = "audio/wav";
      payload = raw;
    } else if (body !== undefined) {
      headers["Content-Type"] = "application/json";
      payload = JSON.stringify(body);
    }
    const response = await fetch(this.base + path, { method, headers, body: payload });
    const text = await response.text();
    const json = text ? JSON.parse(text) : {};
    if (!response.ok) throw new ApiError(response.status, json.error ?? response.statusText, json);
    return json as T;
  }

  health = () => this.request<Health>("GET", "/health");
  setupStart = () => this.request<void>("POST", "/api/setup/start");
  setupComplete = (code: string, name: string) => this.request<NewToken>("POST", "/api/setup", { code, name });
  isAdmin = async () => {
    try {
      await this.request("GET", "/api/tokens");
      return true;
    } catch (e) {
      if (e instanceof ApiError && e.status === 403) return false;
      throw e;
    }
  };

  source = () => this.request<Source>("GET", "/api/audio/source");
  volume = () => this.request<Volume>("GET", "/api/audio/volume");
  setVolume = (volume: number) => this.request<Volume>("POST", "/api/audio/volume", { volume });
  setLimit = (limit: number) => this.request<Volume>("POST", "/api/audio/volume-limit", { limit });

  stations = async () => (await this.request<{ stations: Station[] }>("GET", "/api/radio/stations")).stations;
  playStation = async (id: string) => void (await this.request("POST", "/api/radio/play", { id }));
  stopRadio = async () => void (await this.request("POST", "/api/radio/stop"));
  addStation = async (name: string, url: string) =>
    (await this.request<{ station: Station }>("POST", "/api/radio/stations", { name, url })).station;
  removeStation = async (id: string) => void (await this.request("DELETE", `/api/radio/stations/${encodeURIComponent(id)}`));

  feeds = async () => (await this.request<{ feeds: Feed[] }>("GET", "/api/podcasts")).feeds;
  addFeed = async (url: string) => (await this.request<{ feed: Feed }>("POST", "/api/podcasts", { url })).feed;
  removeFeed = async (id: string) => void (await this.request("DELETE", `/api/podcasts/${encodeURIComponent(id)}`));
  episodes = async (feed: string) =>
    (await this.request<{ episodes: Episode[] }>("GET", `/api/podcasts/${encodeURIComponent(feed)}/episodes`)).episodes;
  playEpisode = async (feed: string, episode: string) => void (await this.request("POST", "/api/podcasts/play", { feed, episode }));
  stopPodcast = async () => void (await this.request("POST", "/api/podcasts/stop"));

  messages = async () => (await this.request<{ messages: Message[] }>("GET", "/api/messages")).messages;
  sendMessage = async (wav: Blob, play: boolean) =>
    (await this.request<{ message: Message }>("POST", `/api/messages?play=${play}`, undefined, wav)).message;
  renameMessage = async (id: string, name: string) =>
    (await this.request<{ message: Message }>("PATCH", `/api/messages/${encodeURIComponent(id)}`, { name })).message;
  playMessage = async (id: string) => void (await this.request("POST", `/api/messages/${encodeURIComponent(id)}/play`));
  deleteMessage = async (id: string) => void (await this.request("DELETE", `/api/messages/${encodeURIComponent(id)}`));
  messageAudio = async (id: string) => {
    const token = this.token();
    const response = await fetch(`${this.base}/api/messages/${encodeURIComponent(id)}/audio`, {
      headers: token ? { Authorization: `Bearer ${token}` } : {},
    });
    if (!response.ok) throw new ApiError(response.status, response.statusText);
    return response.blob();
  };
  announce = async (text: string, lang: string) => void (await this.request("POST", "/api/tts", { text, lang }));

  bluetooth = () => this.request<BluetoothStatus>("GET", "/api/bluetooth/status");
  pair = async () => {
    await this.request("POST", "/api/bluetooth/pair");
    return this.bluetooth();
  };

  tokens = async () => (await this.request<{ tokens: TokenInfo[] }>("GET", "/api/tokens")).tokens;
  createToken = (name: string, admin: boolean) => this.request<NewToken>("POST", "/api/tokens", { name, admin });
  revokeToken = async (id: string) => void (await this.request("DELETE", `/api/tokens/${encodeURIComponent(id)}`));

  ota = () => this.request<OtaStatus>("GET", "/api/ota/status");
  otaCheck = async () => void (await this.request("POST", "/api/ota/check"));
  otaReboot = async () => void (await this.request("POST", "/api/ota/reboot"));

  timer = () => this.request<Timer>("GET", "/api/timer");
  startTimer = (minutes: number) => this.request<Timer>("POST", "/api/timer", { minutes });
  cancelTimer = () => this.request<Timer>("DELETE", "/api/timer");
  alarms = async () => (await this.request<{ alarms: Alarm[] }>("GET", "/api/alarms")).alarms;
  addAlarm = async (alarm: AlarmInput) => (await this.request<{ alarm: Alarm }>("POST", "/api/alarms", alarm)).alarm;
  updateAlarm = async (id: string, alarm: AlarmInput) =>
    (await this.request<{ alarm: Alarm }>("PUT", `/api/alarms/${encodeURIComponent(id)}`, alarm)).alarm;
  removeAlarm = async (id: string) => void (await this.request("DELETE", `/api/alarms/${encodeURIComponent(id)}`));
}
