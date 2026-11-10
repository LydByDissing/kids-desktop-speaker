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
log "copying the bundle to the Pi..."
"${SCP[@]}" "$WORK/dev/image.raucb" "$PI_HOST:/data/kds.raucb" || exit 1
log "installing..."
remote "rauc install /data/kds.raucb >/tmp/install.log 2>&1; echo \$? >/tmp/install.rc; rm -f /data/kds.raucb"
[ "$(remote cat /tmp/install.rc)" = 0 ] || { log "install failed:"; remote tail -5 /tmp/install.log; exit 1; }
id=$(boot_id); reboot_pi
wait_for_reboot "$id" 420 || { log "Pi did not come back"; exit 1; }
log "running $(remote cat /etc/kds-version) from $(remote rauc status | sed -n 's/^Booted from: //p')"
