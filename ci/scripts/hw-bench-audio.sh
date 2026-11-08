#!/bin/bash
# hw-bench-audio.sh — measure audio candidates on a real Pi Zero W (wave 3)
# ========================================================================
# Usage: PI_HOST=root@kidsspeaker-XXXX.local hw-bench-audio.sh
#
# Installs candidate packages on the Pi under test (not in the image) and records
# CPU and memory for: CamillaDSP with the design's pipeline, internet radio decoding
# with mpv and GStreamer, and text-to-speech with espeak-ng, flite and Piper.
# Audio goes to ALSA's null device, so no speaker or HAT is needed. Reflash the Pi
# afterwards for a clean device.

set -uo pipefail
: "${PI_HOST:?set PI_HOST=root@host}"
SSH=(ssh -o BatchMode=yes -o ConnectTimeout=10 -o StrictHostKeyChecking=accept-new "$PI_HOST")
CAMILLA_URL=https://github.com/HEnquist/camilladsp/releases/download/v4.1.3/camilladsp-linux-armv6.tar.gz
PIPER_URL=https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_armv7l.tar.gz
RADIO_URL="${KDS_BENCH_RADIO_URL:-http://live-icy.dr.dk/A/A05H.mp3}"
SECONDS_PER_RUN="${KDS_BENCH_SECONDS:-60}"

"${SSH[@]}" "CAMILLA_URL=$CAMILLA_URL PIPER_URL=$PIPER_URL RADIO_URL=$RADIO_URL RUN=$SECONDS_PER_RUN bash -s" <<'REMOTE'
set -u
W=/data/bench; mkdir -p $W; cd $W
TICKS=$(getconf CLK_TCK)

echo "=== Setup"
apt-get update -qq >/dev/null 2>&1
DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
    mpv gstreamer1.0-tools gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
    gstreamer1.0-plugins-ugly espeak-ng flite alsa-utils >/dev/null 2>&1 && echo "packages installed"
[ -x camilladsp ] || curl -fsSL "$CAMILLA_URL" | tar -xz
[ -d piper ] || curl -fsSL "$PIPER_URL" | tar -xz
modprobe snd-aloop && echo "snd-aloop loaded"
uname -m; free -m | sed -n 2p | awk '{print "RAM MB available: " $7}'

# measure <label> <seconds> <command...>: runs the command, then reports average CPU
# (% of the single core) and peak memory. The command is stopped after <seconds>.
measure() {
    local label=$1 secs=$2; shift 2
    "$@" >"$W/$label.log" 2>&1 &
    local pid=$! peak=0 t0 t1 rss
    sleep 3
    t0=$(awk '{print $14+$15+$16+$17}' /proc/$pid/stat 2>/dev/null || echo 0)
    for _ in $(seq 1 $((secs - 3))); do
        rss=$(ps -o rss= --ppid $pid --pid $pid 2>/dev/null | awk '{s+=$1} END {print s+0}')
        [ "$rss" -gt "$peak" ] && peak=$rss
        sleep 1
    done
    # Include children: total CPU of the process tree.
    t1=$(ps -o times= --ppid $pid --pid $pid 2>/dev/null | awk '{s+=$1} END {print s+0}')
    local cpu=$(awk -v t="$t1" -v s="$((secs - 3))" 'BEGIN{printf "%.1f", 100*t/(s+3)}')
    kill $pid 2>/dev/null; wait $pid 2>/dev/null
    printf "%-34s CPU %6s%%   peak RSS %6s KB\n" "$label" "$cpu" "$peak"
}

echo "=== CamillaDSP (design pipeline: 6 ch in, 2 out, 48 kHz S32_LE)"
cat > camilla-design.yml <<'YML'
devices:
  samplerate: 48000
  chunksize: 1024
  queuelimit: 4
  capture: {type: Alsa, channels: 6, device: "hw:Loopback,1,0", format: S32_LE}
  playback: {type: Alsa, channels: 2, device: "null", format: S32_LE}
filters:
  main_volume: {type: Volume, parameters: {ramp_time: 400, fader: Aux1}}
  tts_volume: {type: Volume, parameters: {ramp_time: 200, fader: Aux2}}
  bt_volume: {type: Volume, parameters: {ramp_time: 400, fader: Aux3}}
