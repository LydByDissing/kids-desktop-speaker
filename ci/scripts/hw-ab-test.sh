#!/bin/bash
# hw-ab-test.sh — prove A/B updates and rollback on a real Pi Zero W (KDS-hf9)
# ============================================================================
# Usage: PI_HOST=root@kidsspeaker-XXXX.local hw-ab-test.sh
# Use the mDNS name rather than an IP address, which can change after an update.
#
# Requires a Pi running our SD image, and ci/output/rootfs.tar (make sd-image).
# Installs a good update, then two broken ones, and checks that the Pi falls back
# by itself. Bundles use a throwaway key that is added to this Pi's keyring only,
# so the device under test is left with test keys: reflash it afterwards.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK="$REPO/ci/output/abtest"
GOOD=v0.2.0-test
source "$REPO/ci/scripts/hw-lib.sh"
trap hw_cleanup EXIT

install_and_reboot() { # bundle-name [may-not-return]
    log "copying $1 to the Pi..."
    "${SCP[@]}" "$WORK/$1/image.raucb" "$PI_HOST:/data/ota/test.raucb" || { log "copy failed"; return 1; }
    log "installing $1..."
    remote "rauc install /data/ota/test.raucb >/tmp/install.log 2>&1; echo \$?; rm -f /data/ota/test.raucb" | tail -1 > "$WORK/rc"
    [ "$(cat "$WORK/rc")" = 0 ] || { log "install failed:"; remote tail -5 /tmp/install.log; return 1; }
    local id; id=$(boot_id)
    reboot_pi
    # A broken slot may reboot before it is ever reachable. Its result is checked
    # once the Pi settles.
    if ! wait_for_reboot "$id" 420; then
        [ "${2:-}" = may-not-return ] && return 0
        log "Pi did not come back"; return 1
    fi
}

[ -f "$ROOTFS" ] || { echo "error: missing $ROOTFS" >&2; exit 2; }
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
hw_cleanup; mkdir -p "$WORK"

log "=== Setup"
make_test_identity || exit 1
make_bundle good "$GOOD" none || exit 1
# bad-wifi needs Wi-Fi configured, so the VM leaves it out. bad-kernel is opt-in:
# a kernel that hangs before Linux arms the watchdog leaves the device stuck until
# power-cycled (KDS-fjl), which strands an unattended Pi.
BAD_UPDATES="${HW_BAD_UPDATES:-bad-api bad-wifi}"
for bad in $BAD_UPDATES; do
    make_bundle "$bad" "v0.3.0-$bad" "$bad" || exit 1
done

# The boot partition isn't updated by RAUC, so install the current boot script.
docker run --rm -v "$REPO/ci/boot:/b:ro" -v "$WORK:/o" debian:bookworm-slim bash -c \
    'apt-get update -qq; apt-get install -y -qq --no-install-recommends u-boot-tools >/dev/null 2>&1; mkimage -q -A arm -T script -C none -n "KDS A/B boot" -d /b/boot.cmd /o/boot.scr; chmod a+r /o/boot.scr'
"${SCP[@]}" "$WORK/boot.scr" "$PI_HOST:/boot/firmware/boot.scr"
trust_test_identity
summary

log "=== Test 1: good update"
START=$(slot)
OTHER=$([ "$START" = A ] && echo B || echo A)
install_and_reboot good || fail=1
wait_settled "$OTHER" 300 || log "  did not settle on slot $OTHER"
summary
check "booted the other slot" "$(slot)" "$OTHER"
check "runs the new version" "$(remote cat /etc/kds-version)" "$GOOD"
check "new slot was marked good" "$(remote systemctl is-active rauc-mark-good)" active
check "services are healthy" "$(remote systemctl is-system-running)" running
check "API answers" "$(remote curl -s -m 5 -o /dev/null -w '%{http_code}' http://localhost:8080/health)" 200
check "boot partition was updated from the new slot" "$(remote cat /boot/firmware/kds-boot.version)" "$GOOD"
check "machine-id is unique to the device" "$(remote "grep -c 1278cb00626849cb8a226e307f448a55 /etc/machine-id")" 0
GOOD_SLOT=$OTHER

for bad in $BAD_UPDATES; do
    log "=== Test: $bad update falls back"
    t0=$SECONDS
    BAD_SLOT=$([ "$GOOD_SLOT" = A ] && echo B || echo A)
    install_and_reboot "$bad" may-not-return || fail=1
    # Three failed tries, each a full boot plus the health check's wait.
    wait_settled "$GOOD_SLOT" 1800 || log "  did not settle back on slot $GOOD_SLOT"
    log "  settled after $((SECONDS - t0))s"
    summary
    check "fell back to the good slot" "$(slot)" "$GOOD_SLOT"
    check "used up the bad slot's tries" "$(remote fw_printenv -n BOOT_${BAD_SLOT}_LEFT)" 0
    check "runs the good version" "$(remote cat /etc/kds-version)" "$GOOD"
    check "services are healthy" "$(remote systemctl is-system-running)" running
done

if [ "$fail" -ne 0 ]; then log "A/B tests FAILED"; exit 1; fi
log "All A/B tests passed."
