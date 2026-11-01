#!/bin/bash
# hw-test.sh — test the image's programs on a real Pi Zero W over SSH (DEC-024)
# =============================================================================
# Usage: PI_HOST=user@host hw-test.sh [--keep]
#
# Copies the image's userland and release binaries to a scratch directory on the
# Pi and runs them through the image's own dynamic loader, so no root is needed
# and the Pi's OS and partitions are untouched. The scratch directory is removed
# afterwards. This tests the CPU and libraries, not our kernel or boot chain.
#
# Requires: make -C ci bundle (for ci/output/rootfs.tar) and release binaries in
# target/arm-unknown-linux-gnueabihf/release.

set -euo pipefail

: "${PI_HOST:?set PI_HOST=user@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
BIN="$REPO/target/arm-unknown-linux-gnueabihf/release"
ROOTFS="$REPO/ci/output/rootfs.tar"
REMOTE=kds-hw-test
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=15 "$PI_HOST")

for f in "$ROOTFS" "$BIN/api-server" "$BIN/audio-player" "$BIN/ota-updater" "$BIN/bluetooth-service"; do
    [ -e "$f" ] || { echo "error: missing $f" >&2; exit 2; }
done

cleanup() { [ "${1:-}" = "--keep" ] || "${SSH[@]}" "rm -rf ~/$REMOTE" || true; }
trap 'cleanup "${KEEP:-}"' EXIT
[ "${1:-}" = "--keep" ] && KEEP=--keep

"${SSH[@]}" "rm -rf ~/$REMOTE && mkdir -p ~/$REMOTE/image ~/$REMOTE/control ~/$REMOTE/bin"

echo "==> Copying image userland (no service binaries)..."
python3 - "$ROOTFS" <<'PY' | "${SSH[@]}" "tar -xf - -C ~/$REMOTE/image --no-same-owner --no-same-permissions 2>/dev/null || true"
import sys, tarfile
want = ('usr/lib/arm-linux-gnueabihf/', 'usr/lib/ld-linux', 'lib', 'usr/bin/', 'bin', 'usr/sbin/', 'sbin', 'usr/libexec/')
skip = ('usr/lib/arm-linux-gnueabihf/gconv/', 'usr/lib/arm-linux-gnueabihf/perl')
with tarfile.open(sys.argv[1]) as src, tarfile.open(fileobj=sys.stdout.buffer, mode='w|') as dst:
    for m in src:
        n = m.name.removeprefix('./')
        if not n.startswith(want) or n.startswith(skip) or not (m.isreg() or m.issym() or m.isdir()):
            continue
        dst.addfile(m, src.extractfile(m) if m.isreg() else None)
PY

echo "==> Copying ARMv7 control..."
docker run --rm --platform linux/arm/v7 --entrypoint /bin/tar arm32v7/debian:bookworm-slim \
    -chf - usr/bin/true usr/lib/ld-linux-armhf.so.3 usr/lib/arm-linux-gnueabihf/libc.so.6 2>/dev/null \
    | "${SSH[@]}" "tar -xf - -C ~/$REMOTE/control"

echo "==> Copying release binaries..."
tar -C "$BIN" -cf - api-server audio-player ota-updater bluetooth-service | "${SSH[@]}" "tar -xf - -C ~/$REMOTE/bin"

"${SSH[@]}" "REMOTE=$REMOTE bash -s" <<'REMOTE_SCRIPT'
set -u
cd ~/"$REMOTE"
fail=0
check() { if [ "$2" = "$3" ]; then echo "  [PASS] $1"; else echo "  [FAIL] $1 (got $2, want $3)"; fail=1; fi; }
IMG=$PWD/image
LD=$IMG/usr/lib/ld-linux-armhf.so.3
[ -e "$LD" ] || LD=$IMG/usr/lib/arm-linux-gnueabihf/ld-linux-armhf.so.3
LIBS=$IMG/usr/lib/arm-linux-gnueabihf

echo "--- Hardware"
grep -E '^Model' /proc/cpuinfo; uname -m
free -m | sed -n 2p | awk '{print "RAM MB: total " $2 ", available " $7}'

echo "--- CPU check"
CLD=$PWD/control/usr/lib/ld-linux-armhf.so.3
"$CLD" --library-path control/usr/lib/arm-linux-gnueabihf control/usr/bin/true >/dev/null 2>&1
check "control: ARMv7 Debian crashes on this CPU" "$?" 132

