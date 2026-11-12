#!/bin/bash
# hw-dsp-test.sh — CamillaDSP on a real Pi: levels, clipping and volume (DEC-036)
# ===============================================================================
# Usage: PI_HOST=root@host [KDS_AIRPLAY_SINK=<pipewire raop sink>] hw-dsp-test.sh
#
# Runs on the build host against the image installed on the Pi. Reads
# CamillaDSP's meters through an SSH tunnel to its localhost websocket and plays
# test tones into "speaker":
#   - a -6 dBFS tone arrives at -6 dB and leaves lowered by the config's
#     headroom (mono mix, no gain error)
#   - a full-scale tone leaves under the -1 dBFS limit, with no clipped samples
#   - the loopback's rate follows the HAT, and CamillaDSP keeps up
# With KDS_AIRPLAY_SINK set (this host streams AirPlay through PipeWire), it
# also checks that the sender's volume sets the speaker volume (DEC-049). With
# KDS_DSP_SOAK_MIN=<minutes> it plays DR P3 that long and counts underruns.
# The speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

PORT=$((20000 + RANDOM % 10000))
ssh -o BatchMode=yes "${SSH_EXTRA[@]}" -p "${HW_SSH_PORT:-22}" -f -N -o ExitOnForwardFailure=yes \
    -L "$PORT:127.0.0.1:1234" "$PI_HOST" || { echo "error: cannot open a tunnel to CamillaDSP" >&2; exit 2; }
tunnel=$(pgrep -f "[-]L $PORT:127.0.0.1:1234" | head -1)
# The level checks need the device volume at 0 dB (DEC-040); restored after.
volume_before=$(remote "curl -s http://127.0.0.1:8080/api/audio/volume")
restore_volume() {
    local v l
    v=$(echo "$volume_before" | python3 -c 'import json,sys; print(json.load(sys.stdin)["volume"])' 2>/dev/null) || return
    l=$(echo "$volume_before" | python3 -c 'import json,sys; print(json.load(sys.stdin)["limit"])')
    remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"limit\": $l}' http://127.0.0.1:8080/api/audio/volume-limit"
    remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"volume\": $v}' http://127.0.0.1:8080/api/audio/volume"
}
trap 'kill $tunnel 2>/dev/null; rm -rf "$WORK"; restore_volume; speaker_unmute' EXIT
speaker_mute
remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"limit\": 100}' http://127.0.0.1:8080/api/audio/volume-limit"
remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"volume\": 100}' http://127.0.0.1:8080/api/audio/volume"

ws() { python3 "$REPO/ci/scripts/cdsp-ws.py" "127.0.0.1:$PORT" "$@"; }
value() { # command -> the reply's value; for a pair of channels, the left one
    ws "$1" | python3 -c 'import json,sys; v=list(json.load(sys.stdin).values())[0]["value"]; print(v[0] if isinstance(v, list) else v)'
}
near() { python3 -c "import sys; sys.exit(0 if abs($1 - ($2)) <= $3 else 1)" && echo yes || echo "no ($1)"; }
atmost() { python3 -c "import sys; sys.exit(0 if $1 <= $2 else 1)" && echo yes || echo "no ($1)"; }

tone() { # name dBFS rate seconds
    python3 - "$WORK/$1.wav" "$2" "$3" "$4" <<'PY'
import math, struct, sys, wave
path, db, rate, secs = sys.argv[1], float(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4])
a = 10 ** (db / 20) * 32767
w = wave.open(path, "wb"); w.setnchannels(2); w.setsampwidth(2); w.setframerate(rate)
w.writeframes(b"".join(struct.pack("<hh", int(a * math.sin(2 * math.pi * 1000 * i / rate)),
                                   int(a * math.sin(2 * math.pi * 1000 * i / rate))) for i in range(rate * secs)))
PY
}
# CamillaDSP logs an underrun each time a stream ends: the loopback stops, its
# capture stalls and the HAT runs dry, into silence. Only underruns while a tone
# plays count, so each tone's middle seconds are checked.
XRUNS=0
play_and_measure() { # tone-name -> sets CAP, PLAY, CLIP; adds to XRUNS
    "${SCP[@]}" "$WORK/$1.wav" "$PI_HOST:/tmp/kds-$1.wav"
    remote "aplay -q -D speaker /tmp/kds-$1.wav" &
    local player=$!
    sleep 1.5
    local from; from=$(remote date +%s.%N)
    sleep 1.5
    CAP=$(value GetCaptureSignalPeak); PLAY=$(value GetPlaybackSignalPeak); CLIP=$(value GetClippedSamples)
    local to; to=$(remote date +%s.%N)
    XRUNS=$((XRUNS + $(remote "journalctl -u camilladsp --since @$from --until @$to -o cat --no-pager" | grep -ciE 'underrun|overrun')))
    wait $player
}

