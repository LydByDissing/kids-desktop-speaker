# hw-lib.sh — shared helpers for the real-hardware tests. Source it; don't run it.
# Expects REPO, PI_HOST and WORK to be set.

IMAGE=kids-desktop-speaker:latest
PLATFORM=linux/arm/v6
ROOTFS="${HW_ROOTFS:-$REPO/ci/output/rootfs.tar}"
# HW_SSH_PORT and HW_SSH_KEY let the same tests drive the A/B VM (ci/vm/ab-vm.sh).
SSH_EXTRA=()
[ -n "${HW_SSH_KEY:-}" ] && SSH_EXTRA+=(-i "$HW_SSH_KEY" -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR)
# Host keys live on the data partition, so they stay the same across slots.
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=8 -o StrictHostKeyChecking=accept-new "${SSH_EXTRA[@]}" -p "${HW_SSH_PORT:-22}" "$PI_HOST")
SCP=(scp -q -o BatchMode=yes -o ConnectTimeout=20 -o StrictHostKeyChecking=accept-new "${SSH_EXTRA[@]}" -P "${HW_SSH_PORT:-22}")

fail=0
log() { echo "[$(date +%H:%M:%S)] $*"; }
check() { if [ "$2" = "$3" ]; then log "  [PASS] $1"; else log "  [FAIL] $1 (got '$2', want '$3')"; fail=1; fi; }
remote() { "${SSH[@]}" "$@" 2>/dev/null; }
hw_cleanup() {
    docker run --rm -v "$REPO/ci/output:/o" --entrypoint /bin/rm debian:bookworm-slim -rf "/o/$(basename "$WORK")" >/dev/null 2>&1
}

# A throwaway signing identity, trusted only by the device under test.
make_test_identity() {
    "$REPO/ci/scripts/gen-signing-ca.sh" --out-dir "$WORK/id" >/dev/null
}

trust_test_identity() {
    "${SCP[@]}" "$WORK/id/keyring.pem" "$PI_HOST:/tmp/test-ca.pem"
    remote 'grep -qF "$(sed -n 2p /tmp/test-ca.pem)" /etc/rauc/keyring.pem || cat /tmp/test-ca.pem >> /etc/rauc/keyring.pem; systemctl restart rauc; sync'
}

make_bundle() { # name version mutation(none|bad-api|bad-kernel|bad-wifi)
    local dir="$WORK/$1"; mkdir -p "$dir"
    python3 - "$ROOTFS" "$dir/rootfs.tar" "$2" "$3" "$WORK/id/keyring.pem" <<'PY'
import io, sys, tarfile
src_path, dst_path, version, mutation, test_ca = sys.argv[1:6]
extra_ca = open(test_ca, "rb").read()
with tarfile.open(src_path) as src, tarfile.open(dst_path, "w", format=tarfile.GNU_FORMAT) as dst:
    for m in src:
        name = m.name.removeprefix("./")
        data = src.extractfile(m).read() if m.isreg() else None
        if name in ("etc/kds-version", "usr/lib/kds-boot/kds-boot.version"):
            data = version.encode() + b"\n"
        elif name == "etc/rauc/keyring.pem":
            data += extra_ca
        elif mutation == "bad-api" and name == "usr/local/bin/api-server":
            data = b"#!/bin/sh\nexit 1\n"
        elif mutation == "bad-kernel" and name == "boot/vmlinuz-kds":
            data = data[:4096]
        # bad-wifi: the update stops starting wpa_supplicant, so Wi-Fi never comes up.
        if mutation == "bad-wifi" and name == "etc/systemd/system/multi-user.target.wants/wpa_supplicant@wlan0.service":
            continue
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
    docker run --rm -v "$dir:/d" --entrypoint /bin/sh debian:bookworm-slim -c 'rm -f /d/rootfs.tar; chmod a+r /d/image.raucb'
    log "built bundle $1: $(du -h "$dir/image.raucb" | cut -f1)"
}

boot_id() { remote cat /proc/sys/kernel/random/boot_id; }
slot() { remote "rauc status 2>/dev/null | sed -n 's/^Booted from: .*(\(.\))/\1/p'"; }

wait_for_reboot() { # old-boot-id limit-seconds
    local start=$SECONDS id
    while [ $((SECONDS - start)) -lt "$2" ]; do
        id=$(boot_id)
        if [ -n "$id" ] && [ "$id" != "$1" ]; then return 0; fi
        sleep 10
    done
    return 1
}

wait_settled() { # slot limit-seconds
    local start=$SECONDS
    while [ $((SECONDS - start)) -lt "$2" ]; do
        if [ "$(slot)" = "$1" ] && [ "$(remote systemctl is-active rauc-mark-good)" = active ]; then
            return 0
        fi
        sleep 15
    done
    return 1
}

reboot_pi() { remote "nohup sh -c 'sleep 2; systemctl reboot' >/dev/null 2>&1 &"; }

summary() {
    remote 'echo "  version=$(cat /etc/kds-version) slot=$(rauc status 2>/dev/null | sed -n "s/^Booted from: //p") state=$(systemctl is-system-running)"; fw_printenv 2>/dev/null | grep -E "^BOOT_" | tr "\n" " "; echo'
}

# Hardware tests play sound, so they mute the speaker (the HAT's line output)
# for their duration; set KDS_HW_AUDIBLE=1 to listen. Checks read the stream
# before the HAT (ALSA state, CamillaDSP's meters), so muting changes no result.
SPEAKER_MUTED_BY_TEST=0
speaker_mute() {
    [ "${KDS_HW_AUDIBLE:-0}" = 1 ] && return 0
    remote "amixer -q -c IQaudIOCODEC sset Lineout off" || return 0
    SPEAKER_MUTED_BY_TEST=1
    log "speaker muted for the test (KDS_HW_AUDIBLE=1 to listen)"
}
speaker_unmute() {
    [ "$SPEAKER_MUTED_BY_TEST" = 1 ] || return 0
    remote "amixer -q -c IQaudIOCODEC sset Lineout on" && log "speaker unmuted"
    SPEAKER_MUTED_BY_TEST=0
}
