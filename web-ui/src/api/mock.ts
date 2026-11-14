import type { ApiClient } from "./client";
import { OnConnected, OnEvent, same, Topic, TOPICS } from "../live";
import {
  Alarm, AlarmInput, ApiError, BluetoothStatus, Episode, Feed, Health, Message, NewToken, OtaStatus,
  Source, Station, Timer, TokenInfo, Volume,
} from "./types";

/** How the mock speaker starts out. */
export interface MockOptions {
  setUp?: boolean;
  admin?: boolean;
  playing?: string | null;
  programme?: string | null;
  volume?: number;
  limit?: number;
  /** The code the "speaker reads aloud". */
  code?: string;
  stations?: Station[];
  messages?: Message[];
  feeds?: Feed[];
  ota?: OtaStatus;
  /** Calls answer after this many milliseconds, to show loading states. */
  delay?: number;
  /** An update reboots in this many seconds (the grace period, OTA_REQ_010). */
  rebootIn?: number;
  /** Live updates; off shows how the app copes without them. Default on. */
  live?: boolean;
}

const DAY = 86_400;
const now = () => Math.floor(Date.now() / 1000);

export const SAMPLE_STATIONS: Station[] = [
  { id: "dr-p1", name: "DR P1", url: "http://live-icy.dr.dk/A/A03H.mp3" },
  { id: "dr-p2", name: "DR P2", url: "http://live-icy.dr.dk/A/A04H.mp3" },
  { id: "dr-p3", name: "DR P3", url: "http://live-icy.dr.dk/A/A05H.mp3" },
];

export const SAMPLE_MESSAGES: Message[] = [
  { id: "m2", recorded: now() - 3600, name: "Godnat", duration_s: 12.4, last_played: now() - 3600 },
  { id: "m1", recorded: now() - 2 * DAY, name: null, duration_s: 5.1, last_played: now() - 2 * DAY },
];

export const SAMPLE_FEEDS: Feed[] = [
  { id: "f1", title: "Børneradio", url: "https://example.com/born.xml" },
];

const SAMPLE_EPISODES: Episode[] = [
  { id: "e2", title: "Dinosaurerne", published: "Tue, 10 Nov 2026 07:00:00 +0100", duration_s: 1260, url: "http://x/2.mp3", position_s: 312 },
  { id: "e1", title: "Rummet", published: "Tue, 03 Nov 2026 07:00:00 +0100", duration_s: 1140, url: "http://x/1.mp3", position_s: null },
];

/** An in-memory speaker for Storybook and the tests. It keeps state, so a
 *  story can be clicked through: playing a station shows it as playing. */
export class MockClient implements ApiClient {
  setUp: boolean;
  admin: boolean;
  playing: string | null;
  programme: string | null;
  volumeValue: number;
  limit: number;
  code: string;
  attemptsLeft = 5;
  stationList: Station[];
  messageList: Message[];
  feedList: Feed[];
  tokenList: TokenInfo[];
  otaStatus: OtaStatus;
  timerEnd: number | null = null;
  timerMinutes = 0;
  alarmList: Alarm[] = [];
  pairingUntil = 0;
  announced: { text: string; lang: string }[] = [];
  played: string[] = [];
  delay: number;
  live: boolean;
  private next = 1;

  constructor(o: MockOptions = {}) {
    this.setUp = o.setUp ?? true;
    this.admin = o.admin ?? true;
    this.playing = o.playing ?? null;
    this.programme = o.programme ?? null;
    this.volumeValue = o.volume ?? 60;
    this.limit = o.limit ?? 75;
    this.code = o.code ?? "471293";
    this.stationList = [...(o.stations ?? SAMPLE_STATIONS)];
    this.messageList = [...(o.messages ?? SAMPLE_MESSAGES)];
    this.feedList = [...(o.feeds ?? SAMPLE_FEEDS)];
    this.otaStatus = o.ota ?? { state: "idle", detail: null, version: "v0.4.0" };
    this.tokenList = [
      { id: "t1", name: "Tues telefon", admin: true, created: now() - 30 * DAY, last_used: now() - 60 },
      { id: "t2", name: "Idas iPad", admin: false, created: now() - 10 * DAY, last_used: now() - DAY },
    ];
    this.delay = o.delay ?? 0;
    this.live = o.live ?? true;
    if (o.rebootIn !== undefined) {
      this.otaStatus = { ...this.otaStatus, state: "done", pending: "v0.5.0", reboot_at: now() + o.rebootIn };
    }
  }

