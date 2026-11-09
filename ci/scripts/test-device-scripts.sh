#!/bin/bash
# test-device-scripts.sh — tests for update-boot.sh and kds-health-check.sh (DEC-029)
set -uo pipefail
DEV=$(cd "$(dirname "$0")/../device" && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fail=0
check() { if eval "$2" >/dev/null 2>&1; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi; }

echo "update-boot.sh"
FILES="bootcode.bin start.elf fixup.dat start_cd.elf fixup_cd.dat u-boot.bin bcm2708-rpi-zero-w.dtb overlays/iqaudio-codec.dtbo config.txt boot.scr kds-boot.version"
fresh() { # dir version
    rm -rf "$1"; mkdir -p "$1/overlays"
    for f in $FILES; do echo "$f $2" > "$1/$f"; done
    printf '\x27\x05\x19\x56 script %s' "$2" > "$1/boot.scr"
    echo "$2" > "$1/kds-boot.version"
}
fresh "$TMP/new" v2; fresh "$TMP/boot" v1; rm -rf "$TMP/boot/overlays"
mkdir -p "$TMP/boot/kds"; echo key > "$TMP/boot/kds/authorized_keys"; echo env > "$TMP/boot/kds-ab.env"
out=$("$DEV/update-boot.sh" "$TMP/new" "$TMP/boot")
check "copies every changed file" "for f in $FILES; do cmp $TMP/new/\$f $TMP/boot/\$f || exit 1; done"
check "writes the boot script last, before the version" "echo \"\$out\" | grep '^update-boot: updated ' | tail -2 | head -1 | grep -q boot.scr"
check "records the new version" "grep -qx v2 $TMP/boot/kds-boot.version"
check "leaves device settings alone" "grep -qx key $TMP/boot/kds/authorized_keys && grep -qx env $TMP/boot/kds-ab.env"
check "leaves no temporary files" "! find $TMP/boot -name '*.new' | grep -q ."
check "creates the overlays folder on an older boot partition" "[ -f $TMP/boot/overlays/iqaudio-codec.dtbo ]"
out=$("$DEV/update-boot.sh" "$TMP/new" "$TMP/boot")
check "changes nothing when files match" "echo \"\$out\" | grep -q '0 file(s) updated'"
fresh "$TMP/bad" v3; echo "not a script" > "$TMP/bad/boot.scr"
check "refuses an invalid boot script" "! $DEV/update-boot.sh $TMP/bad $TMP/boot"
check "leaves the boot partition unchanged after refusing" "grep -qx v2 $TMP/boot/kds-boot.version && grep -q 'v2' $TMP/boot/config.txt"
fresh "$TMP/short" v4; rm "$TMP/short/u-boot.bin"
check "refuses a slot with a missing file" "! $DEV/update-boot.sh $TMP/short $TMP/boot"

echo "kds-health-check.sh"
STUB="$TMP/stub"; mkdir -p "$STUB" "$TMP/net/wlan0"
# Stubs: each command's behaviour comes from a file the test writes.
for cmd in curl systemctl ip iw; do
    printf '#!/bin/bash\ncase "%s" in\n curl) [ -f %s/api_up ] ;;\n systemctl) [ -f %s/wpa_up ] ;;\n ip) [ -f %s/has_ip ] && echo "    inet 192.168.1.9/24" ;;\n iw) [ -f %s/ssid_visible ] && echo "	SSID: HomeNet" ;;\nesac\n' "$cmd" "$STUB" "$STUB" "$STUB" "$STUB" > "$STUB/$cmd"
    chmod +x "$STUB/$cmd"
done
printf 'network={\n\tssid="HomeNet"\n\tpsk=abc\n}\n' > "$TMP/wifi.conf"
health() { PATH="$STUB:$PATH" KDS_HEALTH_TRIES=2 KDS_HEALTH_DELAY=0 KDS_HEALTH_SYS_NET="$TMP/net" KDS_HEALTH_WIFI_CONF="$1" "$DEV/kds-health-check.sh"; }
state() { rm -f "$STUB"/{api_up,wpa_up,has_ip,ssid_visible}; for s in "$@"; do touch "$STUB/$s"; done; }

state; check "fails when the API doesn't answer" "! health $TMP/wifi.conf"
state api_up; check "passes with no Wi-Fi configured" "health $TMP/none.conf"
state api_up; check "fails when wpa_supplicant isn't running" "! health $TMP/wifi.conf"
state api_up wpa_up has_ip; check "passes with an IPv4 address" "health $TMP/wifi.conf"
state api_up wpa_up ssid_visible; check "fails when the network is visible but unjoined" "! health $TMP/wifi.conf"
state api_up wpa_up; check "passes when the network isn't visible (router off)" "health $TMP/wifi.conf"
rmdir "$TMP/net/wlan0"; state api_up wpa_up has_ip
check "fails when wlan0 is missing" "! health $TMP/wifi.conf"

[ "$fail" -eq 0 ] && echo "All device script tests passed." || echo "Device script tests FAILED"
exit "$fail"
