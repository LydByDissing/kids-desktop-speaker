import { useCallback } from "preact/hooks";
import { useApp } from "../context";
import { formatDuration, useCountdown, useTopic } from "../hooks";

/** While an update's reboot waits out its grace period, every screen shows
 *  when it happens (OTA_REQ_010); a parent may restart at once (OTA_REQ_007). */
export function RebootBanner() {
  const { client, t, admin, notify } = useApp();
  const ota = useTopic("ota", useCallback(() => client.ota(), [client]), 30_000);
  const rebootAt = ota.data?.reboot_at ?? null;
  // The seconds left when the status arrived; useCountdown ticks from there.
  const left = rebootAt ? Math.max(0, rebootAt - Math.floor(ota.at / 1000)) : 0;
  const now = useCountdown(left, ota.at);
  if (!rebootAt) return null;
  return (
    <div class="banner" role="status">
      <span>{t("reboot.banner", { time: formatDuration(now) })}</span>
      {admin && (
        <button class="link" onClick={() => client.otaReboot().catch(() => notify(t("error.generic")))}>
          {t("ota.reboot")}
        </button>
      )}
    </div>
  );
}
