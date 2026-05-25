Architecture Decision Records
==============================

This section captures significant architecture decisions made during the design
of the Kids Desktop Speaker software platform. Each decision includes the
context, options considered, and the rationale for the chosen path.

.. contents::
   :local:
   :depth: 2

OS & OTA Strategy
-----------------

.. _dec-001:

DEC-001 — Operating System Selection
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_001
   :status: resolved
   :date: 2026-05-25
   :tags: os, ota, infrastructure

**Decision**: Use **Raspberry Pi OS Lite (Bookworm)** as the base operating
system, with **RAUC** for atomic OTA updates.

**Context**

The OS choice determines the OTA mechanism. The platform runs on a Raspberry
Pi Zero W (ARMv6). The initial plan used NixOS for its declarative
configuration and atomic upgrades, but ARMv6 support was uncertain.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 10 20 30 40

   * - Option
     - OS
     - OTA Mechanism
     - Trade-offs
   * - A
     - NixOS
     - ``nixos-rebuild switch``
     - Declarative, atomic, rollback built in. **Risk: ARMv6 support
       uncertain.** Community project (nixos-rpi-zero-w) last updated 2022.
   * - B
     - Raspberry Pi OS
     - RAUC_ (A/B partitions)
     - **CHOSEN.** ARMv6 proven. A/B partition scheme with atomic switch.
       Rollback = switch back. Pi bootloader backend available.
   * - C
     - Raspberry Pi OS
     - Mender_ (A/B partitions)
     - ARMv6 proven. Similar A/B approach. Managed cloud option available.
       More complex than RAUC.
   * - D
     - Yocto + RAUC
     - RAUC (A/B partitions)
     - Full control over image. Reproducible builds. Steep learning curve.
       Overkill for v1.
   * - E
     - Raspberry Pi OS
     - Custom script (no atomic)
     - Simplest. No atomic updates, no rollback. Risk of broken state.

**Rationale**

NixOS (Option A) was evaluated but ARMv6 support could not be confirmed
with current NixOS releases. The community project demonstrating NixOS on
Pi Zero W dates from 2022 and has not been maintained. Without confidence
in the base OS, building OTA on top of it was too risky.

Option B (Pi OS + RAUC) was chosen because:

- ARMv6 is fully proven — Raspberry Pi OS officially supports Pi Zero W
- RAUC provides atomic A/B updates with rollback, matching the reliability
  properties we need
- The A/B partition scheme is well-understood and battle-tested on embedded
  Linux devices
- RAUC has a `Pi bootloader backend`_ for partition switching

The trade-off is losing NixOS's declarative configuration. Services will be
managed via systemd unit files and deployment scripts instead.

**Consequences**

- All container pages must be updated to reference Pi OS + systemd instead
  of NixOS
- The OTA Updater container uses ``rauc install`` instead of
  ``nixos-rebuild switch``
- The HAT Audio Driver uses standard ALSA/systemd configuration instead of
  NixOS device tree overlays
- A new RAUC configuration (``system.conf``, slot layout) must be created
- The CI pipeline must build Pi OS images, not NixOS closures

.. _RAUC: https://github.com/rauc/rauc
.. _Mender: https://github.com/mendersoftware/mender
.. _Pi bootloader backend: https://github.com/Rtone/raspberrypi-firmware-rauc-bootloader-backend

CI Pipeline
-----------

.. _dec-002:

DEC-002 — CI Pipeline Architecture
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_002
   :status: resolved
   :date: 2026-05-25
   :tags: ci, infrastructure, ota

**Decision**: Use **Docker + QEMU user-mode emulation** (Solution 1) for
the CI pipeline, and **pi-gen** (Solution 3) for producing flashable SD card
images. Drive the pipeline with **plain Docker + shell scripts** (not Dagger)
for v1.

**Context**

The CI pipeline must build and test Raspberry Pi OS images on x86_64
hardware (developer machines and GitHub Actions). No ARM hardware is
available in CI. The pipeline needs to build images, run smoke tests, and
produce RAUC bundles.

**Pipeline Architecture Options**

.. list-table::
   :header-rows: 1
   :widths: 15 25 35 25

   * - Solution
     - Approach
     - Pros
     - Cons
   * - 1
     - Docker + QEMU user-mode
     - Fast, works today, GitHub Actions compatible, full ARM userspace
     - Cannot test actual hardware (no GPIO/I2S/Wi-Fi)
   * - 2
     - QEMU system emulation (full VM)
     - Most realistic — boots real kernel, real systemd, tests bootloader
     - Slow (10-30 min), complex setup, needs kernel/dtb files
   * - 3
     - pi-gen (image factory)
     - Produces flashable SD card images, well-documented, staged builds
     - Long build times (30-60 min), steep learning curve, opinionated