for cmd in "usr/bin/bash --version" "usr/bin/rauc --version" "usr/sbin/bluetoothd --version" \
           "usr/bin/systemctl --version" "usr/bin/openssl version" "usr/sbin/hostapd -v" \
           "usr/sbin/dnsmasq --version" "usr/sbin/avahi-daemon --version" "usr/bin/aplay --version"; do
    set -- $cmd
    p=$1; shift
    if [ ! -f "$IMG/$p" ]; then check "image $p" missing present; continue; fi
    "$LD" --library-path "$LIBS" "$IMG/$p" "$@" >/dev/null 2>&1
    rc=$?
    if [ "$rc" = 132 ] || [ "$rc" = 139 ] || [ "$rc" = 127 ]; then check "image $p" "$rc" "no crash"; else check "image $p" ok ok; fi
done

echo "--- Services (release builds, image libraries)"
printf '%-20s %8s %10s %8s\n' service "RSS KB" "CPU s/30s" state
for svc in api-server audio-player bluetooth-service; do
    "$LD" --library-path "$LIBS" "bin/$svc" >"$svc.log" 2>&1 &
    pid=$!
    sleep 30
    if kill -0 "$pid" 2>/dev/null; then state=running; else state=exited; fi
    rss=$(awk '/VmRSS/{print $2}' "/proc/$pid/status" 2>/dev/null || echo -)
    ticks=$(awk '{print $14+$15}' "/proc/$pid/stat" 2>/dev/null || echo 0)
    printf '%-20s %8s %10s %8s\n' "$svc" "$rss" "$(awk -v t="$ticks" 'BEGIN{printf "%.2f", t/100}')" "$state"
    if [ "$svc" = api-server ]; then
        s=$(curl -s -o /dev/null -w '%{http_code} %{time_total}s' http://localhost:8080/health)
        echo "  /health: $s"
        check "api-server answers /health" "${s%% *}" 200
    fi
    kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
    rc=$?
    [ "$rc" = 132 ] && check "$svc" "$rc" "no crash"
    check "$svc keeps running" "$state" running
    [ "$state" = running ] || tail -5 "$svc.log" | cut -c1-140
    if [ "$svc" = audio-player ]; then cut -c1-110 "$svc.log" | grep -E 'unavailable|started' | sed 's/^/  /'; fi
done

echo "--- ota-updater HTTPS"
# Wi-Fi can fail a single request, so check often and require one success within
# the window, as a device would on its next check. The high version means nothing
# gets installed.
echo v999.0.0 > ver
start=$(date +%s.%N)
OTA_VERSION_FILE=$PWD/ver OTA_CHECK_INTERVAL_SECS=15 "$LD" --library-path "$LIBS" bin/ota-updater > ota.log 2>&1 &
pid=$!
OK='ota: (nothing to install|release check failed: http error: HTTP status)'
for _ in $(seq 1 240); do
    grep -qE "$OK" ota.log && break
    sleep 0.5
done
elapsed=$(awk -v s="$start" -v e="$(date +%s.%N)" 'BEGIN{printf "%.1f", e-s}')
rss=$(awk '/VmHWM/{print $2}' "/proc/$pid/status" 2>/dev/null || echo -)
grep -E 'ota: ' ota.log | cut -c1-140
echo "  first success after ${elapsed}s, peak RSS ${rss} KB"
kill "$pid" 2>/dev/null; wait "$pid" 2>/dev/null
grep -qE "$OK" ota.log
check "ota-updater completes an HTTPS request" "$?" 0

echo "--- Bundle handling (v0.1.0-rc.1, 54 MB)"
URL=https://github.com/LydByDissing/kids-desktop-speaker/releases/download/v0.1.0-rc.1/image.raucb
curl -sL -o bundle -w '  download: %{size_download} bytes in %{time_total}s (%{speed_download} B/s)\n' "$URL"
s=$(date +%s.%N); sha256sum bundle >/dev/null
awk -v s="$s" -v e="$(date +%s.%N)" 'BEGIN{printf "  sha256: %.1fs\n", e-s}'
free -m | sed -n 2p | awk '{print "  RAM MB available afterwards: " $7}'
rm -f bundle

[ "$fail" -eq 0 ] && echo "All hardware tests passed." || { echo "Hardware tests FAILED"; exit 1; }
REMOTE_SCRIPT
