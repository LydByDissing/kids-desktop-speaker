#!/usr/bin/env python3
"""Writes a variant of the speaker's CamillaDSP config, for A/B listening.

Usage: dsp-variant.py VARIANT [config.yml] > variant.yml

Variants (each leaves one part of the bass processing out, DEC-051/052):
  installed    the config as it is
  no-vb        without the virtual bass (also what CamillaGUI 4.1 can show)
  no-duck      without the bass ducking compressor (the limiter still guards)
  no-loudness  without loudness (the Audio Player still raises the amplifier
               by the mid cut it expects, so low volumes play louder)
  no-fill      without the +6 dB bass fill
  flat         mono, the subsonic filter and the limiter only
"""
import os
import sys

import yaml

VARIANTS = ["installed", "no-vb", "no-duck", "no-loudness", "no-fill", "flat"]


def without_filter(conf, name):
    """Drops `name` from every filter step, and steps left empty."""
    steps = []
    for step in conf["pipeline"]:
        if step.get("type") == "Filter":
            step = dict(step, names=[n for n in step["names"] if n != name])
            if not step["names"]:
                continue
        steps.append(step)
    conf["pipeline"] = steps
    conf["filters"].pop(name, None)
    return conf


def variant(conf, name):
    if name == "installed":
        return conf
    if name == "no-vb":
        return without_filter(conf, "virtual_bass")
    if name == "no-duck":
        conf["pipeline"] = [
            s for s in conf["pipeline"] if not (s.get("type") == "Processor" and s.get("name") == "bass_duck")
        ]
        conf["processors"].pop("bass_duck", None)
        return conf
    if name == "no-loudness":
        return without_filter(conf, "loudness")
    if name == "no-fill":
        conf["filters"]["bass_boost"]["parameters"]["gain"] = 0.0
        conf["filters"]["boosted_view"]["parameters"]["gain"] = 0.0
        return conf
    if name == "flat":
        keep_filters = {"subsonic", "lookahead", "clipper"}
        conf["filters"] = {k: v for k, v in conf["filters"].items() if k in keep_filters}
        conf["processors"] = {"limiter": conf["processors"]["limiter"]}
        conf["mixers"] = {k: conf["mixers"][k] for k in ("to_mono", "split_limit", "to_both")}
        conf["pipeline"] = [
            {"type": "Mixer", "name": "to_mono"},
            {"type": "Filter", "channels": [0], "names": ["subsonic"]},
            {"type": "Mixer", "name": "split_limit"},
            {"type": "Filter", "channels": [0], "names": ["lookahead"]},
            {"type": "Processor", "name": "limiter"},
            {"type": "Filter", "channels": [0], "names": ["clipper"]},
            {"type": "Mixer", "name": "to_both"},
        ]
        return conf
    raise SystemExit(f"unknown variant {name!r}; one of: {', '.join(VARIANTS)}")


def main():
    if len(sys.argv) < 2 or sys.argv[1] in ("-h", "--help"):
        print(__doc__.strip())
        sys.exit(0 if len(sys.argv) > 1 else 2)
    repo = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    path = sys.argv[2] if len(sys.argv) > 2 else os.path.join(repo, "ci/device/camilladsp.yml")
    with open(path) as f:
        conf = yaml.safe_load(f)
    out = variant(conf, sys.argv[1])
    print(f"# Variant {sys.argv[1]!r} of {os.path.basename(path)}, for listening (ci/scripts/dsp-variant.py).")
    yaml.safe_dump(out, sys.stdout, sort_keys=False, default_flow_style=None, width=120)


if __name__ == "__main__":
    main()
