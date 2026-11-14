#!/usr/bin/env python3
"""Imports a box EQ fitted by REW into the speaker's CamillaDSP config (DSP_REQ_002, DEC-054).

Usage: dsp-import-rew.py [options] EXPORT.txt     import REW's exported filters
       dsp-import-rew.py [options] --check EXPORT.txt   only check them
       dsp-import-rew.py --clear                   remove the box EQ

EXPORT.txt is REW's "Export filter settings as text" (equaliser Generic) or
its Equalizer APO export. Peaking filters (PK) and shelves (LS/HS with REW's
fixed Q, LSC/HSC with a Q) become CamillaDSP Biquads eq_1, eq_2, ... in a
generated block of ci/device/camilladsp.yml, run on the mono signal right
after to_mono, before the bass split, so every band and the bass compressor's
side chain see them. Filters switched off or of type None are skipped; REW's
preamp is ignored (the limiter guards the level).

It refuses the export, changing nothing, when a filter is of another type or
outside the limits (options, defaults in brackets):

  --max-filters N     number of filters [10]
  --min-freq HZ       lowest centre frequency [60]
  --max-freq HZ       highest centre frequency [12000]
  --min-q / --max-q   Q of each filter [0.3 / 10]
  --max-boost DB      boost of one filter, and of all of them together
                      anywhere from 20 Hz to 20 kHz [6]
  --max-cut DB        cut of one filter [18]
  --config PATH       the config to change [ci/device/camilladsp.yml]

On import, the export is copied to ci/voicing/rew-export.txt (each iteration
is a commit); the generated block names its checksum.
"""
import argparse
import cmath
import hashlib
import math
import os
import re
import shutil
import sys

REPO = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
CONFIG = os.path.join(REPO, "ci/device/camilladsp.yml")
KEPT = os.path.join(REPO, "ci/voicing/rew-export.txt")
RATE = 48000
BEGIN = "# >>> box EQ"
END = "# <<< box EQ"
# REW's LS/HS (and Equalizer APO's) have a fixed Q of 0.71.
SHELF_Q = 0.71

LINE = re.compile(
    r"^\s*Filter\s*\d*\s*:\s*(?P<state>ON|OFF)\s+(?P<type>[A-Za-z]+)(?:\s+(?P<slope>\d+)\s*dB)?"
    r"(?:\s+Fc\s+(?P<fc>[\d.]+)\s*Hz)?(?:\s+Gain\s+(?P<gain>[-+]?[\d.]+)\s*dB)?(?:\s+Q\s+(?P<q>[\d.]+))?",
    re.IGNORECASE,
)
PREAMP = re.compile(r"^\s*Preamp\s*:\s*(?P<gain>[-+]?[\d.]+)\s*dB", re.IGNORECASE)
KINDS = {"PK": "Peaking", "LS": "Lowshelf", "LSC": "Lowshelf", "HS": "Highshelf", "HSC": "Highshelf"}


class Refused(Exception):
    pass


def parse(text):
    """The filters in a REW export, as dicts {type, freq, gain, q}, and the
    preamp (or None). Raises Refused for filters we can't place."""
    filters, preamp = [], None
    for number, line in enumerate(text.splitlines(), 1):
        if m := PREAMP.match(line):
            preamp = float(m["gain"])
            continue
        m = LINE.match(line)
        if not m:
            continue
        kind = m["type"].upper()
        if m["state"].upper() == "OFF" or kind == "NONE":
            continue
        if kind not in KINDS:
            raise Refused(f"line {number}: filter type {m['type']} can't be placed before the bass split; "
                          f"use peaking (PK) filters and shelves (LS, HS, LSC, HSC)")
        if m["slope"]:
            raise Refused(f"line {number}: {kind} {m['slope']} dB shelves aren't supported; use LS/HS or LSC/HSC")
        if m["fc"] is None or m["gain"] is None:
            raise Refused(f"line {number}: {kind} needs Fc and Gain")
        q = m["q"]
        if q is None:
            if kind in ("PK", "LSC", "HSC"):
                raise Refused(f"line {number}: {kind} needs a Q")
            q = SHELF_Q
        filters.append({"type": KINDS[kind], "freq": float(m["fc"]), "gain": float(m["gain"]), "q": float(q)})
    return filters, preamp