  private async answer<T>(value: T | (() => T)): Promise<T> {
    if (this.delay) await new Promise((r) => setTimeout(r, this.delay));
    return typeof value === "function" ? (value as () => T)() : value;
  }

  private needAdmin() {
    if (!this.admin) throw new ApiError(403, "an admin token is required");
  }

  health = () => this.answer<Health>(() => ({ status: "ok", version: this.otaStatus.version, set_up: this.setUp }));
  setupStart = () => this.answer<void>(() => { this.attemptsLeft = 5; });
  setupComplete = (code: string, name: string) =>
    this.answer<NewToken>(() => {
      if (code.trim() !== this.code) {
        this.attemptsLeft -= 1;
        if (this.attemptsLeft <= 0) throw new ApiError(409, "no setup code is active; ask the speaker for one");
        throw new ApiError(403, "wrong code", { attempts_left: this.attemptsLeft });
      }
      this.setUp = true;
      this.admin = true;
      const info: TokenInfo = { id: `t${++this.next}`, name, admin: true, created: now(), last_used: null };
      this.tokenList.push(info);
      return { token: `kds_mock_${info.id}`, info };
    });
  isAdmin = () => this.answer(() => this.admin);

  source = () =>
    this.answer<Source>(() => ({
      active: this.playing ? "radio" : null,
      sources: { radio: { state: this.playing ? "playing" : "idle", station: this.playing, title: this.programme } },
    }));
  private volumeReply = (): Volume => ({
    volume: this.volumeValue,
    limit: this.limit,
    db: this.volumeValue === 0 ? null : Math.round(-63 * (1 - this.volumeValue / 100) * 10) / 10,
  });
  volume = () => this.answer(this.volumeReply);
  setVolume = (v: number) => this.answer(() => { this.volumeValue = Math.min(v, this.limit); return this.volumeReply(); });
  setLimit = (l: number) =>
    this.answer(() => {
      this.needAdmin();
      this.limit = l;
      this.volumeValue = Math.min(this.volumeValue, l);
      return this.volumeReply();
    });

  stations = () => this.answer(() => [...this.stationList]);
  playStation = (id: string) =>
    this.answer<void>(() => {
      const s = this.stationList.find((x) => x.id === id);
      if (!s) throw new ApiError(404, `no station ${id}`);
      this.playing = id;
      this.programme = `${s.name} – live`;
    });
  stopRadio = () => this.answer<void>(() => { this.playing = null; this.programme = null; });
  addStation = (name: string, url: string) =>
    this.answer(() => {
      if (!/^https?:\/\/[^/]/.test(url)) throw new ApiError(400, "url must start with http:// or https://");
      const s = { id: name.toLowerCase().replace(/[^a-z0-9]+/g, "-"), name, url };
      this.stationList.push(s);
      return s;
    });
  removeStation = (id: string) => this.answer<void>(() => { this.stationList = this.stationList.filter((s) => s.id !== id); });

  feeds = () => this.answer(() => [...this.feedList]);
  addFeed = (url: string) =>
    this.answer(() => {
      const f = { id: `f${++this.next}`, title: "Ny podcast", url };
      this.feedList.push(f);
      return f;
    });
  removeFeed = (id: string) => this.answer<void>(() => { this.feedList = this.feedList.filter((f) => f.id !== id); });
  episodes = (feed: string) =>
    this.answer(() => {
      if (!this.feedList.some((f) => f.id === feed)) throw new ApiError(404, `no podcast ${feed}`);
      return SAMPLE_EPISODES;
    });
  playEpisode = (_feed: string, episode: string) =>
    this.answer<void>(() => {
      const e = SAMPLE_EPISODES.find((x) => x.id === episode);
      this.playing = "podcast";
      this.programme = e?.title ?? null;
    });
  stopPodcast = () => this.stopRadio();

