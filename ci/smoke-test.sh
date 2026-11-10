#!/bin/bash
# Smoke test script — Kids Desktop Speaker CI
# ===========================================
# Verifies the built image has all required components installed.
# Runs inside the Docker container via QEMU user-mode emulation.
#
# Note: We check for file presence and command availability, not service
# runtime state. systemd cannot run as PID 1 under QEMU user-mode emulation.
# Service start-up testing requires real hardware or QEMU system emulation.
#
# Exit codes:
#   0 — All tests passed
#   1 — One or more tests failed

set -euo pipefail

PASS=0
FAIL=0

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

ok() {
    echo "  [PASS] $1"
    PASS=$((PASS + 1))
}

fail() {
    echo "  [FAIL] $1"
    FAIL=$((FAIL + 1))
}

require_cmd() {
    if command -v "$1" &>/dev/null; then
        ok "command available: $1"
    else
        fail "command missing: $1"
    fi
}

require_file() {
    if [ -f "$1" ]; then
        ok "file exists: $1"
    else
        fail "file missing: $1"
    fi
}

require_executable() {
    if [ -x "$1" ]; then
        ok "executable: $1"
    else
        fail "not executable or missing: $1"
    fi
}

# ---------------------------------------------------------------------------
# Tests
# ---------------------------------------------------------------------------

echo "========================================"
echo "Kids Desktop Speaker — Smoke Tests"
echo "========================================"

echo ""
echo "--- Base System ---"
require_cmd uname
require_cmd apt-get
require_cmd systemctl
ok "architecture: $(uname -m)"

echo ""
echo "--- RAUC ---"
require_cmd rauc
require_file /etc/rauc/system.conf
require_file /etc/rauc/keyring.pem
require_file /etc/fw_env.config
require_file /etc/asound.conf
require_file /etc/udev/rules.d/90-kds-hat.rules
require_file /etc/systemd/system/kds-hat-init.service
require_file /usr/lib/kds-boot/overlays/iqaudio-codec.dtbo
require_file /etc/udev/rules.d/80-kds-wifi-powersave.rules
require_cmd iw
require_file /usr/lib/systemd/system/rauc.service
require_file /usr/lib/systemd/system/systemd-timesyncd.service
require_file /usr/lib/kds-boot/u-boot.bin
require_file /usr/lib/kds-boot/start_cd.elf
require_file /boot/vmlinuz-kds
require_file /usr/lib/kds-boot/start.elf
require_file /etc/systemd/system/kds-device-config.service
ok "RAUC system.conf installed"

echo ""
echo "--- Audio (ALSA) ---"
require_cmd aplay
require_cmd amixer
ok "ALSA utilities installed"

echo ""
echo "--- Announcements (TTS) ---"
require_cmd espeak-ng
for voice in da en; do
    if espeak-ng -v "$voice" -w /tmp/tts-smoke.wav -- "test" 2>/dev/null && [ -s /tmp/tts-smoke.wav ]; then
        ok "espeak-ng speaks $voice"
    else
        fail "espeak-ng cannot speak $voice"
    fi
done
rm -f /tmp/tts-smoke.wav
if grep -q "type dmix" /etc/asound.conf && grep -q 'slave.pcm "hat_mix"' /etc/asound.conf; then
    ok "speaker plays through the shared dmix output"
else
    fail "asound.conf has no shared dmix output"
fi

echo ""
echo "--- AirPlay (shairport-sync, started by audio-player) ---"
require_executable /usr/bin/shairport-sync
require_file /etc/shairport-sync.conf
if grep -q 'output_device = "speaker"' /etc/shairport-sync.conf; then
    ok "shairport-sync plays through the speaker device"
else
    fail "shairport-sync.conf does not use the speaker device"
fi
# audio-player owns the process; the package's own service would race it for the port.
if [ -e /etc/systemd/system/multi-user.target.wants/shairport-sync.service ]; then
    fail "the package's shairport-sync.service is enabled"
else
    ok "the package's shairport-sync.service is disabled"
fi

echo ""
echo "--- Bluetooth ---"
require_cmd bluetoothctl
ok "BlueZ installed"

echo ""
echo "--- Platform Service Binaries ---"
require_executable /usr/local/bin/api-server
require_executable /usr/local/bin/audio-player
require_executable /usr/local/bin/ota-updater
require_executable /usr/local/bin/bluetooth-service
ok "all service binaries present and executable"

echo ""
echo "--- systemd Unit Files ---"
require_file /etc/systemd/system/api-server.service
require_file /etc/systemd/system/audio-player.service
require_file /etc/systemd/system/ota-updater.service
require_file /etc/systemd/system/bluetooth-service.service
require_file /etc/systemd/system/rauc-mark-good.service
ok "all systemd units installed"

echo ""
echo "--- Bundle Output (if OUTPUT_DIR is set) ---"
if [ -n "${OUTPUT_DIR:-}" ]; then
    require_file "${OUTPUT_DIR}/image.raucb"
else
    ok "OUTPUT_DIR not set; skipping bundle output test"
fi

echo ""
echo "========================================"
echo "Results: ${PASS} passed, ${FAIL} failed"
echo "========================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

echo "All smoke tests passed."
exit 0
