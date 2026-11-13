#!/usr/bin/env python3
"""Checks the speaker's CamillaDSP config offline (DSP_REQ_001, DEC-051).

Usage: test-dsp-config.py [config.yml] [image]

Runs the config through the image's own CamillaDSP (ARMv6, in Docker under
QEMU) with test signals on stdin instead of the loopback, and measures what
comes out:

- nothing ever leaves above the -0.5 dBFS guard, and the look-ahead limiter
  does the work (the clipper hardly acts);
- quiet bass gets the static boost;
- loud bass is turned down while the mids keep their level;
- loudness lifts the bass relative to the mids as the volume drops;
- the subsonic filter removes what the driver can't play.

Needs the image (make -C ci build) and numpy.
"""
import os
import subprocess
import sys
import tempfile

import numpy as np

RATE = 48000
REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CONFIG = sys.argv[1] if len(sys.argv) > 1 else os.path.join(REPO, "ci/device/camilladsp.yml")
IMAGE = sys.argv[2] if len(sys.argv) > 2 else "kids-desktop-speaker:latest"
failures = 0


def check(name, ok, detail=""):
    global failures
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}{f' ({detail})' if detail else ''}")
    failures += 0 if ok else 1


def offline_config(text):
    """The config with stdin/stdout instead of ALSA, and no rate adjust."""
    out = []
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("capture:"):
            line = "  capture: {type: Stdin, channels: 2, format: S16_LE}"
        elif stripped.startswith("playback:"):
            line = "  playback: {type: Stdout, channels: 2, format: S32_LE}"
        elif stripped.startswith(("enable_rate_adjust:", "target_level:", "adjust_period:")):
            continue
        out.append(line)
    return "\n".join(out) + "\n"


def tone(freq, dbfs, seconds):
    t = np.arange(int(RATE * seconds)) / RATE
    return 10 ** (dbfs / 20) * np.sin(2 * np.pi * freq * t)


def run(signal, aux1_db=0.0):
    """Plays a mono float signal (both channels) through CamillaDSP; returns
    channel 0 of the output as floats (1.0 = full scale)."""
    pcm = np.clip(np.round(signal * 32767), -32768, 32767).astype("<i2")
    stereo = np.repeat(pcm, 2)
    with tempfile.TemporaryDirectory() as tmp:
        with open(os.path.join(tmp, "c.yml"), "w") as f:
            f.write(offline_config(open(CONFIG).read()))
        with open(os.path.join(tmp, "state.yml"), "w") as f:
            f.write("config_path: /work/c.yml\nmute: [false, false, false, false, false]\n"
                    f"volume: [0.0, {aux1_db}, 0.0, 0.0, 0.0]\n")
        # Silence after the signal flushes the delays and filters.
        data = stereo.tobytes() + bytes(4 * RATE // 2)
        result = subprocess.run(
            ["docker", "run", "--rm", "-i", "--platform", "linux/arm/v6", "-v", f"{tmp}:/work",
             "--entrypoint", "/usr/bin/camilladsp", IMAGE, "-s", "/work/state.yml", "/work/c.yml"],
            input=data, capture_output=True, check=True)
    out = np.frombuffer(result.stdout, dtype="<i4").astype(np.float64) / 2**31
    return out[0::2]


def level(signal, freq, skip=0.5):
    """The level in dBFS of `freq` in `signal`, past the first `skip` seconds."""
    x = signal[int(RATE * skip):int(RATE * skip) + RATE]
    spectrum = np.abs(np.fft.rfft(x * np.hanning(len(x)))) * 2 / (len(x) * 0.5)
    bin_ = int(round(freq * len(x) / RATE))
    return 20 * np.log10(max(spectrum[bin_ - 2:bin_ + 3].max(), 1e-12))


def peak_db(signal):
    return 20 * np.log10(max(np.abs(signal).max(), 1e-12))


print(f"CamillaDSP config: {CONFIG}")

print("=== Quiet music: the static bass boost")
out = run(tone(80, -30, 2) + tone(1000, -30, 2))
boost = level(out, 80) - level(out, 1000)
check("80 Hz comes out about 6 dB above 1 kHz", 4.5 <= boost <= 7.5, f"{boost:+.1f} dB")

print("=== Loud bass: the bass is turned down, the mids keep their level")
quiet_mid = level(run(tone(1000, -8, 2)), 1000)
out = run(tone(80, -2, 2) + tone(1000, -8, 2))
mid = level(out, 1000)
check("1 kHz keeps its level next to loud bass", abs(mid - quiet_mid) <= 1.0, f"{mid:.1f} vs {quiet_mid:.1f} dBFS")
check("nothing above the -0.5 dBFS guard", peak_db(out) <= -0.49, f"peak {peak_db(out):.2f} dBFS")

print("=== Full-scale bass and treble: the limiter does the work")
out = run(tone(60, 0, 2) + tone(3000, -3, 2))
at_guard = np.mean(np.abs(out) >= 10 ** (-0.51 / 20))
check("nothing above the -0.5 dBFS guard", peak_db(out) <= -0.49, f"peak {peak_db(out):.2f} dBFS")
check("the clipper hardly acts (under 0.1 % of samples)", at_guard < 0.001, f"{at_guard * 100:.3f} %")

print("=== Loudness: more bass relative to the mids at low volume")
# CamillaDSP's loudness shelf sits at 70 Hz: measure below it.
ref_run = run(tone(50, -30, 2) + tone(1000, -30, 2), aux1_db=0.0)
reference = level(ref_run, 50) - level(ref_run, 1000)
low = run(tone(50, -30, 2) + tone(1000, -30, 2), aux1_db=-32.0)
lift = (level(low, 50) - level(low, 1000)) - reference
check("20 dB below the reference: bass about 8 dB higher relative to the mids", 6.0 <= lift <= 9.5, f"{lift:+.1f} dB")
check("as cut mids, not boosted bass", level(low, 1000) < -35, f"1 kHz at {level(low, 1000):.1f} dBFS")

print("=== Subsonic: below what the driver can play")
out = run(tone(25, -12, 2) + tone(1000, -12, 2))
cut = level(out, 1000) - level(out, 25)
check("25 Hz at least 12 dB below 1 kHz", cut >= 12, f"{cut:.1f} dB")

print("All DSP config tests passed." if failures == 0 else f"{failures} DSP config tests FAILED.")
sys.exit(1 if failures else 0)
