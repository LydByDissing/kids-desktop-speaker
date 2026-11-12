#!/bin/bash
# hw-radio-test.sh — internet radio on a real Pi (RAD_REQ_001, DEC-048)
# =====================================================================
# Usage: PI_HOST=root@host hw-radio-test.sh
#
# Runs against the image installed on the Pi and needs internet access. Plays
# DR P3 through the API, checks that the Audio Player's stream player plays it
# to the HAT and reports the programme, that an announcement pauses and resumes
# it, and that it stops. The speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
trap speaker_unmute EXIT
speaker_mute

api() { # method path [json] -> HTTP status; body in /tmp/kds-api.out on the Pi
    remote "curl -s -o /tmp/kds-api.out -w '%{http_code}' -X $1 -H 'content-type: application/json' ${3:+-d '$3'} http://127.0.0.1:8080$2"
}
body() { remote cat /tmp/kds-api.out; }
hat() { remote "grep -c RUNNING /proc/asound/IQaudIOCODEC/pcm0p/sub0/status 2>/dev/null"; }
# The stream player's output: aplay reading raw samples (announcements play files).
streaming() { remote "pgrep -f '[a]play -q -D speaker -t raw' | wc -l"; }

log "=== No MPD (DEC-048)"
check "MPD is not installed" "$(remote 'test -e /usr/bin/mpd && echo yes || echo no')" no
check "nothing listens on MPD's port" "$(remote "ss -ltnH 'sport = :6600'" | wc -l)" 0

log "=== Stations"
check "GET /api/radio/stations" "$(api GET /api/radio/stations)" 200
check "DR P3 is in the list" "$(body | grep -c '"id":"dr-p3"')" 1
check "a file:// station is refused" "$(api POST /api/radio/stations '{"name": "x", "url": "file:///etc/passwd"}')" 400

log "=== Playing DR P3"
check "POST /api/radio/play" "$(api POST /api/radio/play '{"id": "dr-p3"}')" 200
playing=no
for _ in $(seq 20); do [ "$(hat)" = 1 ] && { playing=yes; break; }; sleep 1; done
check "the HAT plays the stream" "$playing" yes
sleep 5
api GET /api/audio/source >/dev/null
src=$(body)
check "radio is the active source" "$(echo "$src" | grep -c '"active":"radio"')" 1
check "the station is reported" "$(echo "$src" | grep -c '"station":"dr-p3"')" 1
log "  programme: $(echo "$src" | grep -o '"title":"[^"]*"' | head -1)"

check "the stream player plays it" "$(streaming)" 1

log "=== An announcement pauses the radio"
# Paused, the player closes its output (the radio stays the active source).
remote "for i in \$(seq 40); do pgrep -f '[a]play -q -D speaker -t raw' >/dev/null && echo playing || echo paused; sleep 0.25; done | uniq | tr '\n' ' ' > /tmp/kds-radio-watch" &
watch=$!
sleep 0.5
check "POST /api/tts" "$(api POST /api/tts '{"text": "Radio", "lang": "en"}')" 202
wait $watch
states=$(remote cat /tmp/kds-radio-watch)
log "  the radio over time: $states"
check "the radio was paused during the announcement" "$(echo "$states" | grep -c 'playing paused playing')" 1
api GET /api/audio/source >/dev/null
check "the radio plays again afterwards" "$(body | grep -c '"active":"radio"')" 1

log "=== Stop"
check "POST /api/radio/stop" "$(api POST /api/radio/stop)" 200
sleep 2
api GET /api/audio/source >/dev/null
check "radio is idle" "$(body | grep -o '"radio":{[^}]*"state":"[a-z]*"' | grep -o 'state":"[a-z]*' | head -1)" 'state":"idle'
check "the stream player's output is closed" "$(streaming)" 0

if [ "$fail" = 0 ]; then log "All radio tests passed."; else log "Some radio tests FAILED."; fi
exit "$fail"
