#!/bin/bash
# hw-volume-test.sh — the device volume and the limit on a real Pi (VOL_REQ_001, DEC-040)
# ======================================================================================
# Usage: PI_HOST=root@host hw-volume-test.sh
#
# Sets the volume and the limit through the API (from the Pi itself, which needs
# no token) and checks the amplifier's gain (the codec's Lineout Volume,
# DEC-050), that CamillaDSP stays at 0 dB (through an SSH tunnel to its
# websocket) and the level of a -6 dBFS tone leaving it. Restores the
# settings afterwards. The speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

api() { # method path [json] -> prints the body
    remote "curl -s -X $1 -H 'content-type: application/json' ${3:+-d '$3'} http://127.0.0.1:8080$2"
}
before=$(api GET /api/audio/volume)
PORT=$((20000 + RANDOM % 10000))
ssh -o BatchMode=yes "${SSH_EXTRA[@]}" -p "${HW_SSH_PORT:-22}" -f -N -o ExitOnForwardFailure=yes \
    -L "$PORT:127.0.0.1:1234" "$PI_HOST" || { echo "error: cannot open a tunnel to CamillaDSP" >&2; exit 2; }
tunnel=$(pgrep -f "[-]L $PORT:127.0.0.1:1234" | head -1)
restore() {
    local v l
    v=$(echo "$before" | python3 -c 'import json,sys; print(json.load(sys.stdin)["volume"])')
    l=$(echo "$before" | python3 -c 'import json,sys; print(json.load(sys.stdin)["limit"])')
    api POST /api/audio/volume-limit "{\"limit\": $l}" >/dev/null
    api POST /api/audio/volume "{\"volume\": $v}" >/dev/null
    kill "$tunnel" 2>/dev/null
    rm -rf "$WORK"
    speaker_unmute
}
trap restore EXIT
speaker_mute

ws() { python3 "$REPO/ci/scripts/cdsp-ws.py" "127.0.0.1:$PORT" "$@"; }
value() { ws "$1" | python3 -c 'import json,sys; v=list(json.load(sys.stdin).values())[0]["value"]; print(v[0] if isinstance(v, list) else v)'; }
field() { echo "$1" | python3 -c "import json,sys; print(json.load(sys.stdin)[\"$2\"])"; }
near() { python3 -c "import sys; sys.exit(0 if abs($1 - ($2)) <= $3 else 1)" && echo yes || echo "no ($1)"; }
# The amplifier's gain (DEC-050): the codec's Lineout Volume step, 48 = 0 dB.
lineout() { remote "amixer -c IQaudIOCODEC cget name='Lineout Volume'" | sed -n 's/.*: values=//p'; }

log "=== Volume"
r=$(api POST /api/audio/volume-limit '{"limit": 75}')
r=$(api POST /api/audio/volume '{"volume": 50}')
check "the API reports 50 % as -31.5 dB" "$(field "$r" db)" -31.5
# Loudness (DEC-051) cuts the mids by 7.8 dB at 50 %; the amplifier makes it
# up: 63 - 31.5 + 7.8 = step 39.
check "the amplifier is at step 39" "$(lineout)" 39
check "CamillaDSP stays at 0 dB" "$(near "$(value GetVolume)" 0 0.1)" yes
check "and not muted" "$(value GetMute)" False

log "=== The limit"
r=$(api POST /api/audio/volume '{"volume": 95}')
check "asking for 95 % gives the 75 % limit" "$(field "$r" volume)" 75
check "the amplifier is at step 49 (75 %: 63 - 15.75 + 1.5)" "$(lineout)" 49
r=$(api POST /api/audio/volume-limit '{"limit": 40}')
check "lowering the limit to 40 % lowers the volume" "$(field "$r" volume)" 40
check "the amplifier is at step 33 (40 %: 63 - 37.8 + 8)" "$(lineout)" 33

log "=== It applies to the sound"
python3 - "$WORK/tone.wav" <<'PY'
import math, struct, sys, wave
a = 10 ** (-6 / 20) * 32767
w = wave.open(sys.argv[1], "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(48000)
w.writeframes(b"".join(struct.pack("<hh", int(a*math.sin(2*math.pi*1000*i/48000)), int(a*math.sin(2*math.pi*1000*i/48000))) for i in range(48000*5)))
PY
"${SCP[@]}" "$WORK/tone.wav" "$PI_HOST:/tmp/kds-volume-tone.wav"
remote "aplay -q -D speaker /tmp/kds-volume-tone.wav" &
sleep 2.5
cap=$(value GetCaptureSignalPeak); out=$(value GetPlaybackSignalPeak)
wait
log "  -6 dBFS tone at 40 %: in $cap dB, out $out dB"
# The volume is analog now: the DAC gets the tone at its own level, less the
# loudness filter's 8 dB mid cut at 40 % (the amplifier makes that up).
check "the DAC gets the tone at -6 dB less the 8 dB mid cut (-14 dB)" "$(near "$out" -14 0.7)" yes

log "=== Mute"
r=$(api POST /api/audio/volume '{"volume": 0}')
check "0 % mutes CamillaDSP" "$(value GetMute)" True

if [ "$fail" = 0 ]; then log "All volume tests passed."; else log "Some volume tests FAILED."; fi
exit "$fail"
