#!/bin/bash
# hw-bluetooth-test.sh — Bluetooth audio input on a real Pi (BT_REQ_001-003, DEC-035)
# ==================================================================================
# Usage: PI_HOST=root@host hw-bluetooth-test.sh
#
# Runs against the image installed on the Pi and needs no phone: checks that the
# adapter presents itself as an A2DP loudspeaker, that the Bluetooth Service's
# agent is registered, that POST /api/bluetooth/pair opens a pairing window,
# that audio-player runs bluealsa-aplay and stops it during an announcement,
# and that pairings are stored on /data. Pair a phone while the window is open
# and play to it to hear it end to end.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

api() { # method path -> HTTP status; body in /tmp/kds-api.out on the Pi
    remote "curl -s -o /tmp/kds-api.out -w '%{http_code}' -X $1 http://127.0.0.1:8080$2"
}

log "=== Services"
for unit in bluetooth bluealsa bluetooth-service audio-player; do
    check "$unit is running" "$(remote systemctl is-active $unit)" active
done
check "the package's bluealsa-aplay service is not running" "$(remote systemctl is-active bluealsa-aplay)" inactive

log "=== The adapter presents itself as a speaker"
show=$(remote bluetoothctl show)
check "device class is Audio/Video loudspeaker" "$(echo "$show" | grep -oE 'Class: 0x[0-9a-f]*0414' | wc -l)" 1
check "offers Audio Sink" "$(echo "$show" | grep -c 'UUID: Audio Sink')" 1
check "does not offer Audio Source" "$(echo "$show" | grep -c 'UUID: Audio Source')" 0
check "the Bluetooth Service's agent is registered" \
    "$(remote "journalctl -u bluetooth-service -b -o cat --no-pager" | grep -c 'pairing agent registered')" 1

log "=== Pairing window"
check "POST /api/bluetooth/pair is accepted" "$(api POST /api/bluetooth/pair)" 200
sleep 1
show=$(remote bluetoothctl show)
check "the adapter is discoverable" "$(echo "$show" | grep -c 'Discoverable: yes')" 1
check "the adapter is pairable" "$(echo "$show" | grep -c 'Pairable: yes')" 1
check "GET /api/bluetooth/status answers" "$(api GET /api/bluetooth/status)" 200
check "status reports the open window" "$(remote cat /tmp/kds-api.out | grep -c '"open":true')" 1

log "=== Bluetooth audio is a source managed by audio-player"
ap=$(remote pgrep -x audio-player | head -1)
ba=$(remote pgrep -x bluealsa-aplay | head -1)
check "bluealsa-aplay is running" "$([ -n "$ba" ] && echo yes)" yes
check "audio-player started it" "$(remote "awk '{print \$4}' /proc/$ba/stat")" "$ap"
check "it plays to the speaker device" "$(remote "tr '\\0' ' ' < /proc/$ba/cmdline" | grep -c -- '--pcm=speaker')" 1
since=$(remote date +%s)
remote "for i in \$(seq 40); do awk '{print \$3}' /proc/$ba/stat; sleep 0.25; done > /tmp/kds-bt-watch" &
watch=$!
sleep 0.5
remote "curl -s -o /dev/null -H 'content-type: application/json' -d '{\"text\": \"Bluetooth\", \"lang\": \"en\"}' http://127.0.0.1:8080/api/tts"
wait $watch
check "an announcement stops it" "$(remote cat /tmp/kds-bt-watch | grep -c T | awk '{print ($1>0)?"yes":"no"}')" yes
check "it runs again afterwards" "$(remote "awk '{print \$3}' /proc/$ba/stat")" S

log "=== Pairings are kept on /data"
check "/data/bluetooth exists, mode 700" "$(remote stat -c %a /data/bluetooth)" 700
bt=$(remote pgrep -x bluetoothd | head -1)
check "bluetoothd sees /data/bluetooth as its state directory" \
    "$(remote "touch /data/bluetooth/.kds-probe; nsenter -t $bt -m test -e /var/lib/bluetooth/.kds-probe && echo yes; rm -f /data/bluetooth/.kds-probe")" yes
paired=$(remote "bluetoothctl devices Paired" | wc -l)
log "  paired devices: $paired"
if [ "$paired" -gt 0 ]; then
    check "every paired device is stored on /data" \
        "$(remote "for d in \$(bluetoothctl devices Paired | cut -d' ' -f2); do ls -d /data/bluetooth/*/\$d >/dev/null 2>&1 || echo missing; done" | grep -c missing)" 0
fi

if [ "$fail" = 0 ]; then log "All Bluetooth tests passed."; else log "Some Bluetooth tests FAILED."; fi
exit "$fail"
