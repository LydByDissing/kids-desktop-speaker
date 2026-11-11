#!/bin/bash
# hw-install.sh — install the current build on a development Pi and reboot into it
# ================================================================================
# Usage: PI_HOST=root@host hw-install.sh [version]
#
# Builds a bundle from ci/output/rootfs.tar (make -C ci export-rootfs), signed with
# a throwaway key that is added to this Pi's keyring only, as the hardware tests
# do. The Pi installs it into its other slot and reboots. For development Pis
# only: release devices trust only the release key.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK="$REPO/ci/output/hwinstall"
VERSION=${1:-v0.0.0-dev-$(git -C "$REPO" rev-parse --short HEAD)}
source "$REPO/ci/scripts/hw-lib.sh"
trap hw_cleanup EXIT

[ -f "$ROOTFS" ] || { echo "error: missing $ROOTFS (run make -C ci export-rootfs)" >&2; exit 2; }
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
hw_cleanup; mkdir -p "$WORK"

make_test_identity || exit 1
trust_test_identity || exit 1
make_bundle dev "$VERSION" none || exit 1
size=$(stat -c %s "$WORK/dev/image.raucb")
free=$(remote "df --output=avail -B1 /data | tail -1")
if [ "${free:-0}" -lt $((size + 50 * 1024 * 1024)) ]; then
    log "not enough space on the Pi's /data: $((free / 1048576)) MB free, the bundle is $((size / 1048576)) MB"
    exit 1
fi
log "copying the bundle to the Pi ($((size / 1048576)) MB)..."
"${SCP[@]}" "$WORK/dev/image.raucb" "$PI_HOST:/data/kds.raucb" || { log "copy failed"; remote "rm -f /data/kds.raucb"; exit 1; }
log "installing..."
remote "rauc install /data/kds.raucb >/tmp/install.log 2>&1; echo \$? >/tmp/install.rc; rm -f /data/kds.raucb"
[ "$(remote cat /tmp/install.rc)" = 0 ] || { log "install failed:"; remote tail -5 /tmp/install.log; exit 1; }
id=$(boot_id); reboot_pi
wait_for_reboot "$id" 420 || { log "Pi did not come back"; exit 1; }
log "running $(remote cat /etc/kds-version) from $(remote rauc status | sed -n 's/^Booted from: //p')"

# A speaker that isn't set up reads its setup code aloud until it is
# (AUTH_REQ_003). A development Pi is set up over loopback instead, unless
# KDS_HW_LEAVE_UNCONFIGURED=1. The token itself is not kept.
if [ "${KDS_HW_LEAVE_UNCONFIGURED:-0}" != 1 ]; then
    for _ in $(seq 30); do
        remote "curl -s -o /dev/null http://127.0.0.1:8080/health" && break
        sleep 2
    done
    if remote "curl -s http://127.0.0.1:8080/api/tokens" | grep -q '"admin":true'; then
        log "already set up"
    else
        remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"name\": \"development\", \"admin\": true}' http://127.0.0.1:8080/api/tokens" \
            && log "set up as a development speaker (admin token 'development', not kept)"
    fi
fi