log "=== CamillaDSP"
check "camilladsp is running" "$(remote systemctl is-active camilladsp)" active
check "it is processing" "$(value GetState)" Running
check "its websocket is reachable only on localhost" "$(remote "ss -ltnH 'sport = :1234'" | awk '{print $4}' | sort -u | paste -sd,)" "127.0.0.1:1234"
clipped_before=$(value GetClippedSamples)

log "=== Levels (mono, headroom, limiter: KDS-9jt.6)"
# The pipeline adds left and right (-6 dB each), cuts by the headroom in the
# config and limits at -1 dBFS. The tones are the same on both channels, so
# they keep their level through the mono mix.
headroom=$(remote "sed -n 's/^ *headroom: {type: Gain, parameters: {gain: \(-\?[0-9.]*\)}}.*/\1/p' /etc/camilladsp/config.yml")
check "the config has a headroom stage" "$([ -n "$headroom" ] && echo yes)" yes
log "  headroom: ${headroom:-?} dB"
tone tone-6 -6 48000 6
play_and_measure tone-6
check "-6 dBFS arrives at -6 dB" "$(near "$CAP" -6 0.3)" yes
check "-6 dBFS leaves lowered by the headroom" "$(near "$PLAY" "-6 + (${headroom:-0})" 0.3)" yes
tone tone-0 0 44100 6
play_and_measure tone-0
check "full scale arrives at 0 dB (44.1 kHz converted)" "$(near "$CAP" 0 0.3)" yes
check "full scale leaves lowered by the headroom, under the -1 dBFS limit" \
    "$(near "$PLAY" "$(python3 -c "print(min(-1, ${headroom:-0}))")" 0.3)" yes
check "no samples were clipped" "$CLIP" "$clipped_before"

log "=== Timing"
rate=$(value GetRateAdjust)
check "the loopback follows the HAT's clock (rate within 0.1 %)" "$(near "$rate" 1 0.001)" yes
load=$(value GetProcessingLoad)
check "processing load is below 50 %" "$(atmost "$load" 50)" yes
check "no underruns or overruns while the tones played" "$XRUNS" 0

if [ -n "${KDS_AIRPLAY_SINK:-}" ]; then
    log "=== The sender's volume sets the speaker volume (AirPlay from this host, DEC-049)"
    node=$(wpctl status | grep -m1 -i "$(echo "$KDS_AIRPLAY_SINK" | sed -E 's/^raop_sink\.([^.]*).*/\1/')" | grep -oE '[0-9]+' | head -1)
    tone airplay -6 44100 40
    device_volume() { remote "curl -s http://127.0.0.1:8080/api/audio/volume" | python3 -c 'import json,sys; print(json.load(sys.stdin)["volume"])'; }
    wpctl set-volume "$node" 1.0
    pw-play --target "$KDS_AIRPLAY_SINK" "$WORK/airplay.wav" >/dev/null 2>&1 &
    sender=$!
    sleep 12
    loud=$(value GetCaptureSignalPeak)
    at_full=$(device_volume)
    wpctl set-volume "$node" 0.5
    sleep 6
    quiet=$(value GetCaptureSignalPeak)
    at_half=$(device_volume)
    wpctl set-volume "$node" 1.0
    kill $sender 2>/dev/null
    log "  capture peak at sender volume 1.0: $loud dB, at 0.5: $quiet dB; speaker volume $at_full % then $at_half %"
    check "the audio arrives at full level (-6 dB)" "$(near "$loud" -6 0.5)" yes
    check "the sender's volume doesn't scale the audio" "$(near "$quiet" "$loud" 0.5)" yes
    check "sender at full volume: the speaker plays at its limit" "$at_full" 100
    check "turning the sender down turns the speaker down" "$(atmost "$at_half" 90)" yes
fi

if [ -n "${KDS_DSP_SOAK_MIN:-}" ]; then
    log "=== Soak: ${KDS_DSP_SOAK_MIN} min of radio"
    remote "curl -s -o /dev/null -X POST -H 'content-type: application/json' -d '{\"id\": \"dr-p3\"}' http://127.0.0.1:8080/api/radio/play"
    sleep 15
    from=$(remote date +%s)
    sleep $((KDS_DSP_SOAK_MIN * 60))
    to=$(remote date +%s)
    remote "curl -s -o /dev/null -X POST http://127.0.0.1:8080/api/radio/stop"
    check "no underruns during ${KDS_DSP_SOAK_MIN} min of radio" \
        "$(remote "journalctl -u camilladsp --since @$from --until @$to -o cat --no-pager" | grep -ciE 'underrun|overrun')" 0
fi

if [ "$fail" = 0 ]; then log "All DSP tests passed."; else log "Some DSP tests FAILED."; fi
exit "$fail"
