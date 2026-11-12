// Recorded messages are uploaded as 16-bit PCM WAV, 24 kHz mono (DEC-037), so
// the speaker needs no decoder.

export const MESSAGE_RATE = 24_000;
export const MAX_SECONDS = 30;

/** Encodes mono samples (-1..1) as a 16-bit PCM WAV file. */
export function encodeWav(samples: Float32Array, rate: number): Uint8Array<ArrayBuffer> {
  const data = samples.length * 2;
  const out = new Uint8Array(new ArrayBuffer(44 + data));
  const view = new DataView(out.buffer);
  const text = (at: number, s: string) => [...s].forEach((c, i) => view.setUint8(at + i, c.charCodeAt(0)));
  text(0, "RIFF");
  view.setUint32(4, 36 + data, true);
  text(8, "WAVEfmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true); // PCM
  view.setUint16(22, 1, true); // mono
  view.setUint32(24, rate, true);
  view.setUint32(28, rate * 2, true);
  view.setUint16(32, 2, true);
  view.setUint16(34, 16, true);
  text(36, "data");
  view.setUint32(40, data, true);
  samples.forEach((s, i) => {
    const v = Math.max(-1, Math.min(1, s));
    view.setInt16(44 + i * 2, v < 0 ? v * 0x8000 : v * 0x7fff, true);
  });
  return out;
}

/** Linear resampling, enough for speech. Cuts to MAX_SECONDS. */
export function resample(samples: Float32Array, from: number, to: number): Float32Array {
  const length = Math.min(Math.floor((samples.length * to) / from), MAX_SECONDS * to);
  const out = new Float32Array(length);
  const step = from / to;
  for (let i = 0; i < length; i++) {
    const pos = i * step;
    const j = Math.floor(pos);
    const frac = pos - j;
    out[i] = (samples[j] ?? 0) * (1 - frac) + (samples[j + 1] ?? samples[j] ?? 0) * frac;
  }
  return out;
}

/** A recording in progress. */
export interface Recording {
  /** Stops and returns the message as WAV. */
  stop(): Promise<Blob>;
  cancel(): void;
}

export type Recorder = () => Promise<Recording>;

/** Records from the microphone (needs HTTPS) and converts to WAV. */
export const browserRecorder: Recorder = async () => {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
  const recorder = new MediaRecorder(stream);
  const chunks: Blob[] = [];
  recorder.ondataavailable = (e) => chunks.push(e.data);
  recorder.start();
  const release = () => stream.getTracks().forEach((t) => t.stop());
  return {
    stop: () =>
      new Promise<Blob>((resolve, reject) => {
        recorder.onstop = async () => {
          release();
          try {
            const encoded = await new Blob(chunks).arrayBuffer();
            const context = new AudioContext();
            const audio = await context.decodeAudioData(encoded);
            await context.close();
            const mono = audio.getChannelData(0);
            resolve(new Blob([encodeWav(resample(mono, audio.sampleRate, MESSAGE_RATE), MESSAGE_RATE)], { type: "audio/wav" }));
          } catch (e) {
            reject(e);
          }
        };
        recorder.stop();
      }),
    cancel: () => {
      recorder.onstop = null;
      if (recorder.state !== "inactive") recorder.stop();
      release();
    },
  };
};

/** A recorder that "records" a short tone, for Storybook and the tests. */
export const fakeRecorder: Recorder = async () => ({
  stop: async () => {
    const tone = new Float32Array(MESSAGE_RATE * 2).map((_, i) => 0.3 * Math.sin((2 * Math.PI * 440 * i) / MESSAGE_RATE));
    return new Blob([encodeWav(tone, MESSAGE_RATE)], { type: "audio/wav" });
  },
  cancel: () => {},
});
