#!/bin/bash
# hw-setup-test.sh — the spoken setup code on a real Pi (AUTH_REQ_003, DEC-038)
# ============================================================================
# Usage: PI_HOST=root@host hw-setup-test.sh
#
# Makes the Pi forget its tokens for a while (they are restored afterwards),
# restarts the API Server and checks that the speaker reads a setup code aloud
# repeatedly, with a pause between readings, that the API refuses requests from
# the network, and that the readings stop once an admin token exists. The code
# itself is never logged, so the test sets the speaker up over loopback. The
# speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

restore() {
    remote "if [ -f /data/api/tokens.json.hw-setup-test ]; then mv /data/api/tokens.json.hw-setup-test /data/api/tokens.json; else rm -f /data/api/tokens.json; fi; systemctl restart api-server"
    speaker_unmute
}
trap restore EXIT
speaker_mute

host=$(remote cat /proc/sys/kernel/hostname)
log "=== An unconfigured speaker"
remote "systemctl stop api-server; [ -f /data/api/tokens.json ] && mv /data/api/tokens.json /data/api/tokens.json.hw-setup-test; systemctl start api-server"
since=$(remote date +%s)
sleep 50
readings=$(remote "journalctl -u audio-player --since @$since -o short-unix --no-pager" | grep 'announced (Da' )
count=$(echo "$readings" | grep -c . )
log "  readings in 50 s: $count"
check "it reads the setup code aloud repeatedly" "$([ "$count" -ge 2 ] && echo yes)" yes
gap=$(echo "$readings" | awk '{t[NR]=$1} END {if (NR>=2) printf "%d", t[2]-t[1]}')
log "  time between the first two readings: ${gap:-?} s (reading plus the 8 s pause)"
check "with a pause between readings" "$([ "${gap:-0}" -ge 9 ] && echo yes)" yes
check "the network gets 401 without a token" "$(curl -sk -o /dev/null -w '%{http_code}' "https://$host.local/api/audio/source")" 401
check "/health stays open" "$(curl -sk -o /dev/null -w '%{http_code}' "https://$host.local/health")" 200
check "asking for a new code too soon is refused" \
    "$(curl -sk -o /dev/null -w '%{http_code}' -X POST "https://$host.local/api/setup/start"; curl -sk -o /dev/null -w '%{http_code}' -X POST "https://$host.local/api/setup/start")" 202429
check "a wrong code is refused" \
    "$(curl -sk -o /dev/null -w '%{http_code}' -X POST -H 'content-type: application/json' -d '{"code": "000000", "name": "x"}' "https://$host.local/api/setup")" 403

log "=== Once set up, the readings stop"
remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"name\": \"hw-setup-test\", \"admin\": true}' http://127.0.0.1:8080/api/tokens"
sleep 30
stopped_since=$(remote date +%s)
sleep 30
check "no readings after setup" "$(remote "journalctl -u audio-player --since @$stopped_since -o cat --no-pager" | grep -c 'announced (Da')" 0

if [ "$fail" = 0 ]; then log "All setup tests passed."; else log "Some setup tests FAILED."; fi
exit "$fail"
