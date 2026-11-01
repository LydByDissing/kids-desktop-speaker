#!/bin/bash
# test-armv6.sh — prove the image runs on the Pi Zero W's CPU (DEC-024)
# =====================================================================
# Runs the image's programs under QEMU pinned to ARM1176, the Pi Zero W's CPU.
# An ARMv7 instruction crashes with SIGILL. A control proves the pin is enforced.
# Requires the image (make build). This is emulation, not real hardware.

set -uo pipefail

IMAGE=kids-desktop-speaker:latest
fail=0
check() { if [ "$2" = "$3" ]; then echo "  [PASS] $1"; else echo "  [FAIL] $1 (got $2, want $3)"; fail=1; fi; }

# --init keeps the program under test from being PID 1, where a crash can hang
# instead of exiting. The timeout is a backstop so CI can't hang here.
in_image() {
    timeout 120 docker run --rm --init --platform linux/arm/v6 -e QEMU_CPU=arm1176 \
        --entrypoint /bin/bash "$IMAGE" -c "$1"
}

# The control must crash, or a pass below means nothing.
timeout 120 docker run --rm --init --platform linux/arm/v7 -e QEMU_CPU=arm1176 \
    arm32v7/debian:bookworm-slim /bin/true >/dev/null 2>&1
check "control: ARMv7 Debian crashes on ARM1176" "$?" 132

in_image '. /etc/os-release; echo "$ID"' | grep -q '^raspbian$'
check "image is Raspbian" "$?" 0

for cmd in "/bin/bash --version" "/usr/bin/rauc --version" "/usr/sbin/bluetoothd --version" \
           "/bin/systemctl --version" "/usr/bin/openssl version" "/usr/sbin/hostapd -v" \
           "/usr/sbin/dnsmasq --version" "/usr/sbin/avahi-daemon --version" "/usr/bin/aplay --version"; do
    rc=$(in_image "$cmd >/dev/null 2>&1; echo \$?" 2>/dev/null | tail -1)
    if [ "$rc" = 132 ] || [ "$rc" = 139 ] || [ "$rc" = 127 ]; then
        check "$cmd" "$rc" "no crash"
    else
        check "$cmd" ok ok
    fi
done

# Services run until the timeout (124) unless they crash.
for svc in api-server audio-player bluetooth-service; do
    rc=$(in_image "timeout 8 /usr/local/bin/$svc >/dev/null 2>&1; echo \$?" 2>/dev/null | tail -1)
    check "$svc keeps running" "$rc" 124
done

# A real HTTPS request exercises the crypto code. The high version means nothing
# gets installed.
out=$(in_image 'echo v999.0.0 > /tmp/ver; OTA_VERSION_FILE=/tmp/ver timeout 40 /usr/local/bin/ota-updater 2>&1')
echo "$out" | grep -qE 'ota: (nothing to install|release check failed: http error: HTTP status)'
check "ota-updater completes an HTTPS request" "$?" 0
[ "$fail" -eq 0 ] || echo "$out" | tail -5

if [ "$fail" -ne 0 ]; then echo "ARMv6 tests FAILED"; exit 1; fi
echo "All ARMv6 tests passed."