**Pipeline Driver: Dagger vs Plain Docker**

The pipeline driver orchestrates the build → test → bundle steps. Two
options were evaluated (see :doc:`ci-tooling` for full analysis):

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Tool
     - Pros
     - Cons
   * - Dagger (Python SDK)
     - Programmatic pipelines, content-addressable caching, secret
       management, step-by-step observability
     - New dependency (Dagger engine + Python SDK), overkill for 3 linear
       steps, harder to debug locally, version churn history
   * - Plain Docker + shell
     - Zero new dependencies, transparent (every step is a visible docker
       command), works out of the box on GitHub Actions and developer laptops
     - No built-in caching beyond Docker layers, no structured secret
       management, shell scripts can grow messy

**Rationale**

Solution 1 (Docker + QEMU user-mode) is the primary CI mechanism because:

- It works on any x86_64 Linux host with Docker — no special packages needed
- ARM containers run via transparent QEMU user-mode emulation (``binfmt_misc``)
- Verified: arm32v7 Debian containers run on x86_64, RAUC 1.8 installs and
  operates correctly
- Build times are acceptable (~5-15 min for image build)
- GitHub Actions compatible via ``docker/setup-qemu-action``
- Smoke tests (service status, RAUC status) run inside the container

Plain Docker + shell scripts is chosen over Dagger for v1 because:

- The pipeline is only 3 linear steps (build, test, bundle) — Dagger's DAG
  scheduling and caching shine for complex multi-stage pipelines, not simple
  linear ones
- Every CI step is a visible ``docker`` command — when a build fails, reproduce
  it by pasting the command. No Dagger engine needed locally.
- Zero new dependencies — Docker is already required. No Dagger CLI, no Python
  SDK, no engine lifecycle to manage.
- Dagger can be layered on later without changing the Dockerfile or fundamental
  approach if the pipeline grows complex enough to justify it.

Solution 3 (pi-gen) is used for producing the final flashable SD card image
for initial device setup. This is a one-time-per-release task, not a
per-PR requirement.

Solution 2 (full VM) is deferred. It may be added later for validating the
RAUC A/B partition layout and bootloader integration on release tags.

**Consequences**

- A ``Dockerfile.pi-image`` will define the Pi OS image with all platform
  services
- A ``ci/Makefile`` with ``build``, ``test``, ``bundle`` targets drives the
  pipeline using plain ``docker buildx build`` and ``docker run`` commands
- A GitHub Actions workflow calls the Makefile targets; RAUC bundle is only
  produced on pushes to ``main``, not on every PR
- pi-gen integration will be added as a separate workflow for release tags
- Hardware validation (audio output, Bluetooth, Wi-Fi) still requires real
  Pi Zero W hardware
- Dagger can be introduced later if the pipeline grows beyond ~10
  interdependent steps or caching becomes a bottleneck

RAUC Partition Layout
---------------------

.. _dec-003:

DEC-003 — RAUC A/B Partition Layout
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_003
   :status: resolved
   :date: 2026-05-25
   :tags: ota, storage, infrastructure

**Decision**: Use RAUC with an A/B partition scheme for atomic system
updates with automatic rollback.

**Context**

With NixOS no longer the OS, the OTA mechanism must be RAUC. RAUC requires
an A/B partition layout: two root filesystem slots (A and B). Updates are
written to the inactive slot, and the bootloader switches to it on reboot.

**Partition Layout**

.. list-table::
   :header-rows: 1
   :widths: 20 15 25 40

   * - Partition
     - Size
     - Purpose
     - Notes
   * - boot (FAT32)
     - 256 MB
     - Bootloader, kernel, device trees
     - Shared between slots
   * - rootfs-a (ext4)
     - 1.5 GB
     - Root filesystem slot A
     - Active slot
   * - rootfs-b (ext4)
     - 1.5 GB
     - Root filesystem slot B
     - Inactive slot (update target)
   * - data (ext4)
     - Remainder
     - User data, configs, voice models
     - Persistent across updates, not touched by RAUC

**Rationale**