  messages = () => this.answer(() => [...this.messageList].sort((a, b) => b.recorded - a.recorded));
  sendMessage = (wav: Blob, play: boolean) =>
    this.answer(() => {
      const m: Message = { id: `m${++this.next}`, recorded: now(), name: null, duration_s: Math.round(wav.size / 4800) / 10, last_played: now() };
      this.messageList.push(m);
      if (play) this.played.push(m.id);
      return m;
    });
  renameMessage = (id: string, name: string) =>
    this.answer(() => {
      const m = this.messageList.find((x) => x.id === id);
      if (!m) throw new ApiError(404, `no message ${id}`);
      m.name = name.trim() || null;
      return { ...m };
    });
  playMessage = (id: string) => this.answer<void>(() => { this.played.push(id); });
  deleteMessage = (id: string) => this.answer<void>(() => { this.messageList = this.messageList.filter((m) => m.id !== id); });
  messageAudio = () => this.answer(() => new Blob([], { type: "audio/wav" }));
  announce = (text: string, lang: string) => this.answer<void>(() => { this.announced.push({ text, lang }); });

  bluetooth = () =>
    this.answer<BluetoothStatus>(() => {
      const left = Math.max(0, Math.round((this.pairingUntil - Date.now()) / 1000));
      return { connected: false, pairing: { open: left > 0, remaining_s: left } };
    });
  pair = () => {
    this.pairingUntil = Date.now() + 180_000;
    return this.bluetooth();
  };

  tokens = () => this.answer(() => { this.needAdmin(); return [...this.tokenList]; });
  createToken = (name: string, admin: boolean) =>
    this.answer<NewToken>(() => {
      this.needAdmin();
      const info: TokenInfo = { id: `t${++this.next}`, name, admin, created: now(), last_used: null };
      this.tokenList.push(info);
      return { token: `kds_mock_${info.id}_${"0".repeat(48)}`, info };
    });
  revokeToken = (id: string) =>
    this.answer<void>(() => {
      this.needAdmin();
      const t = this.tokenList.find((x) => x.id === id);
      if (t?.admin && this.tokenList.filter((x) => x.admin).length === 1) {
        throw new ApiError(409, "the last admin token cannot be revoked");
      }
      this.tokenList = this.tokenList.filter((x) => x.id !== id);
    });

  ota = () => this.answer(() => ({ ...this.otaStatus }));
  otaCheck = () => this.answer<void>(() => {});
  otaReboot = () => this.answer<void>(() => {});

  private timerReply = (): Timer =>
    this.timerEnd && this.timerEnd > Date.now()
      ? { running: true, minutes: this.timerMinutes, remaining_s: Math.round((this.timerEnd - Date.now()) / 1000) }
      : { running: false };
  timer = () => this.answer(this.timerReply);
  startTimer = (minutes: number) =>
    this.answer(() => {
      this.timerMinutes = minutes;
      this.timerEnd = Date.now() + minutes * 60_000;
      return this.timerReply();
    });
  cancelTimer = () => this.answer(() => { this.timerEnd = null; return this.timerReply(); });
  alarms = () => this.answer(() => [...this.alarmList]);
  addAlarm = (a: AlarmInput) =>
    this.answer(() => {
      const alarm = { ...a, id: `alarm-${this.alarmList.length + 1}` };
      this.alarmList.push(alarm);
      return alarm;
    });
  updateAlarm = (id: string, a: AlarmInput) =>
    this.answer(() => {
      const i = this.alarmList.findIndex((x) => x.id === id);
      if (i < 0) throw new ApiError(404, `no alarm ${id}`);
      this.alarmList[i] = { ...a, id };
      return this.alarmList[i];
    });
  removeAlarm = (id: string) => this.answer<void>(() => { this.alarmList = this.alarmList.filter((a) => a.id !== id); });

  private topic(topic: Topic): Promise<unknown> {
    switch (topic) {
      case "source": return this.source();
      case "volume": return this.volume();
      case "timer": return this.timer();
      case "bluetooth": return this.bluetooth();
      case "ota": return this.ota();
    }
  }

  /** Like the speaker's Event Hub: every topic, then each change, looked for
   *  every 300 ms. A change made directly on the mock shows up too. */
  events = (onEvent: OnEvent, onConnected: OnConnected) => {
    if (!this.live) {
      onConnected(false);
      return () => {};
    }
    const last = new Map<Topic, [unknown, number]>();
    let stopped = false;
    const look = async () => {
      for (const topic of TOPICS) {
        const value = await this.topic(topic);
        const prev = last.get(topic);
        if (stopped || (prev && same(prev[0], value, Date.now() - prev[1]))) continue;
        last.set(topic, [value, Date.now()]);
        onEvent(topic, value);
      }
    };
    onConnected(true);
    look();
    const id = setInterval(look, 300);
    return () => {
      stopped = true;
      clearInterval(id);
    };
  };
}