def coefficients(f):
    """The biquad's (b, a) as CamillaDSP designs it (the RBJ cookbook)."""
    w0 = 2 * math.pi * f["freq"] / RATE
    a_ = 10 ** (f["gain"] / 40)
    alpha = math.sin(w0) / (2 * f["q"])
    cos = math.cos(w0)
    if f["type"] == "Peaking":
        b = [1 + alpha * a_, -2 * cos, 1 - alpha * a_]
        a = [1 + alpha / a_, -2 * cos, 1 - alpha / a_]
    else:
        s = 2 * math.sqrt(a_) * alpha
        sign = 1 if f["type"] == "Lowshelf" else -1
        b = [a_ * ((a_ + 1) - sign * (a_ - 1) * cos + s),
             sign * 2 * a_ * ((a_ - 1) - sign * (a_ + 1) * cos),
             a_ * ((a_ + 1) - sign * (a_ - 1) * cos - s)]
        a = [(a_ + 1) + sign * (a_ - 1) * cos + s,
             -sign * 2 * ((a_ - 1) + sign * (a_ + 1) * cos),
             (a_ + 1) + sign * (a_ - 1) * cos - s]
    return b, a


def response_db(filters, freq):
    """The filters' combined gain in dB at `freq`."""
    z = cmath.exp(-1j * 2 * math.pi * freq / RATE)
    total = 1.0
    for f in filters:
        b, a = coefficients(f)
        total *= (b[0] + b[1] * z + b[2] * z * z) / (a[0] + a[1] * z + a[2] * z * z)
    return 20 * math.log10(abs(total))


def check(filters, limits):
    """Raises Refused naming the first limit the filters break."""
    if not filters:
        raise Refused("no filters in the export (to remove the EQ, use --clear)")
    if len(filters) > limits.max_filters:
        raise Refused(f"{len(filters)} filters; at most {limits.max_filters}")
    for i, f in enumerate(filters, 1):
        name = f"filter {i} ({f['type']} {f['freq']:g} Hz)"
        if not limits.min_freq <= f["freq"] <= limits.max_freq:
            raise Refused(f"{name}: outside {limits.min_freq:g}-{limits.max_freq:g} Hz")
        if not limits.min_q <= f["q"] <= limits.max_q:
            raise Refused(f"{name}: Q {f['q']:g} outside {limits.min_q:g}-{limits.max_q:g}")
        if f["gain"] > limits.max_boost:
            raise Refused(f"{name}: boosts {f['gain']:g} dB; at most {limits.max_boost:g}")
        if f["gain"] < -limits.max_cut:
            raise Refused(f"{name}: cuts {-f['gain']:g} dB; at most {limits.max_cut:g}")
    freqs = [20 * 1000 ** (k / 600) for k in range(601)]  # 20 Hz-20 kHz, 200 points per decade
    peak, at = max((response_db(filters, f), f) for f in freqs)
    if peak > limits.max_boost + 0.05:
        raise Refused(f"together the filters boost {peak:.1f} dB at {at:.0f} Hz; at most {limits.max_boost:g}")


def block(filters, source):
    """The generated lines: (filter lines, pipeline lines)."""
    if filters:
        digest = hashlib.sha256(source).hexdigest()[:12]
        head = (f"  {BEGIN} (DEC-054): generated by make -C ci dsp-import-rew from "
                f"ci/voicing/rew-export.txt (sha256 {digest}); don't edit by hand")
    else:
        head = f"  {BEGIN} (DEC-054): none imported yet (make -C ci dsp-import-rew)"
    lines = [head]
    for i, f in enumerate(filters, 1):
        lines.append(f"  eq_{i}: {{type: Biquad, parameters: {{type: {f['type']}, freq: {f['freq']:g}, "
                     f"gain: {f['gain']:g}, q: {f['q']:g}}}}}")
    lines.append(f"  {END}")
    names = ", ".join(f"eq_{i}" for i in range(1, len(filters) + 1))
    steps = [f"  {BEGIN}"] + ([f"  - {{type: Filter, channels: [0], names: [{names}]}}"] if filters else [])
    return lines, steps + [f"  {END}"]


