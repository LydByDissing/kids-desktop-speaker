#!/bin/bash
# hw-announce-test.sh — prove the announcement flow on a real Pi (TTS_REQ_002, DEC-033)
# ====================================================================================
# Usage: PI_HOST=root@host hw-announce-test.sh
#
# Runs against the image already installed on the Pi. Sends announcements through
# the API and checks that the running source (shairport-sync) is stopped while the
# chimes and speech play on the HAT and resumed afterwards, that a repeated
# message comes from the TTS cache, and that bad requests are refused. You should
# hear: ding-ding, the message, ding-ding.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
trap 'rm -rf "$WORK"' EXIT

remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

# Waits up to 30 s for the audio-player journal since $1 to contain $2; prints the count.
journal_count() {
    local n=0
    for _ in $(seq 15); do
        n=$(remote "journalctl -u audio-player --since @$1 -o cat --no-pager" | grep -c "$2")
        [ "$n" -gt 0 ] && break
        sleep 2
    done
    echo "$n"
}

api() { # method path [json]
    remote "curl -s -o /tmp/kds-api.out -w '%{http_code}' -X $1 -H 'content-type: application/json' ${3:+-d '$3'} http://127.0.0.1:8080$2"
}

log "=== Preconditions"
check "audio-player is running" "$(remote systemctl is-active audio-player)" active
check "espeak-ng is installed" "$(remote 'command -v espeak-ng >/dev/null && echo yes')" yes
src=$(remote pgrep -x shairport-sync | head -1)
check "a source is running (shairport-sync)" "$([ -n "$src" ] && echo yes)" yes

log "=== Announcement during a running source"
# A unique message, so the first time is a cache miss. Words, not a timestamp:
# espeak reads a long number out digit group by digit group.
words=(red green blue yellow orange purple)
msg="Test ${words[RANDOM % 6]} ${words[RANDOM % 6]} ${words[RANDOM % 6]} $((RANDOM % 100))"
since=$(remote date +%s)
# Watch the source's process state and the HAT for 15 s, every 0.2 s.
remote "for i in \$(seq 75); do s=\$(awk '{print \$3}' /proc/$src/stat 2>/dev/null); h=\$(grep -c RUNNING /proc/asound/IQaudIOCODEC/pcm0p/sub0/status 2>/dev/null); echo \"\$s\$h\"; sleep 0.2; done > /tmp/kds-watch" &
watch=$!
sleep 0.5
check "POST /api/tts is accepted" "$(api POST /api/tts "{\"text\": \"$msg\", \"lang\": \"en-GB\"}")" 202
wait $watch
states=$(remote cat /tmp/kds-watch | uniq | tr '\n' ' ')
log "  source state + HAT running, over time: $states"
check "the source was stopped while the HAT played" "$(echo "$states" | grep -q 'T1' && echo yes)" yes
check "the source runs again afterwards" "$(remote "awk '{print \$3}' /proc/$src/stat")" S
check "the source was not restarted" "$(remote pgrep -x shairport-sync | head -1)" "$src"
check "the announcement was synthesised (cache miss)" "$(journal_count "$since" 'announced (En, .*cache Miss)')" 1

log "=== The same message again comes from the cache"
since=$(remote date +%s)
check "POST /api/tts is accepted" "$(api POST /api/tts "{\"text\": \"$msg\", \"lang\": \"en\"}")" 202
check "the repeat was a cache hit" "$(journal_count "$since" 'cache Hit')" 1

log "=== Danish is the default"
since=$(remote date +%s)
check "POST /api/tts without a language" "$(api POST /api/tts '{"text": "Aftensmaden er klar"}')" 202
check "it was spoken in Danish" "$(journal_count "$since" 'announced (Da,')" 1

log "=== Bad requests"
check "empty text is 400" "$(api POST /api/tts '{"text": " "}')" 400
check "a language that is not enabled is 400" "$(api POST /api/tts '{"text": "Bonjour", "lang": "fr"}')" 400

if [ "$fail" = 0 ]; then log "All announcement tests passed."; else log "Some announcement tests FAILED."; fi
exit "$fail"
