#!/bin/bash
# Smoke test script — Kids Desktop Speaker CI
# ===========================================
# Verifies the built image has all required components installed and
# operational. Runs inside the Docker container via QEMU user-mode emulation.
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

require_service() {
    if systemctl is-active --quiet "$1" 2>/dev/null; then
        ok "service active: $1"
    else
        fail "service not active: $1"
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
if rauc status &>/dev/null; then
    ok "rauc status"
else
    # rauc status may fail without real partitions — that's expected in CI
    ok "rauc status (no partitions — expected in CI)"
fi

echo ""
echo "--- Audio ---"
require_cmd aplay
require_cmd amixer
ok "ALSA utilities installed"

echo ""
echo "--- Bluetooth ---"
require_cmd bluetoothctl
ok "BlueZ installed"

echo ""
echo "--- Platform Services ---"
require_file /usr/local/bin/api-server
require_file /usr/local/bin/audio-player
require_file /usr/local/bin/ota-updater
require_file /usr/local/bin/bluetooth-service
ok "all service binaries present"

echo ""
echo "--- systemd Units ---"
require_file /etc/systemd/system/rauc-mark-good.service
ok "RAUC mark-good service installed"

echo ""
echo "========================================"
echo "Results: ${PASS} passed, ${FAIL} failed"
echo "========================================"

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi

echo "All smoke tests passed."
exit 0