mixers:
  to_stereo:
    channels: {in: 6, out: 2}
    mapping:
      - {dest: 0, sources: [{channel: 0, gain: 0}, {channel: 2, gain: 0}, {channel: 4, gain: 0}]}
      - {dest: 1, sources: [{channel: 1, gain: 0}, {channel: 3, gain: 0}, {channel: 5, gain: 0}]}
pipeline:
  - {type: Filter, channels: [0, 1], names: [main_volume]}
  - {type: Filter, channels: [2, 3], names: [tts_volume]}
  - {type: Filter, channels: [4, 5], names: [bt_volume]}
  - {type: Mixer, name: to_stereo}
YML
# The same, plus 10 peaking EQ filters per output channel, as an upper bound.
# Plain shell: the device has no python3.
{
    sed '/^mixers:/,$d' camilla-design.yml
    for i in $(seq 0 9); do
        echo "  eq$i: {type: Biquad, parameters: {type: Peaking, freq: $((100 * (i + 1))), q: 1.0, gain: -1.0}}"
    done
    sed -n '/^mixers:/,$p' camilla-design.yml
    echo "  - {type: Filter, channels: [0, 1], names: [eq0, eq1, eq2, eq3, eq4, eq5, eq6, eq7, eq8, eq9]}"
} > camilla-eq.yml
speaker-test -D hw:Loopback,0,0 -c 6 -r 48000 -F S32_LE -t sine >/dev/null 2>&1 &
FEEDER=$!
sleep 2
./camilladsp --check camilla-design.yml >/dev/null 2>&1 && echo "design config is valid for CamillaDSP v4" || { echo "design config INVALID:"; ./camilladsp --check camilla-design.yml 2>&1 | tail -3; }
measure "camilladsp design pipeline" $RUN ./camilladsp camilla-design.yml
measure "camilladsp + 10 EQ per channel" $RUN ./camilladsp camilla-eq.yml
a=$(awk '{print $14+$15}' /proc/$FEEDER/stat); sleep 20; b=$(awk '{print $14+$15}' /proc/$FEEDER/stat)
printf "%-34s CPU %6s%%\n" "speaker-test feeding loopback" "$(awk -v d=$((b - a)) -v t=$TICKS 'BEGIN{printf "%.1f", 100*d/(20*t)}')"
kill $FEEDER 2>/dev/null; wait $FEEDER 2>/dev/null
grep -iE "error|underrun|overrun" camilladsp*.log 2>/dev/null | head -3

echo "=== Internet radio ($RADIO_URL)"
measure "mpv MP3 stream" $RUN mpv --no-video --ao=null --really-quiet "$RADIO_URL"
measure "gstreamer MP3 stream" $RUN gst-launch-1.0 -q souphttpsrc location="$RADIO_URL" ! decodebin ! audioconvert ! audioresample ! fakesink sync=true

echo "=== Text-to-speech (100 characters)"
TEXT="Good morning! It is seven o'clock, time to get up. Breakfast is ready in the kitchen downstairs now."
echo "text length: ${#TEXT}"
for engine in espeak-ng flite; do
    s=$(date +%s%N)
    case $engine in
        espeak-ng) espeak-ng -w tts-espeak.wav "$TEXT" ;;
        flite) flite -t "$TEXT" -o tts-flite.wav ;;
    esac
    e=$(( ($(date +%s%N) - s) / 1000000 ))
    f=tts-${engine%-ng}.wav
    rate=$(od -An -tu4 -j24 -N4 "$f" | tr -d ' ')
    dur=$(awk -v s="$(stat -c%s "$f")" -v r="$rate" 'BEGIN{printf "%.1f", (s - 44) / (r * 2)}')
    printf "%-34s %6s ms to synthesise %s s of speech at %s Hz\n" "$engine" "$e" "$dur" "$rate"
done
./piper/piper --help >/dev/null 2>&1; rc=$?
case $rc in
    132) echo "piper (armv7 build)                Illegal instruction: does not run on ARMv6" ;;
    0) echo "piper (armv7 build)                runs" ;;
    *) echo "piper (armv7 build)                exit $rc" ;;
esac
echo "=== Done"
REMOTE
