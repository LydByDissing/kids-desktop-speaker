# Agent Instructions — Kids Desktop Speaker

## Project Overview

The Kids Desktop Speaker is a **software platform** running on a Raspberry Pi Zero W that turns the IQaudIO Pi-Codec Zero HAT into a streaming speaker for kids and young adults. The full speaker product (enclosure, speakers, hardware) is documented separately — this repo covers the software only.

### Architecture (C4 Model)

The software platform has 8 containers:

| Container | Technology | Purpose |
|-----------|-----------|---------|
| Web Interface | Leptos WASM + Axum | Player (playback, volume, sleep timer) + Config (radio, podcasts, alarms, API tokens) |
| API Server | Axum (Rust) | REST + WebSocket API, token auth, mDNS/SSDP, hosts Leptos frontend |
| Audio Player | Rust + librespot | Audio sources, source switching, TTS ducking, sleep timer (separate process) |
| Bluetooth Service | Rust + bluer + BlueZ | AVRCP control from paired keypad/knob; A2DP under evaluation |
| CamillaDSP | Rust (external) + systemd | Master volume, EQ profiles, DSP pipeline, 2-ch loopback mixer for TTS |
| TTS Engine | Piper + piper-tts-rs | Offline neural TTS, FIFO queue, dedicated ALSA loopback channel |
| OTA Updater | Rust + NixOS | Atomic system updates via nixos-rebuild, GitHub Releases, auto-rollback |
| HAT Audio Driver | ALSA / Linux kernel / NixOS | IQaudIO Pi-Codec Zero (WM8731 codec), declaratively configured in NixOS flake |

### Tech Stack

- **Language**: Rust (primary), Nix (OS config)
- **OS**: NixOS on Raspberry Pi Zero W (ARMv6)
- **Audio pipeline**: Sources → ALSA loopback → CamillaDSP → HAT driver
- **Docs**: Sphinx + C4-PlantUML + sphinx-needs
- **Issue tracking**: bd (beads)

### Open Investigation Items

These require research or prototyping before implementation decisions can be made:

1. **Per-source ALSA channels** — Should each audio source get its own loopback channel? Depends on CamillaDSP mixer API capabilities.
2. **AirPlay receiver** — rairplay (Rust, early) vs shairport-sync (C, proven).
3. **Internet radio / podcast backend** — MPD (proven by Volumio) vs custom Rust vs GStreamer.
4. **NixOS on Pi Zero W** — ARMv6 validation. Community project from 2022 needs re-testing.
5. **Release signing** — GPG, minisign, or Nix built-in trust model.
6. **piper-tts-rs maturity** — FFI wrapper early-stage; fallback to CLI subprocess.
7. **A2DP decision** — Recommend AVRCP-only for v1; evaluate A2DP later.

## Documentation

- **Architecture**: `docs/source/architecture/` — C4 diagrams and container details
- **Requirements**: `docs/source/requirements/requirements.rst` — sphinx-needs managed
- **Interview state**: `.docs-interview.json` — tracks architecture interview progress
- **Build**: `cd docs && make html`

### Documentation Rules

- **Never use ASCII-art or stick-figure diagrams.** All diagrams must use PlantUML.
- Use C4-PlantUML macros for architecture views, plain PlantUML for technical flows.
- Always add `SHOW_LEGEND()` and `title` to C4 diagrams.
- Build and verify (`make html`) after any doc changes.

## Issue Tracking

This project uses **bd (beads)** for issue tracking.
Run `bd prime` for workflow context, or install hooks (`bd hooks install`) for auto-injection.

**Quick reference:**
- `bd ready` - Find unblocked work
- `bd create "Title" --type task --priority 2` - Create issue
- `bd close <id>` - Complete work
- `bd sync` - Sync with git (run at session end)

For full workflow details: `bd prime`

## Landing the Plane (Session Completion)

**When ending a work session**, complete ALL steps below. Work is NOT complete until changes are committed.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create bd issues for anything needing follow-up
2. **Build and verify** - `cd docs && make html` (zero warnings)
3. **Update issue status** - Close finished work, update in-progress items
4. **Commit everything** - Code, docs, and beads state:
   ```bash
   git add -A
   bd sync
   git commit -m "descriptive message"
   ```
5. **Hand off** - Provide context summary for next session
