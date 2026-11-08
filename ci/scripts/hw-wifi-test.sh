#!/bin/bash
# hw-wifi-test.sh — repeated network use on a real Pi Zero W (KDS-3kd)
# ====================================================================
# Usage: PI_HOST=root@kidsspeaker-XXXX.local hw-wifi-test.sh
#
# Installs the build under test, checks Wi-Fi power saving is off, then makes
# repeated HTTPS release checks and full bundle downloads from GitHub, as a device
# does when updating, and reports any failure. Uses a throwaway signing key on the
# device, so reflash it afterwards.

set -uo pipefail
: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK="$REPO/ci/output/wifitest"
CHECKS="${KDS_WIFI_CHECKS:-50}"
DOWNLOADS="${KDS_WIFI_DOWNLOADS:-3}"
RELEASE=https://github.com/LydByDissing/kids-desktop-speaker/releases/download/v0.1.0-rc.2
source "$REPO/ci/scripts/hw-lib.sh"
trap hw_cleanup EXIT

remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
hw_cleanup; mkdir -p "$WORK"

log "=== Installing the build under test"
make_test_identity || exit 1
make_bundle base v0.4.1-wifi-test none || exit 1
trust_test_identity
"${SCP[@]}" "$WORK/base/image.raucb" "$PI_HOST:/data/ota/base.raucb" || exit 1
remote 'rauc install /data/ota/base.raucb >/tmp/install.log 2>&1; rc=$?; rm -f /data/ota/base.raucb; exit $rc' \
    || { log "install failed"; remote tail -5 /tmp/install.log; exit 1; }
OTHER=$([ "$(slot)" = A ] && echo B || echo A)
id=$(boot_id); reboot_pi
wait_for_reboot "$id" 420 || { log "Pi did not come back"; exit 1; }
wait_settled "$OTHER" 300 || { log "did not settle on slot $OTHER"; exit 1; }
check "runs the build under test" "$(remote cat /etc/kds-version)" v0.4.1-wifi-test
check "Wi-Fi power saving is off" "$(remote 'iw dev wlan0 get power_save')" "Power save: off"
remote "cat /proc/net/wireless | tail -1 | awk '{print \"  signal: \" \$4 \" dBm\"}'"

log "=== $CHECKS HTTPS release checks"
result=$(remote "fail=0; total=0; for i in \$(seq 1 $CHECKS); do s=\$(date +%s%N); if curl -sf -m 30 -o /dev/null $RELEASE/manifest.json; then total=\$((total + (\$(date +%s%N) - s) / 1000000)); else fail=\$((fail + 1)); fi; sleep 2; done; echo \"\$fail \$total\"")
read -r fails total <<<"$result"
ok_checks=$((CHECKS - ${fails:-$CHECKS}))
log "  $ok_checks of $CHECKS succeeded, average $(( ok_checks > 0 ? ${total:-0} / ok_checks : 0 )) ms"
check "every release check succeeded" "${fails:-?}" 0

log "=== $DOWNLOADS full bundle downloads"
for i in $(seq 1 "$DOWNLOADS"); do
    line=$(remote "curl -sfL -m 900 -o /data/ota/wifi.raucb -w '%{size_download} %{time_total} %{speed_download}' $RELEASE/image.raucb; echo \" \$?\"; rm -f /data/ota/wifi.raucb")
    read -r size secs speed rc <<<"$line"
    log "  download $i: rc=${rc:-?} $(( ${size%.*:-0} / 1000000 )) MB in ${secs:-?} s ($(( ${speed%.*:-0} / 1000 )) kB/s)"
    check "download $i succeeded" "${rc:-?}" 0
done
check "driver reported no timeouts" "$(remote "journalctl -k -b --no-pager | grep -ciE 'brcmf.*(timeout|failed)'")" 0

if [ "$fail" -ne 0 ]; then log "Wi-Fi tests FAILED"; exit 1; fi
log "All Wi-Fi tests passed."
