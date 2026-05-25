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
| OTA Updater | Rust + RAUC | Atomic system updates via RAUC A/B partitions, GitHub Releases, auto-rollback |
| HAT Audio Driver | ALSA / Linux kernel / systemd | IQaudIO Pi-Codec Zero (WM8731 codec), configured via device tree overlay |

### Tech Stack

- **Language**: Rust (primary), Python (build scripts)
- **OS**: Raspberry Pi OS Lite (Bookworm) on Raspberry Pi Zero W (ARMv6)
- **OTA**: RAUC with A/B partitions
- **Audio pipeline**: Sources → ALSA loopback → CamillaDSP → HAT driver
- **Docs**: Sphinx + C4-PlantUML + sphinx-needs
- **Issue tracking**: bd (beads)

### Implementation Priority: OTA First

**OTA is the force multiplier.** With OTA + CI/CD in place, every change ships by tagging a release — the device pulls and applies it automatically. This must be working before any other feature implementation.

**Implementation order:**
1. Decide OS + OTA strategy (NixOS vs Pi OS + RAUC) — `KDS-d5m`
2. Validate chosen OS on Pi Zero W hardware — `KDS-d5m`
3. Build system config (NixOS flake or Pi OS image) — `KDS-eeu`
4. Set up GitHub Actions CI/CD (build + release on tag) — `KDS-4vc`
5. Implement the OTA Updater service — `KDS-3zt`
6. Close the loop: tag → CI builds → device pulls → atomic update

**OS decision**: Raspberry Pi OS Lite (Bookworm) + RAUC (`DEC-001`_). NixOS was evaluated but ARMv6 support could not be confirmed. RAUC provides atomic A/B updates with rollback on a proven ARMv6 platform.

### Open Investigation Items

1. ~~NixOS on Pi Zero W~~ [P0] — **RESOLVED**: NixOS abandoned due to uncertain ARMv6 support. Using Pi OS + RAUC. `KDS-d5m` ✅ (`DEC-001`)
2. **AirPlay receiver** [P1] — rairplay (Rust, early) vs shairport-sync (C, proven). `KDS-oab`
3. **Internet radio / podcast backend** [P1] — MPD (proven by Volumio) vs custom Rust vs GStreamer. `KDS-u47`
4. **Release signing** [P2] — GPG or minisign for RAUC bundle signing. `KDS-8oi`
5. **piper-tts-rs maturity** [P2] — FFI wrapper early-stage; fallback to CLI subprocess. `KDS-m6q`
6. **A2DP decision** [P3] — Recommend AVRCP-only for v1; evaluate A2DP later. `KDS-1hn`

### Resolved

- **Per-source volume** — CamillaDSP 5 faders + Volume filters confirmed. `KDS-vdp` ✅
  - 6-channel snd-aloop: ch0-1 main, ch2-3 TTS, ch4-5 Bluetooth
  - TTS ducking via SetFaderVolume(Main, -20dB) — runtime only, no config reload
  - Source switching by writing to ch0-1, CamillaDSP config unchanged

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