- A/B partitions provide atomic updates: the active slot is never modified
- Rollback is a bootloader-level operation — switch back to the previous slot
- The data partition is separate and persistent: user configs, radio stations,
  podcast feeds, alarms, API tokens, Bluetooth pairings, and Piper voice
  models survive updates
- 1.5 GB per rootfs slot is sufficient for Pi OS Lite + our services

**Consequences**

- The OTA Updater container calls ``rauc install <bundle>`` instead of
  ``nixos-rebuild switch``
- A ``system.conf`` RAUC configuration file defines the slot layout
- A ``rauc-mark-good.service`` systemd unit marks the current slot as good
  after successful boot
- The initial SD card image must be created with the A/B partition layout
  (handled by pi-gen)

Audio
-----

.. _dec-004:

DEC-004 — AirPlay Receiver
~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_004
   :status: resolved
   :date: 2026-05-25
   :tags: audio, airplay

**Decision**: Use **shairport-sync** (C) as a subprocess for v1.

**Context**

The Audio Player needs an AirPlay receiver. Two Rust libraries exist
(rairplay, shairplay) but both are early-stage. The de facto standard is
shairport-sync (C).

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 30 30

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - rairplay (Rust)
     - All-Rust stack, AP2 support
     - Early-stage (22 downloads), unproven
   * - B
     - shairplay (Rust)
     - Pure Rust
     - Very new (0 downloads), WIP fork
   * - C
     - shairport-sync (C)
     - **CHOSEN.** Battle-tested, 8 650 stars, actively maintained
     - C dependency, AP1 only

**Rationale**

shairport-sync is the only mature option. It outputs to ALSA directly,
advertises via mDNS/Bonjour automatically, and is proven on Raspberry Pi.
The Audio Player manages it as a child process.

**Consequences**

- shairport-sync is installed as a system package (not a Rust crate)
- The Audio Player starts/stops it as a child process
- shairport-sync.conf configures ALSA output to the loopback device
- Can be replaced with a Rust library later without architecture changes

.. _dec-005:

DEC-005 — Internet Radio & Podcast Backend
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_005
   :status: resolved
   :date: 2026-05-25
   :tags: audio, radio, podcast

**Decision**: Use **MPD** (Music Player Daemon) as a backend, controlled
from the Audio Player via the MPD protocol.

**Context**

The Audio Player needs to play internet radio streams and podcast episodes.
Volumio, moOde, and RuneAudio all use MPD successfully for this purpose.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 30 30

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - MPD (C daemon)
     - **CHOSEN.** Proven, handles all stream formats, playlist management
     - C dependency, separate daemon
   * - B
     - Custom Rust (rodio + stream-download)
     - Full Rust control
     - More engineering effort, format support risk on Pi Zero W
   * - C
     - GStreamer (Rust bindings)
     - Handles all formats, proven pipeline
     - Heavy dependency, complex cross-compilation

**Rationale**

MPD is proven, handles HTTP streams natively, and has a mature Rust client
crate (``mpd_client`` v1.4.1). The Audio Player controls it via the MPD
protocol on localhost:6600.

**Consequences**

- MPD runs as a separate systemd service
- MPD's data (database, playlists) lives on the data partition
- The Audio Player uses ``mpd_client`` to control playback and playlists
- Radio stations and podcast feeds are managed as MPD playlist entries

Bluetooth
---------

.. _dec-006:

DEC-006 — A2DP Audio Input
~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_006
   :status: resolved
   :date: 2026-05-25
   :tags: bluetooth, a2dp

**Decision**: **AVRCP only for v1.** A2DP audio input is deferred.

**Context**

The Bluetooth Service can support both AVRCP (remote control) and A2DP
(audio input from a phone). The Pi Zero W's BCM43430 chip supports both
profiles simultaneously.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 35 25

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - AVRCP only
     - **CHOSEN.** Simpler, lower CPU/memory overhead
     - No BT audio from phones
   * - B
     - AVRCP + A2DP
     - Full BT speaker capability
     - PulseAudio/PipeWire dependency, CPU overhead, source switching complexity

**Rationale**

AirPlay and Spotify Connect cover the primary streaming use cases. A2DP
adds significant complexity and CPU load on the already-constrained Pi Zero
W. The loopback channel (ch4-5) is reserved for future A2DP support.

**Consequences**

- No A2DP sink profile is registered in v1
- The ``loopback_bt`` ALSA channel is unused in v1
- A2DP can be added later as a feature toggle without architecture changes
- No PulseAudio/PipeWire dependency in v1