def without_block(lines):
    out, inside = [], False
    for line in lines:
        if line.strip().startswith(BEGIN):
            inside = True
        elif line.strip().startswith(END):
            inside = False
        elif not inside:
            out.append(line)
    return out


def rewrite(config_text, filters, source=b""):
    """The config with the box EQ block replaced (or added)."""
    filter_lines, steps = block(filters, source)
    out = []
    for line in without_block(config_text.splitlines()):
        out.append(line)
        if line == "filters:":
            out += filter_lines
        elif line.strip() == "- {type: Mixer, name: to_mono}":
            out += steps
    if sum(line.strip().startswith(BEGIN) for line in out) != 2:
        raise Refused("the config has no 'filters:' section or to_mono step to place the EQ at")
    return "\n".join(out) + "\n"


def imported(config_text):
    """The box EQ filters in a config, parsed back from the generated block."""
    pattern = re.compile(r"^\s*eq_\d+: \{type: Biquad, parameters: \{type: (\w+), freq: ([\d.e+-]+), "
                         r"gain: ([\d.e+-]+), q: ([\d.e+-]+)\}\}")
    return [{"type": m[1], "freq": float(m[2]), "gain": float(m[3]), "q": float(m[4])}
            for line in config_text.splitlines() if (m := pattern.match(line))]


def read(path, mode="r"):
    with open(path, mode) as f:
        return f.read()


def write(path, text):
    with open(path, "w") as f:
        f.write(text)


def arguments(argv):
    p = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    p.add_argument("export", nargs="?")
    p.add_argument("--check", action="store_true")
    p.add_argument("--clear", action="store_true")
    p.add_argument("--config", default=CONFIG)
    p.add_argument("--keep", default=KEPT, help=argparse.SUPPRESS)
    p.add_argument("--max-filters", type=int, default=10)
    p.add_argument("--min-freq", type=float, default=60)
    p.add_argument("--max-freq", type=float, default=12000)
    p.add_argument("--min-q", type=float, default=0.3)
    p.add_argument("--max-q", type=float, default=10)
    p.add_argument("--max-boost", type=float, default=6)
    p.add_argument("--max-cut", type=float, default=18)
    # make -C ci runs us in ci/: a relative FILE may mean the repo's root.
    p.add_argument("--relative-to", help=argparse.SUPPRESS)
    args = p.parse_args(argv)
    if not args.clear and not args.export:
        p.error("give REW's exported filters, or --clear")
    if args.export and args.relative_to and not os.path.exists(args.export):
        other = os.path.join(args.relative_to, args.export)
        if os.path.exists(other):
            args.export = other
    return args


def main(argv=None):
    args = arguments(sys.argv[1:] if argv is None else argv)
    config_text = read(args.config)
    try:
        if args.clear:
            write(args.config, rewrite(config_text, []))
            print(f"==> Removed the box EQ from {args.config}.")
            return 0
        source = read(args.export, "rb")
        filters, preamp = parse(source.decode("utf-8", "replace"))
        check(filters, args)
        new = rewrite(config_text, filters, source)
    except Refused as e:
        print(f"error: refused {args.export or args.config}: {e}", file=sys.stderr)
        return 1
    for i, f in enumerate(filters, 1):
        print(f"  eq_{i}: {f['type']:9} {f['freq']:8g} Hz  {f['gain']:+5.1f} dB  Q {f['q']:g}")
    if preamp is not None:
        print(f"  (REW's preamp of {preamp:+.1f} dB is ignored: the limiter guards the level)")
    if args.check:
        print("==> Within the limits.")
        return 0
    write(args.config, new)
    if os.path.abspath(args.export) != os.path.abspath(args.keep):
        os.makedirs(os.path.dirname(args.keep), exist_ok=True)
        shutil.copyfile(args.export, args.keep)
    print(f"==> Imported {len(filters)} filters into {args.config}; the export is kept in {args.keep}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
