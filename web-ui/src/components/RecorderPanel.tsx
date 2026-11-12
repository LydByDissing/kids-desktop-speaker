import { useEffect, useRef, useState } from "preact/hooks";
import { useApp } from "../context";
import type { Recording } from "../wav";
import { MAX_SECONDS } from "../wav";

type Stage = "idle" | "recording" | "preview" | "sending";

/** Record up to 30 s, listen back, send (journey 3, MSG_REQ_001). */
export function RecorderPanel({ onSent }: { onSent: () => void }) {
  const { client, recorder, t, notify } = useApp();
  const [stage, setStage] = useState<Stage>("idle");
  const [elapsed, setElapsed] = useState(0);
  const [wav, setWav] = useState<Blob | null>(null);
  const recording = useRef<Recording | null>(null);
  const preview = wav && typeof URL.createObjectURL === "function" ? URL.createObjectURL(wav) : undefined;

  const stop = async () => {
    const r = recording.current;
    recording.current = null;
    if (!r) return;
    try {
      setWav(await r.stop());
      setStage("preview");
    } catch {
      notify(t("messages.record_failed"));
      setStage("idle");
    }
  };

  useEffect(() => {
    if (stage !== "recording") return;
    const started = Date.now();
    const id = setInterval(() => {
      const s = (Date.now() - started) / 1000;
      setElapsed(s);
      if (s >= MAX_SECONDS) stop();
    }, 200);
    return () => clearInterval(id);
  }, [stage]);

  const start = async () => {
    try {
      recording.current = await recorder();
      setElapsed(0);
      setStage("recording");
    } catch {
      notify(t("messages.no_mic"));
    }
  };

  const send = async () => {
    if (!wav) return;
    setStage("sending");
    try {
      await client.sendMessage(wav, true);
      notify(t("messages.sent"));
      setWav(null);
      setStage("idle");
      onSent();
    } catch {
      notify(t("error.generic"));
      setStage("preview");
    }
  };

  const left = Math.max(0, Math.ceil(MAX_SECONDS - elapsed));
  const ring = 2 * Math.PI * 44;
  return (
    <div class="card recorder">
      {stage === "idle" && (
        <>
          <button class="record" onClick={start}>🎙 {t("messages.record")}</button>
          <p class="muted small">{t("messages.limit")}</p>
        </>
      )}
      {stage === "recording" && (
        <>
          <svg viewBox="0 0 100 100" class="ring" role="img" aria-label={t("messages.left", { n: left })}>
            <circle cx="50" cy="50" r="44" class="ring-bg" />
            <circle cx="50" cy="50" r="44" class="ring-fg" stroke-dasharray={ring} stroke-dashoffset={(ring * elapsed) / MAX_SECONDS} />
            <text x="50" y="58" text-anchor="middle">{left}</text>
          </svg>
          <button class="primary" onClick={stop}>⏹ {t("messages.stop")}</button>
        </>
      )}
      {(stage === "preview" || stage === "sending") && (
        <>
          {preview && <audio controls src={preview} />}
          <div class="row">
            <button class="primary" disabled={stage === "sending"} onClick={send}>{t("messages.send")}</button>
            <button disabled={stage === "sending"} onClick={() => { setWav(null); setStage("idle"); }}>{t("messages.again")}</button>
          </div>
        </>
      )}
    </div>
  );
}
