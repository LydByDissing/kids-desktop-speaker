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
# Voice data only: synthesis under QEMU user emulation depends on the host's QEMU
# (it fails on the CI runner). make hw-announce-test proves speech on a real Pi.
for voice in da en; do
    require_file "/usr/lib/arm-linux-gnueabihf/espeak-ng-data/${voice}_dict"
done
if grep -q "type dmix" /etc/asound.conf && grep -q 'slave.pcm "hat_mix"' /etc/asound.conf; then
    ok "speaker plays through the shared dmix output"
else
    fail "asound.conf has no shared dmix output"
fi

echo ""
echo "--- Spotify Connect (librespot, started by audio-player) ---"
require_executable /usr/bin/librespot
require_file /usr/share/doc/librespot/copyright
if /usr/bin/librespot --version 2>/dev/null | grep -q "^librespot 0\.8\."; then
    ok "librespot 0.8 runs"
else
    fail "librespot does not run"
fi

echo ""
echo "--- Bluetooth audio (BlueALSA, DEC-035) ---"
require_executable /usr/bin/bluealsa
require_executable /usr/bin/bluealsa-aplay
if grep -q "^Class = 0x240414" /etc/bluetooth/main.conf; then
    ok "the adapter presents itself as a loudspeaker"
else
    fail "main.conf does not set the loudspeaker class"
fi
if grep -q "ExecStart=/usr/bin/bluealsa -p a2dp-sink$" /etc/systemd/system/bluealsa.service.d/sink-only.conf; then
    ok "BlueALSA runs as an A2DP sink only"
else
    fail "BlueALSA is not sink-only"
fi
if grep -q "BindPaths=-/data/bluetooth:/var/lib/bluetooth" /etc/systemd/system/bluetooth.service.d/data.conf; then
    ok "pairings are kept on /data"
else
    fail "pairings are not kept on /data"
fi
# audio-player owns bluealsa-aplay, so announcements can pause it.
if ls /etc/systemd/system/*.wants/bluealsa-aplay.service >/dev/null 2>&1; then
    fail "the package's bluealsa-aplay.service is enabled"
else
    ok "the package's bluealsa-aplay.service is disabled"
fi
if [ -e /etc/systemd/system/bluetooth.target.wants/bluealsa.service ]; then
    ok "bluealsa starts with Bluetooth"
else
    fail "bluealsa is not enabled"
fi

echo ""
echo "--- Internet radio (MPD, RAD_REQ_001) ---"
require_executable /usr/bin/mpd
if grep -q '^bind_to_address "127.0.0.1"' /etc/mpd.conf && grep -q 'device      "speaker"' /etc/mpd.conf; then
    ok "MPD listens on localhost and plays to the speaker"
else
    fail "mpd.conf does not bind localhost or play to the speaker"
fi
if grep -q '^state_file' /etc/mpd.conf; then
    fail "MPD keeps a state file (radio would restart after a reboot)"
else
    ok "MPD keeps no state file"
fi
if [ -e /etc/systemd/system/multi-user.target.wants/mpd.service ]; then
    ok "mpd.service starts at boot"
else
    fail "mpd.service is not enabled"
fi
if ls /etc/systemd/system/*.wants/mpd.socket >/dev/null 2>&1; then
    fail "mpd.socket is enabled (it listens on every interface)"
else
    ok "mpd.socket is disabled"
fi

echo ""
echo "--- DSP (CamillaDSP, DEC-036) ---"
require_executable /usr/bin/camilladsp
require_file /usr/share/doc/camilladsp/copyright
if /usr/bin/camilladsp --check /etc/camilladsp/config.yml 2>&1 | grep -q "Config is valid"; then
    ok "the CamillaDSP config is valid"
else
    fail "the CamillaDSP config is invalid"
fi
if grep -q 'pcm "hw:Loopback,0,0"' /etc/asound.conf && grep -q "^snd-aloop" /etc/modules-load.d/snd-aloop.conf; then
    ok "the sources play into the loopback CamillaDSP reads"
else
    fail "speaker does not feed the loopback"
fi
if grep -q -- "-a 127.0.0.1" /etc/systemd/system/camilladsp.service; then
    ok "CamillaDSP's websocket listens on localhost only"
else
    fail "CamillaDSP's websocket is not limited to localhost"
fi
if grep -q -- "-s /data/camilladsp/statefile.yml" /etc/systemd/system/camilladsp.service; then
    ok "CamillaDSP keeps the volume in a statefile on /data"
else
    fail "CamillaDSP has no statefile"
fi
if grep -q "^StartLimitIntervalSec=0" /etc/systemd/system/camilladsp.service; then
    ok "CamillaDSP keeps retrying until the HAT is there"
else
    fail "CamillaDSP may give up before the HAT appears"
fi
if grep -q "camilladsp.service" /etc/udev/rules.d/90-kds-hat.rules; then
    ok "CamillaDSP starts when the HAT appears"
else
    fail "the HAT rule does not start CamillaDSP"
fi

echo ""
echo "--- One source at a time (DEC-039) ---"
require_executable /usr/local/bin/kds-spotify-event
if grep -q 'run_this_before_play_begins = "/usr/local/bin/audio-player notify airplay active"' /etc/shairport-sync.conf; then
    ok "AirPlay reports when it plays"
else
    fail "shairport-sync does not report when it plays"
fi

echo ""
echo "--- Wi-Fi onboarding (DEC-044) ---"
require_executable /usr/local/bin/kds-wifi-setup
require_executable /usr/local/bin/kds-setup-ap.sh
require_cmd hostapd
require_cmd dnsmasq
if [ -e /etc/systemd/system/multi-user.target.wants/wifi-setup.service ]; then
    ok "wifi-setup starts at boot"
else
    fail "wifi-setup.service is not enabled"
fi
if ls /etc/systemd/system/*.wants/hostapd.service /etc/systemd/system/*.wants/dnsmasq.service >/dev/null 2>&1; then
    fail "hostapd or dnsmasq start by themselves (only the setup AP should run them)"
else
    ok "hostapd and dnsmasq only run for the setup access point"
fi

echo ""
echo "--- Discovery (DEC-043) ---"
if grep -q "<type>_kidsspeaker._tcp</type>" /etc/avahi/services/kids-speaker.service; then
    ok "the API is advertised over mDNS"
else
    fail "no mDNS service for the API"
fi

echo ""
echo "--- Local time (DEC-041) ---"
if [ "$(readlink /etc/localtime)" = /usr/share/zoneinfo/Europe/Copenhagen ]; then
    ok "the time zone is Europe/Copenhagen"
else
    fail "the time zone is $(readlink /etc/localtime)"
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
echo "--- Trimmed for the bundle size (KDS-sgt.8) ---"
ls /var/cache/apt/archives/*.deb >/dev/null 2>&1 && fail "the apt cache is still in the image"
[ -z "$(ls -A /usr/share/man 2>/dev/null)" ] || fail "man pages are still in the image"
[ -d /usr/share/locale/fr ] && fail "translations other than Danish and English are still in the image"
copyrights=$(ls /usr/share/doc/*/copyright 2>/dev/null | wc -l)
[ "$copyrights" -gt 100 ] || fail "copyright files are missing ($copyrights)"
ok "apt cache, man pages and other languages removed; $copyrights copyright files kept"

echo ""
echo "--- Web UI ---"
require_file /usr/share/kds/web/index.html
ls /usr/share/kds/web/assets/*.js >/dev/null 2>&1 || fail "the Web UI has no scripts"
ok "Web UI installed"

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
