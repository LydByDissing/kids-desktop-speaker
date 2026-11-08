#!/bin/bash
# kds-health-check.sh — decide whether this boot is healthy (OTA_REQ_009, DEC-029)
# ================================================================================
# Run by rauc-mark-good before marking the slot good. Exit 0 means healthy; any
# other exit makes the boot count as failed, so the slot's tries run out and the
# device falls back to the other slot.
#
# Healthy means the API answers and, when Wi-Fi is configured, wlan0 gets an IPv4
# address. If it doesn't, a configured network that's visible means this update
# broke the connection; one that isn't visible means the router is off, which
# isn't this update's fault and must not cause reboot loops.

set -uo pipefail

API_URL="${KDS_HEALTH_API_URL:-http://localhost:8080/health}"
WIFI_CONF="${KDS_HEALTH_WIFI_CONF:-/etc/wpa_supplicant/wpa_supplicant-wlan0.conf}"
IFACE="${KDS_HEALTH_IFACE:-wlan0}"
SYS_NET="${KDS_HEALTH_SYS_NET:-/sys/class/net}"
TRIES="${KDS_HEALTH_TRIES:-60}"
DELAY="${KDS_HEALTH_DELAY:-2}"

log() { echo "health: $*"; }
healthy() { log "$*"; exit 0; }
unhealthy() { log "UNHEALTHY: $*"; exit 1; }

api_ok=no
for _ in $(seq 1 "$TRIES"); do
    curl -sf -m 5 "$API_URL" >/dev/null && { api_ok=yes; break; }
    sleep "$DELAY"
done
[ "$api_ok" = yes ] || unhealthy "the API did not answer"

[ -f "$WIFI_CONF" ] || healthy "API answers; no Wi-Fi configured"
[ -d "$SYS_NET/$IFACE" ] || unhealthy "Wi-Fi is configured but $IFACE is missing"
systemctl is-active --quiet "wpa_supplicant@$IFACE" || unhealthy "wpa_supplicant@$IFACE is not running"

for _ in $(seq 1 "$TRIES"); do
    ip -4 addr show dev "$IFACE" 2>/dev/null | grep -q 'inet ' && healthy "API answers; $IFACE has an IPv4 address"
    sleep "$DELAY"
done

ssid=$(sed -n 's/^[[:space:]]*ssid="\(.*\)"[[:space:]]*$/\1/p' "$WIFI_CONF" | head -1)
if [ -n "$ssid" ] && iw dev "$IFACE" scan 2>/dev/null | grep -qF "SSID: $ssid"; then
    unhealthy "network \"$ssid\" is visible but $IFACE got no address"
fi
healthy "API answers; network \"$ssid\" not visible, assuming the router is off"
