// The API's JSON, as documented in the API Reference (DOC_REQ_003).

export type SourceName = "airplay" | "spotify" | "bluetooth" | "radio";

export interface Health {
  status: string;
  version: string;
  set_up: boolean;
}

export interface Source {
  active: SourceName | null;
  sources: { radio: { state: string; station: string | null; title: string | null } };
}

export interface Volume {
  volume: number;
  limit: number;
  db: number | null;
}

export interface Station {
  id: string;
  name: string;
  url: string;
}

export interface Feed {
  id: string;
  title: string;
  url: string;
}

export interface Episode {
  id: string;
  title: string;
  published: string | null;
  duration_s: number | null;
  url: string;
  position_s: number | null;
}

export interface Message {
  id: string;
  recorded: number;
  name: string | null;
  duration_s: number;
  last_played: number;
}

export interface TokenInfo {
  id: string;
  name: string;
  admin: boolean;
  created: number;
  last_used: number | null;
}

export interface NewToken {
  token: string;
  info: TokenInfo;
}

export interface BluetoothStatus {
  connected: boolean;
  pairing: { open: boolean; remaining_s: number };
}

export interface OtaStatus {
  /** "idle", "downloading", "installing", "done", or {"error": "..."}. */
  state: string | { error: string };
  detail: string | null;
  version: string;
}

export interface Timer {
  running: boolean;
  minutes?: number;
  remaining_s?: number;
}

export interface Alarm {
  id: string;
  name: string | null;
  time: string;
  days: number[];
  station: string;
  volume: number;
  enabled: boolean;
}

export type AlarmInput = Omit<Alarm, "id">;

/** An error the API answered with. */
export class ApiError extends Error {
  constructor(
    public status: number,
    message: string,
    public body: Record<string, unknown> = {},
  ) {
    super(message);
  }
}
