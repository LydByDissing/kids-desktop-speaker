#!/bin/bash
# hw-podcasts-test.sh — podcasts on a real Pi (POD_REQ_001/002)
# =============================================================
# Usage: PI_HOST=root@host [KDS_PODCAST_FEED=<rss url>] hw-podcasts-test.sh
#
# Needs internet access on the Pi. Adds a feed, lists its episodes, plays the
# newest for a while, stops, and checks that the position was saved and that
# playing it again continues there. Removes the feed afterwards. The speaker is
# muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
FEED_URL=${KDS_PODCAST_FEED:-https://feeds.npr.org/500005/podcast.xml}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
feed=
cleanup() {
    remote "curl -s -o /dev/null -X POST http://127.0.0.1:8080/api/podcasts/stop"
    [ -n "$feed" ] && remote "curl -s -o /dev/null -X DELETE http://127.0.0.1:8080/api/podcasts/$feed"
    speaker_unmute
}
trap cleanup EXIT
speaker_mute

api() { # method path [json] -> body in /tmp/kds-api.out on the Pi, prints status
    remote "curl -s -o /tmp/kds-api.out -w '%{http_code}' -X $1 -H 'content-type: application/json' ${3:+-d '$3'} http://127.0.0.1:8080$2"
}
field() { remote cat /tmp/kds-api.out | python3 -c "import json,sys; d=json.load(sys.stdin); print($1)"; }

log "=== Feed: $FEED_URL"
check "POST /api/podcasts adds it" "$(api POST /api/podcasts "{\"url\": \"$FEED_URL\"}")" 201
feed=$(field 'd["feed"]["id"]')
log "  title: $(field 'd["feed"]["title"]')"
check "GET .../episodes lists episodes" "$(api GET "/api/podcasts/$feed/episodes")" 200
episode=$(field 'd["episodes"][0]["id"]')
check "there are episodes" "$([ -n "$episode" ] && echo yes)" yes

log "=== Play, stop, resume"
check "POST /api/podcasts/play" "$(api POST /api/podcasts/play "{\"feed\": \"$feed\", \"episode\": \"$episode\"}")" 200
sleep 25
check "the podcast is the active source" "$(remote "curl -s http://127.0.0.1:8080/api/audio/source" | grep -c '"active":"radio"')" 1
check "POST /api/podcasts/stop" "$(api POST /api/podcasts/stop)" 200
api GET "/api/podcasts/$feed/episodes" >/dev/null
saved=$(field 'd["episodes"][0]["position_s"]')
log "  saved position: $saved s"
check "the position was saved (over 15 s)" "$(python3 -c "print('yes' if ($saved or 0) > 15 else 'no')")" yes
check "playing it again" "$(api POST /api/podcasts/play "{\"feed\": \"$feed\", \"episode\": \"$episode\"}")" 200
# From where it stopped, 8 s more puts it past that point; from the start it
# would be at about 8 s.
sleep 8
check "POST /api/podcasts/stop" "$(api POST /api/podcasts/stop)" 200
api GET "/api/podcasts/$feed/episodes" >/dev/null
again=$(field 'd["episodes"][0]["position_s"]')
log "  position after resuming and stopping: $again s"
check "it continued where it stopped" "$(python3 -c "print('yes' if ($again or 0) >= ($saved or 0) + 4 else 'no')")" yes

if [ "$fail" = 0 ]; then log "All podcast tests passed."; else log "Some podcast tests FAILED."; fi
exit "$fail"
