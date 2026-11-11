#!/bin/bash
# hw-sources-test.sh — one source at a time on a real Pi (AUD_REQ_003, DEC-039)
# ============================================================================
# Usage: PI_HOST=root@host KDS_AIRPLAY_SINK=<pipewire raop sink> hw-sources-test.sh
#
# Streams AirPlay from this host (PipeWire) and plays the radio through the API,
# and checks that the newest source wins: radio ends the AirPlay session, and a
# new AirPlay stream stops the radio. The speaker is muted unless
# KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
: "${KDS_AIRPLAY_SINK:?set KDS_AIRPLAY_SINK to the PipeWire AirPlay sink for the Pi on this host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
sender=
cleanup() {
    [ -n "$sender" ] && kill "$sender" 2>/dev/null
    remote "curl -s -o /dev/null -X POST http://127.0.0.1:8080/api/radio/stop"
    rm -rf "$WORK"
    speaker_unmute
}
trap cleanup EXIT
speaker_mute

active() { remote "curl -s http://127.0.0.1:8080/api/audio/source" | python3 -c 'import json,sys; print(json.load(sys.stdin)["active"])'; }
wait_active() { # want -> prints what was seen last
    local seen
    for _ in $(seq 30); do seen=$(active); [ "$seen" = "$1" ] && break; sleep 1; done
    echo "$seen"
}
airplay() {
    python3 - "$WORK/tone.wav" <<'PY'
import math, struct, sys, wave
w = wave.open(sys.argv[1], "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(44100)
w.writeframes(b"".join(struct.pack("<hh", int(3000*math.sin(2*math.pi*440*i/44100)), int(3000*math.sin(2*math.pi*440*i/44100))) for i in range(44100*60)))
PY
    pw-play --target "$KDS_AIRPLAY_SINK" "$WORK/tone.wav" >/dev/null 2>&1 &
    sender=$!
}

log "=== AirPlay starts"
airplay
check "AirPlay is the active source" "$(wait_active airplay)" airplay
shairport=$(remote pgrep -x shairport-sync | head -1)

log "=== The radio starts: AirPlay is stopped"
remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"id\": \"dr-p3\"}' http://127.0.0.1:8080/api/radio/play"
# PipeWire reconnects to the restarted receiver while it still has audio to
# send, which would rightly take over again; an iPhone shows the session ended.
# So the sender stops here, as when someone switches to the radio.
sleep 2
kill "$sender" 2>/dev/null; sender=
check "radio is the active source" "$(wait_active radio)" radio
check "the AirPlay session was ended (shairport-sync restarted)" "$(remote pgrep -x shairport-sync | head -1 | grep -vx "$shairport" | grep -c .)" 1
sleep 5

log "=== AirPlay starts again: the radio stops"
airplay
check "AirPlay is the active source again" "$(wait_active airplay)" airplay
check "MPD stopped" "$(remote "echo status | timeout 3 bash -c 'exec 3<>/dev/tcp/127.0.0.1/6600; cat <&3 & echo status >&3; sleep 0.5; kill %1'" 2>/dev/null | grep -o 'state: [a-z]*')" "state: stop"
journal=$(remote "journalctl -u audio-player --since -3min -o cat --no-pager")
check "the log shows each switch" "$(echo "$journal" | grep -cE 'radio started; stopped airplay|airplay started; stopped radio')" 2

if [ "$fail" = 0 ]; then log "All source tests passed."; else log "Some source tests FAILED."; fi
exit "$fail"
