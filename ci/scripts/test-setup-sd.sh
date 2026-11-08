#!/bin/bash
# test-setup-sd.sh — tests for setup-sd.sh against a fake boot partition
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fail=0
check() { if eval "$2" >/dev/null 2>&1; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi; }

BOOT=$TMP/boot; mkdir -p "$BOOT"; touch "$BOOT/boot.scr" "$BOOT/u-boot.bin"
ssh-keygen -q -t ed25519 -N '' -f "$TMP/id" -C test-key
run() { KDS_WIFI_PSK="$1" "$HERE/setup-sd.sh" --boot "$BOOT" --key "$TMP/id.pub" --ssid "$2" --country "$3"; }

run "correct horse" "HomeNet" dk >/dev/null
check "installs the public key" "cmp $TMP/id.pub $BOOT/kds/authorized_keys"
check "writes the country in upper case" "grep -qx 'country=DK' $BOOT/kds/wifi.conf"
check "writes the network name" "grep -q 'ssid=\"HomeNet\"' $BOOT/kds/wifi.conf"
check "keeps the control interface" "grep -qx 'ctrl_interface=DIR=/run/wpa_supplicant' $BOOT/kds/wifi.conf"
if command -v wpa_passphrase >/dev/null; then
    check "stores the password hashed" "! grep -q 'correct horse' $BOOT/kds/wifi.conf && grep -qE 'psk=[0-9a-f]{64}' $BOOT/kds/wifi.conf"
else
    check "stores the password for wpa_supplicant" "grep -q 'psk=\"correct horse\"' $BOOT/kds/wifi.conf"
fi
check "rejects a private key" "! KDS_WIFI_PSK=12345678 $HERE/setup-sd.sh --boot $BOOT --key $TMP/id --ssid x --country DK"
check "rejects a short password" "! run short HomeNet DK"
check "rejects a bad country code" "! run 'correct horse' HomeNet Denmark"
mkdir -p "$TMP/other"
check "refuses a folder that isn't a speaker boot partition" "! KDS_WIFI_PSK=12345678 $HERE/setup-sd.sh --boot $TMP/other --key $TMP/id.pub --ssid x --country DK"
check "writes nothing to a refused folder" "[ ! -e $TMP/other/kds ]"

[ "$fail" -eq 0 ] && echo "All setup-sd tests passed." || echo "setup-sd tests FAILED"
exit "$fail"
