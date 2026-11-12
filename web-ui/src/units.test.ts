import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { DICTIONARIES, translator } from "./i18n";
import { accessLink, qrSvg, tokenFromHash } from "./share";
import { encodeWav, MAX_SECONDS, MESSAGE_RATE, resample } from "./wav";

describe("WAV for voice messages", () => {
  it("encodes 16-bit PCM mono with a correct header", () => {
    const wav = encodeWav(new Float32Array([0, 1, -1, 0.5]), 24000);
    const view = new DataView(wav.buffer);
    expect(String.fromCharCode(...wav.slice(0, 4))).toBe("RIFF");
    expect(String.fromCharCode(...wav.slice(8, 16))).toBe("WAVEfmt ");
    expect(view.getUint16(20, true)).toBe(1); // PCM
    expect(view.getUint16(22, true)).toBe(1); // mono
    expect(view.getUint32(24, true)).toBe(24000);
    expect(view.getUint16(34, true)).toBe(16);
    expect(view.getUint32(40, true)).toBe(8);
    expect(wav.length).toBe(52);
    expect(view.getInt16(46, true)).toBe(32767);
    expect(view.getInt16(48, true)).toBe(-32768);
  });

  it("resamples to 24 kHz and cuts at 30 seconds", () => {
    expect(resample(new Float32Array(48000), 48000, MESSAGE_RATE).length).toBe(24000);
    expect(resample(new Float32Array(48000 * 40), 48000, MESSAGE_RATE).length).toBe(MAX_SECONDS * MESSAGE_RATE);
  });
});

describe("sharing access", () => {
  it("puts the token in the fragment and reads it back", () => {
    const link = accessLink("https://kidsspeaker-2df5.local", "kds_abc+/=");
    expect(link).toBe("https://kidsspeaker-2df5.local/#token=kds_abc%2B%2F%3D");
    expect(tokenFromHash(new URL(link).hash)).toBe("kds_abc+/=");
    expect(tokenFromHash("#other=1")).toBeNull();
  });

  it("draws a QR code as SVG", () => {
    expect(qrSvg("https://kidsspeaker-2df5.local/#token=x")).toContain("<svg");
  });
});

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name: string) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return name === "stories" ? [] : sourceFiles(path);
    return /\.(ts|tsx)$/.test(name) && !name.includes(".test.") ? [path] : [];
  });
}

describe("translations (LOC_REQ_001)", () => {
  const used = new Set<string>();
  for (const file of sourceFiles(join(__dirname))) {
    for (const m of readFileSync(file, "utf8").matchAll(/\bt\("([a-z_.0-9]+)"/g)) used.add(m[1]);
  }
  // Keys built at run time.
  ["player", "radio", "messages", "settings"].forEach((k) => used.add(`tab.${k}`));
  [1, 2, 3, 4, 5, 6, 7].forEach((d) => used.add(`day.${d}`));
  ["idle", "downloading", "installing", "done", "error"].forEach((s) => used.add(`ota.state.${s}`));

  it("has every key the code uses, in Danish and English", () => {
    for (const lang of ["da", "en"] as const) {
      const missing = [...used].filter((k) => !(k in DICTIONARIES[lang]));
      expect(missing, lang).toEqual([]);
    }
  });

  it("has the same keys and placeholders in both languages", () => {
    expect(Object.keys(DICTIONARIES.en).sort()).toEqual(Object.keys(DICTIONARIES.da).sort());
    const vars = (s: string) => [...s.matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort();
    for (const key of Object.keys(DICTIONARIES.da)) {
      expect(vars(DICTIONARIES.en[key]), key).toEqual(vars(DICTIONARIES.da[key]));
    }
  });

  it("fills in placeholders", () => {
    expect(translator("da")("setup.wrong", { n: 3 })).toBe("Forkert kode. 3 forsøg tilbage.");
    expect(translator("en")("player.limit", { n: 75 })).toBe("at most 75 %");
  });
});
