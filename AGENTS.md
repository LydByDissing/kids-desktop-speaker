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
1. ~~OS + OTA strategy~~ — `KDS-d5m` ✅ (`DEC-001`): Pi OS + RAUC chosen
2. ~~OS validation~~ — `KDS-eeu` ✅: Pi OS proven on ARMv6
3. Build RAUC partition layout and system config — `KDS-4vc`
4. Implement CI pipeline (Docker + Makefile + GitHub Actions) — `KDS-58k`
5. Implement the OTA Updater service (Rust + RAUC) — `KDS-3zt`
6. Integrate pi-gen for flashable SD card images — `KDS-zme`
7. Close the loop: tag → CI builds → device pulls → atomic update

**Key decisions:**
- **DEC-001**: Raspberry Pi OS Lite (Bookworm) + RAUC. NixOS abandoned (ARMv6 uncertain).
- **DEC-002**: CI pipeline uses Docker + QEMU user-mode emulation, driven by plain Docker + shell scripts (not Dagger). pi-gen for SD card images.
- **DEC-003**: RAUC A/B partition layout — boot (256MB), rootfs-a/b (1.5GB each), data (remainder).

### Open Investigation Items

1. **CI pipeline implementation** [P0] — Dockerfile + Makefile + GitHub Actions. `KDS-58k`
2. **AirPlay receiver** [P1] — rairplay (Rust, early) vs shairport-sync (C, proven). `KDS-oab`
3. **Internet radio / podcast backend** [P1] — MPD (proven by Volumio) vs custom Rust vs GStreamer. `KDS-u47`
4. **Release signing** [P2] — GPG or minisign for RAUC bundle signing. `KDS-8oi`
5. **piper-tts-rs maturity** [P2] — FFI wrapper early-stage; fallback to CLI subprocess. `KDS-m6q`
6. **A2DP decision** [P3] — Recommend AVRCP-only for v1; evaluate A2DP later. `KDS-1hn`

### Open Implementation Items

- **OTA Updater service** [P0] — Rust + RAUC. `KDS-3zt`
- **GitHub Actions CI/CD** [P0] — Build Pi OS image + RAUC bundle on tag. `KDS-4vc`
- **pi-gen integration** [P1] — Flashable SD card images for initial setup. `KDS-zme`

### Resolved

- **OS + OTA strategy** — Pi OS + RAUC chosen. NixOS abandoned. `KDS-d5m` ✅ (`DEC-001`)
- **NixOS flake** — No longer needed. `KDS-eeu` ✅ (`DEC-001`)
- **Per-source volume** — CamillaDSP 5 faders + Volume filters confirmed. `KDS-vdp` ✅
  - 6-channel snd-aloop: ch0-1 main, ch2-3 TTS, ch4-5 Bluetooth
  - TTS ducking via SetFaderVolume(Main, -20dB) — runtime only, no config reload
  - Source switching by writing to ch0-1, CamillaDSP config unchanged

## Documentation

- **Architecture**: `docs/source/architecture/` — C4 diagrams and container details
- **Requirements**: `docs/source/requirements/requirements.rst` — sphinx-needs managed
- **Decisions**: `docs/source/development/decisions.rst` — ADRs (DEC-001 through DEC-003)
- **CI Pipeline**: `docs/source/development/ci-pipeline.rst` — Solution 1/2/3 overview
- **CI Tooling**: `docs/source/development/ci-tooling.rst` — Dagger vs plain Docker evaluation
- **Interview state**: `.docs-interview.json` — tracks architecture interview progress
- **Build**: `cd docs && make html`
- **Build rule**: `make html` uses `-W --keep-going` — zero warnings required

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
2. **Capture decisions in docs** - For every significant decision, change, or new feature:
   - **ADRs**: Create or update entries in `docs/source/development/decisions.rst` (DEC-XXX)
   - **Requirements**: Update `docs/source/requirements/requirements.rst` — add/modify requirements linked to decisions
   - **Implementation notes**: Update relevant container pages in `docs/source/architecture/` and developer guides in `docs/source/development/`
   - **Container diagrams**: Update C4 diagrams if architecture changed
3. **Build and verify** - `cd docs && make html` (zero warnings — `-W --keep-going` is set)
4. **Update issue status** - Close finished work via `bd close`, update in-progress items
5. **Commit everything** - Code, docs, and beads state:
   ```bash
   git add -A
   bd sync
   git commit -m "descriptive message"
   ```
6. **Hand off** - Provide context summary for next session
