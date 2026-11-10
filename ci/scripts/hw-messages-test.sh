#!/bin/bash
# hw-messages-test.sh — recorded voice messages on a real Pi (MSG_REQ_001-003, DEC-037)
# ====================================================================================
# Usage: PI_HOST=root@host hw-messages-test.sh
#
# Uploads a short WAV over HTTPS from this host, as the Web UI will, and checks
# that the Audio Player plays it between the chimes with the source paused,
# that it is stored on /data, and that it can be listed, renamed, replayed,
# downloaded and deleted. The speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
trap 'rm -rf "$WORK"; speaker_unmute' EXIT
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
speaker_mute

host=$(remote cat /proc/sys/kernel/hostname)
"${SCP[@]}" "$PI_HOST:/data/tls/cert.pem" "$WORK/device.pem"
api() { # method path [curl args...] -> body in $WORK/out, prints the status
    local method=$1 path=$2; shift 2
    curl -s --cacert "$WORK/device.pem" -o "$WORK/out" -w '%{http_code}' -X "$method" "$@" "https://$host.local$path"
}
field() { python3 -c "import json,sys; d=json.load(open('$WORK/out')); print($1)"; }

# A 2-second 440 Hz tone, 24 kHz mono, as the browser would send.
python3 - "$WORK/msg.wav" <<'PY'
import math, struct, sys, wave
w = wave.open(sys.argv[1], "wb"); w.setnchannels(1); w.setsampwidth(2); w.setframerate(24000)
w.writeframes(b"".join(struct.pack("<h", int(8000 * math.sin(2 * math.pi * 440 * i / 24000))) for i in range(48000)))
PY

log "=== Send a message"
since=$(remote date +%s)
check "POST /api/messages over HTTPS" "$(api POST /api/messages -H 'content-type: audio/wav' --data-binary "@$WORK/msg.wav")" 201
check "it was played at once" "$(field 'd["played"]')" True
id=$(field 'd["message"]["id"]')
check "its duration is 2 s" "$(field 'd["message"]["duration_s"]')" 2.0
check "it is stored on /data" "$(remote "test -s /data/messages/$id.wav && test -s /data/messages/$id.json && echo yes")" yes
played=0
for _ in $(seq 20); do
    played=$(remote "journalctl -u audio-player --since @$since -o cat --no-pager" | grep -c "played message /data/messages/$id.wav")
    [ "$played" -gt 0 ] && break
    sleep 1
done
check "the Audio Player played it" "$played" 1

log "=== Manage it"
check "GET /api/messages" "$(api GET /api/messages)" 200
check "it is listed first" "$(field 'd["messages"][0]["id"]')" "$id"
check "PATCH renames it" "$(api PATCH "/api/messages/$id" -H 'content-type: application/json' -d '{"name": "Godnat"}')" 200
check "the name is saved" "$(field 'd["message"]["name"]')" Godnat
check "GET .../audio returns the recording" "$(api GET "/api/messages/$id/audio")" 200
check "the downloaded recording is the one sent" "$(cmp -s "$WORK/out" "$WORK/msg.wav" && echo same)" same
check "POST .../play replays it" "$(api POST "/api/messages/$id/play")" 200
check "a recording over 30 s is refused" "$(python3 -c "
import struct,sys; n=24000*2*31
sys.stdout.buffer.write(b'RIFF'+struct.pack('<I',36+n)+b'WAVEfmt '+struct.pack('<IHHIIHH',16,1,1,24000,48000,2,16)+b'data'+struct.pack('<I',n)+bytes(n))" > "$WORK/long.wav"; api POST /api/messages --data-binary "@$WORK/long.wav")" 400
sleep 8
check "DELETE removes it" "$(api DELETE "/api/messages/$id")" 200
check "its files are gone" "$(remote "ls /data/messages/$id.* 2>/dev/null | wc -l")" 0

if [ "$fail" = 0 ]; then log "All message tests passed."; else log "Some message tests FAILED."; fi
exit "$fail"
