#!/bin/bash
# hw-schedule-test.sh — alarms and the sleep timer on a real Pi (ALM_REQ_001, SLP_REQ_001)
# ======================================================================================
# Usage: PI_HOST=root@host hw-schedule-test.sh
#
# Needs internet access on the Pi. Sets an alarm for the next minute and checks
# that DR P3 starts and the volume rises to the alarm's level; then runs a
# 1-minute sleep timer and checks that it fades out, stops and restores the
# volume. Takes about 4 minutes; restores the volume afterwards. The speaker is
# muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

api() { remote "curl -s -X $1 -H 'content-type: application/json' ${3:+-d '$3'} http://127.0.0.1:8080$2"; }
field() { echo "$1" | python3 -c "import json,sys; print(json.load(sys.stdin)$2)"; }
before=$(api GET /api/audio/volume)
alarm=
cleanup() {
    [ -n "$alarm" ] && api DELETE "/api/alarms/$alarm" >/dev/null
    api DELETE /api/timer >/dev/null
    api POST /api/radio/stop >/dev/null
    api POST /api/audio/volume "{\"volume\": $(field "$before" '["volume"]')}" >/dev/null
    speaker_unmute
}
trap cleanup EXIT
speaker_mute
api POST /api/radio/stop >/dev/null
api POST /api/audio/volume '{"volume": 20}' >/dev/null

log "=== Alarm"
check "the Pi keeps Copenhagen time" "$(remote 'readlink /etc/localtime')" /usr/share/zoneinfo/Europe/Copenhagen
# The next whole minute, at least 20 s away, in the Pi's local time.
when=$(remote 'date -d "@$(( ( $(date +%s) + 80 ) / 60 * 60 ))" +%H:%M')
r=$(api POST /api/alarms "{\"time\": \"$when\", \"station\": \"dr-p3\", \"volume\": 60}")
alarm=$(field "$r" '["alarm"]["id"]')
log "  alarm $alarm at $when"
for _ in $(seq 60); do
    [ "$(api GET /api/audio/source | python3 -c 'import json,sys; print(json.load(sys.stdin)["active"])')" = radio ] && break
    sleep 2
done
check "the alarm started the radio" "$(api GET /api/audio/source | python3 -c 'import json,sys; print(json.load(sys.stdin)["active"])')" radio
early=$(field "$(api GET /api/audio/volume)" '["volume"]')
log "  volume just after it started: $early %"
check "it started quietly (below the alarm's 60 %)" "$([ "$early" -lt 60 ] && echo yes)" yes
sleep 70
check "the volume rose to the alarm's 60 %" "$(field "$(api GET /api/audio/volume)" '["volume"]')" 60
check "it goes off only once" "$(remote "journalctl -u api-server --since -4min -o cat --no-pager" | grep -c "alarm $alarm .* goes off")" 1

log "=== Sleep timer"
r=$(api POST /api/timer '{"minutes": 1}')
check "a 1-minute timer runs" "$(field "$r" '["running"]')" True
# 1 minute, then the 30-second fade.
sleep 100
check "after the fade, nothing plays" "$(api GET /api/audio/source | python3 -c 'import json,sys; print(json.load(sys.stdin)["active"])')" None
check "the volume is back at 60 %" "$(field "$(api GET /api/audio/volume)" '["volume"]')" 60
check "the timer has finished" "$(field "$(api GET /api/timer)" '["running"]')" False

if [ "$fail" = 0 ]; then log "All schedule tests passed."; else log "Some schedule tests FAILED."; fi
exit "$fail"
