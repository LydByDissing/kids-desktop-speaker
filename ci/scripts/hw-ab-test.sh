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
ROOTFS="$REPO/ci/output/rootfs.tar"
WORK="$REPO/ci/output/abtest"
IMAGE=kids-desktop-speaker:latest
PLATFORM=linux/arm/v6
# Host keys live on the data partition, so they stay the same across slots.
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new "$PI_HOST")
SCP=(scp -q -o BatchMode=yes -o ConnectTimeout=20 -o StrictHostKeyChecking=accept-new)
GOOD=v0.2.0-test

fail=0
log() { echo "[$(date +%H:%M:%S)] $*"; }
check() { if [ "$2" = "$3" ]; then log "  [PASS] $1"; else log "  [FAIL] $1 (got '$2', want '$3')"; fail=1; fi; }
remote() { "${SSH[@]}" "$@" 2>/dev/null; }
cleanup() {
    docker run --rm -v "$REPO/ci/output:/o" --entrypoint /bin/rm debian:bookworm-slim -rf /o/abtest >/dev/null 2>&1
}
trap cleanup EXIT

make_bundle() { # name version mutation
    local dir="$WORK/$1"; mkdir -p "$dir"
    python3 - "$ROOTFS" "$dir/rootfs.tar" "$2" "$3" "$WORK/id/keyring.pem" <<'PY'
import io, sys, tarfile
src_path, dst_path, version, mutation, test_ca = sys.argv[1:6]
extra_ca = open(test_ca, "rb").read()
with tarfile.open(src_path) as src, tarfile.open(dst_path, "w", format=tarfile.GNU_FORMAT) as dst:
    for m in src:
        name = m.name.removeprefix("./")
        data = src.extractfile(m).read() if m.isreg() else None
        if name == "etc/kds-version":
            data = version.encode() + b"\n"
        elif name == "etc/rauc/keyring.pem":
            data += extra_ca
        elif mutation == "bad-api" and name == "usr/local/bin/api-server":
            data = b"#!/bin/sh\nexit 1\n"
        elif mutation == "bad-kernel" and name == "boot/vmlinuz-kds":
            data = data[:4096]
        if data is not None:
            m.size = len(data)
            dst.addfile(m, io.BytesIO(data))
        else:
            dst.addfile(m)
PY
    docker run --rm --platform "$PLATFORM" --tmpfs /verify:size=1g -v "$dir:/output" \
        -v "$REPO/ci/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro" \
        -v "$WORK/id/keyring.pem:/etc/rauc/keyring.pem:ro" \
        -e VERSION="$2" -e RAUC_SIGNING_KEY="$(cat "$WORK/id/signing.key")" \
        -e RAUC_SIGNING_CERT="$(cat "$WORK/id/signing.crt")" -e QEMU_CPU=arm1176 \
        --entrypoint /bin/bash "$IMAGE" /usr/local/bin/create-bundle.sh >"$dir/log" 2>&1 \
        || { tail -5 "$dir/log"; return 1; }
    log "built bundle $1: $(du -h "$dir/image.raucb" | cut -f1)"
    docker run --rm -v "$dir:/d" --entrypoint /bin/rm debian:bookworm-slim -f /d/rootfs.tar
}

boot_id() { remote cat /proc/sys/kernel/random/boot_id; }

# Waits for the Pi to answer with a boot ID other than $1. A rollback reboots
# several times, so also waits until it settles on one boot.
wait_for_reboot() {
    local old="$1" limit="$2" start=$SECONDS id
    while [ $((SECONDS - start)) -lt "$limit" ]; do
        id=$(boot_id)
        if [ -n "$id" ] && [ "$id" != "$old" ]; then echo "$id"; return 0; fi
        sleep 10
    done
    return 1
}

state() { remote "rauc status 2>/dev/null | sed -n 's/^Booted from: .*(\(.\))/\1/p'"; }

wait_settled() { # slot-wanted limit-seconds
    local want="$1" limit="$2" start=$SECONDS
    while [ $((SECONDS - start)) -lt "$limit" ]; do
        if [ "$(state)" = "$want" ] && [ "$(remote systemctl is-active rauc-mark-good)" = active ]; then
            return 0
        fi
        sleep 15
    done
    return 1
}

