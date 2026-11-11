import { useApp } from "../context";

/** The device volume. It cannot go above the limit (VOL_REQ_001); the part
 *  above the limit is shown as out of reach. */
export function VolumeSlider({ volume, limit, onChange }: { volume: number; limit: number; onChange: (v: number) => void }) {
  const { t } = useApp();
  return (
    <div class="volume">
      <label for="volume">{t("player.volume")}</label>
      <div class="volume-row">
        <span aria-hidden="true">🔈</span>
        <input
          id="volume"
          type="range"
          min={0}
          max={limit}
          step={5}
          value={volume}
          style={{ width: `${Math.max(limit, 10)}%` }}
          aria-valuetext={`${volume} %`}
          onChange={(e) => onChange(Number((e.target as HTMLInputElement).value))}
        />
        <span class="out-of-reach" style={{ width: `${100 - limit}%` }} title={t("player.limit", { n: limit })} />
        <span aria-hidden="true">🔊</span>
      </div>
      <p class="muted small">{volume === 0 ? t("player.muted") : `${volume} %`} · {t("player.limit", { n: limit })}</p>
    </div>
  );
}