install_and_reboot() { # bundle-name [may-not-return]
    log "copying $1 to the Pi..."
    "${SCP[@]}" "$WORK/$1/image.raucb" "$PI_HOST:/data/ota/test.raucb" || { log "copy failed"; return 1; }
    log "installing $1..."
    remote "rauc install /data/ota/test.raucb >/tmp/install.log 2>&1; echo \$?; rm -f /data/ota/test.raucb" | tail -1 > "$WORK/rc"
    [ "$(cat "$WORK/rc")" = 0 ] || { log "install failed:"; remote tail -5 /tmp/install.log; return 1; }
    local id; id=$(boot_id)
    remote "nohup sh -c 'sleep 2; systemctl reboot' >/dev/null 2>&1 &"
    # A broken slot may reboot before it is ever reachable. Its result is checked
    # once the Pi settles.
    if ! wait_for_reboot "$id" 420 >/dev/null; then
        [ "${2:-}" = may-not-return ] && return 0
        log "Pi did not come back"; return 1
    fi
}

summary() {
    remote 'echo "  version=$(cat /etc/kds-version) slot=$(rauc status 2>/dev/null | sed -n "s/^Booted from: //p") state=$(systemctl is-system-running)"; fw_printenv 2>/dev/null | grep -E "^BOOT_" | tr "\n" " "; echo'
}

[ -f "$ROOTFS" ] || { echo "error: missing $ROOTFS" >&2; exit 2; }
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
cleanup; mkdir -p "$WORK"

log "=== Setup"
"$REPO/ci/scripts/gen-signing-ca.sh" --out-dir "$WORK/id" >/dev/null || exit 1
make_bundle good "$GOOD" none || exit 1
make_bundle bad-api v0.3.0-bad-api bad-api || exit 1
make_bundle bad-kernel v0.3.0-bad-kernel bad-kernel || exit 1

# The boot partition isn't updated by RAUC, so install the current boot script.
docker run --rm -v "$REPO/ci/boot:/b:ro" -v "$WORK:/o" debian:bookworm-slim bash -c \
    'apt-get update -qq; apt-get install -y -qq --no-install-recommends u-boot-tools >/dev/null 2>&1; mkimage -q -A arm -T script -C none -n "KDS A/B boot" -d /b/boot.cmd /o/boot.scr; chmod a+r /o/boot.scr'
"${SCP[@]}" "$WORK/boot.scr" "$PI_HOST:/boot/firmware/boot.scr"
"${SCP[@]}" "$WORK/id/keyring.pem" "$PI_HOST:/tmp/test-ca.pem"
remote 'cat /tmp/test-ca.pem >> /etc/rauc/keyring.pem; systemctl restart rauc; sync'
summary

log "=== Test 1: good update"
START=$(state)
OTHER=$([ "$START" = A ] && echo B || echo A)
install_and_reboot good || fail=1
wait_settled "$OTHER" 300 || log "  did not settle on slot $OTHER"
summary
check "booted the other slot" "$(state)" "$OTHER"
check "runs the new version" "$(remote cat /etc/kds-version)" "$GOOD"
check "new slot was marked good" "$(remote systemctl is-active rauc-mark-good)" active
check "services are healthy" "$(remote systemctl is-system-running)" running
check "API answers" "$(remote curl -s -m 5 -o /dev/null -w '%{http_code}' http://localhost:8080/health)" 200
check "machine-id is unique to the device" "$(remote "grep -c 1278cb00626849cb8a226e307f448a55 /etc/machine-id")" 0
GOOD_SLOT=$OTHER

for bad in bad-api bad-kernel; do
    log "=== Test: $bad update falls back"
    t0=$SECONDS
    BAD_SLOT=$([ "$GOOD_SLOT" = A ] && echo B || echo A)
    install_and_reboot "$bad" may-not-return || fail=1
    # Three failed tries, each a full boot plus the health check's wait.
    wait_settled "$GOOD_SLOT" 1800 || log "  did not settle back on slot $GOOD_SLOT"
    log "  settled after $((SECONDS - t0))s"
    summary
    check "fell back to the good slot" "$(state)" "$GOOD_SLOT"
    check "used up the bad slot's tries" "$(remote fw_printenv -n BOOT_${BAD_SLOT}_LEFT)" 0
    check "runs the good version" "$(remote cat /etc/kds-version)" "$GOOD"
    check "services are healthy" "$(remote systemctl is-system-running)" running
done

if [ "$fail" -ne 0 ]; then log "A/B tests FAILED"; exit 1; fi
log "All A/B tests passed."
