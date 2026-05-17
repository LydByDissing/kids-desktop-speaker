Ota Updater
===========

Over-the-air update system for the Kids Desktop Speaker. Handles updates for
the **entire system** — OS, software platform, DSP configs, and voice models —
with atomic updates and automatic rollback.

.. important::

   **OTA is the #1 implementation priority.** It is the force multiplier
   for the entire project. With OTA + CI/CD in place, every new feature,
   bug fix, or config change can be pushed to the device by simply tagging
   a GitHub release. The device pulls and applies it automatically. This
   enables rapid iteration — ship early, ship often, and let the device
   update itself. Without OTA, every change requires manual SD card flashing
   and on-device tinkering, which kills velocity.

   **Implementation order**: (1) Decide OS + OTA strategy → (2) Validate on
   Pi Zero W hardware → (3) Build CI/CD pipeline → (4) Implement OTA
   service → (5) Close the loop: tag → build → deploy → update.

Operating System — Decision Pending
-------------------------------------

NixOS was initially chosen for its declarative config and atomic updates,
but **ARMv6 (Pi Zero W) support is uncertain**. The community
`nixos-rpi-zero-w`_ project (38 stars) was last updated in 2022, and
NixOS's official ARMv6 support is minimal. This needs validation before
committing.

.. _nixos-rpi-zero-w: https://github.com/cyber-murmel/nixos-rpi-zero-w

.. todo::

   **OS + OTA strategy decision needed.** Evaluate these options:

.. list-table::
   :header-rows: 1
   :widths: 15 20 30 35

   * - Option
     - OS
     - OTA Mechanism
     - Trade-offs
   * - **A**
     - NixOS
     - ``nixos-rebuild switch``
     - Declarative, atomic, rollback built in. **Risk: ARMv6 support
       uncertain.** Needs validation. If it works, this is the best option.
   * - **B**
     - Raspberry Pi OS
     - `RAUC`_ (A/B partitions)
     - ARMv6 proven. A/B partition scheme: write new image to inactive
       partition, switch bootloader, reboot. Rollback = switch back.
       RAUC has a `Pi bootloader backend`_. Not declarative — services
       managed by systemd + custom scripts.
   * - **C**
     - Raspberry Pi OS
     - `Mender`_ (A/B partitions)
     - ARMv6 proven. Similar A/B approach to RAUC. Has a managed cloud
       option or self-hosted. More complex than RAUC. Commercial backing.
   * - **D**
     - Yocto + RAUC
     - RAUC (A/B partitions)
     - Full control over image. Reproducible builds. Steep learning curve.
       Heavy build system. Best for production at scale, overkill for v1.
   * - **E**
     - Raspberry Pi OS
     - Custom script (no atomic)
     - Simplest. ``apt update`` + ``systemctl restart``. No atomic updates,
       no rollback. Risk of broken state. Only acceptable as last resort.

   .. _RAUC: https://github.com/rauc/rauc
   .. _Pi bootloader backend: https://github.com/Rtone/raspberrypi-firmware-rauc-bootloader-backend
   .. _Mender: https://github.com/mendersoftware/mender

**Recommendation**: Try **Option A (NixOS)** first — if ARMv6 boots and
runs, it's the best fit. If validation fails, fall back to **Option B
(Raspberry Pi OS + RAUC)**, which gives us atomic A/B updates on a
proven ARMv6 platform at the cost of losing NixOS's declarative config.

Technology Choice (Pending OS Decision)
----------------------------------------

The OTA service technology depends on the OS choice:

.. list-table::
   :header-rows: 1
   :widths: 20 30 50

   * - OS Decision
     - OTA Mechanism
     - Rust OTA Service Role
   * - NixOS
     - ``nixos-rebuild switch --upgrade``
     - Check GitHub Releases, download closure, trigger rebuild, manage
       reboot scheduling
   * - Pi OS + RAUC
     - RAUC A/B partition install
     - Check GitHub Releases, download image bundle, call ``rauc install``,
       manage reboot scheduling, report slot status
   * - Pi OS + Mender
     - Mender client A/B install
     - Check GitHub Releases, trigger Mender deployment, manage reboot
       scheduling

Regardless of OS choice, the Rust OTA service always:

- Checks GitHub Releases for new versions
- Downloads updates in the background
- Schedules reboots during low-activity windows
- Reports status via API Server (REST + WebSocket)
- Triggers rollback if the update fails

Previous NixOS Analysis
~~~~~~~~~~~~~~~~~~~~~~~~

The analysis below assumed NixOS. It remains valid if NixOS is confirmed.
If a different OS is chosen, the same principles apply but the
implementation differs.

If NixOS is confirmed, these benefits apply:

.. list-table::
   :header-rows: 1
   :widths: 25 75

   * - NixOS Feature
     - Benefit for this project
   * - Declarative config
     - Entire system (kernel, services, packages, configs) defined in a single
       flake. Reproducible builds. No "works on my machine" drift.
   * - Atomic upgrades
     - ``nixos-rebuild switch`` creates a new system generation atomically.
       If anything goes wrong, the previous generation is intact.
   * - Automatic rollback
     - On boot failure, NixOS automatically rolls back to the last working
       generation. No manual intervention needed.
   * - Rollback on demand
     - ``nixos-rebuild --rollback`` or selecting a previous generation at
       boot. Every generation is preserved until explicitly cleaned up.
   * - Package pinning
     - Every dependency is pinned to an exact hash in the nixpkgs lock file.
       No surprise breakage from upstream changes.
   * - Cross-compilation
     - Build the Pi Zero W image on a faster machine, transfer the closure.

NixOS on Pi Zero W
~~~~~~~~~~~~~~~~~~~

The Pi Zero W uses an **ARMv6** (armhf) CPU. NixOS supports this via the
``sd-image-raspberrypi`` target, which uses the ``linux_rpi1`` kernel and
includes explicit Pi Zero support (``enable_uart=1`` in the boot config).

A community project (`nixos-rpi-zero-w`_) demonstrates NixOS running on the
Pi Zero W (38 stars, though last updated 2022 — needs validation with current
NixOS).

.. _nixos-rpi-zero-w: https://github.com/cyber-murmel/nixos-rpi-zero-w

.. todo::

   **Validation needed**: Test current NixOS (25.05+) on Pi Zero W. The
   community project is from 2022. Verify that the ``sd-image-raspberrypi``
   target still builds for ARMv6 and boots on Pi Zero W hardware.

System Architecture
---------------------

The entire system is defined by a NixOS flake:

.. plantuml::
   :caption: NixOS Declarative System Architecture
   :name: fig-nixos-arch

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title NixOS Declarative System Architecture

   package "NixOS Flake" as flake {
       component ["API Server (Axum)"] as api
       component ["Audio Player (librespot)"] as player
       component ["Bluetooth Service (bluer)"] as bt
       component ["CamillaDSP + profiles"] as dsp
       component ["TTS Engine (Piper)"] as tts
       component ["OTA Updater"] as ota
       component ["HAT Audio Driver"] as hat
       component ["Web Interface (Leptos)"] as webui
   }

   package "OS Layer (NixOS)" as os {
       component ["Linux kernel (rpi1)"] as kernel
       component ["BlueZ"] as bluez
       component ["ALSA + loopback"] as alsa
       component ["systemd"] as systemd
       component ["NetworkManager"] as net
   }

   package "Configuration" as config {
       component ["CamillaDSP YAML profiles"] as profiles
       component ["Piper voice models"] as voices
       component ["Speaker platform config"] as platcfg
   }

   flake --> os : depends on
   flake --> config : includes

   @enduml

Everything below the "NixOS Flake" boundary is built and deployed as a single
atomic unit. A new release = a new flake commit = a new system generation.

Update Lifecycle
-----------------

.. plantuml::
   :caption: OTA Update Lifecycle
   :name: fig-ota-lifecycle

   @startuml
   title OTA Update Lifecycle

   participant "GitHub Releases" as gh
   participant "OTA Updater" as ota
   participant "API Server" as api
   participant "NixOS" as nixos
   participant "Web Interface" as ui

   == Background Check ==
   ota -> gh : Check latest release version
   gh -> ota : New version available
   ota -> api : WebSocket event: ota_status (update available)
   api -> ui : Push update notification

   == User Triggers Update ==
   ui -> api : POST /api/ota/apply
   api -> ota : Start update

   == Download & Build ==
   ota -> gh : Download release assets (nix closure)
   ota -> nixos : nixos-rebuild switch --upgrade
   note over nixos : Downloads closures, builds new generation

   == Apply ==
   nixos -> nixos : Create new system generation
   note over nixos : Atomic switch
   ota -> api : WebSocket event: ota_status (applied, reboot needed)

   == Scheduled Reboot ==
   note over ota : Wait for low-activity window
   note over ota : (or user-configured timeslot)
   ota -> nixos : Schedule reboot
   nixos -> nixos : Reboot into new generation

   == Verification ==
   alt Boot succeeds
       nixos -> ota : System healthy
       ota -> api : WebSocket event: ota_status (update complete)
       note over nixos : New generation is now default
   else Boot fails
       nixos -> nixos : Auto-rollback to previous generation
       nixos -> ota : Rolled back
       ota -> api : WebSocket event: ota_status (rollback)
   end

   @enduml

Controlled Update Process
~~~~~~~~~~~~~~~~~~~~~~~~~~

Updates follow a strict, controlled process:

1. **Release**: A new version is published to GitHub Releases. The release
   includes the Nix closure and a version manifest.
2. **Discovery**: The OTA Updater periodically checks GitHub Releases for
   new versions (or is triggered via API).
3. **Download**: Release assets are downloaded in the background. No
   interruption to playback.
4. **Build**: ``nixos-rebuild switch --upgrade`` creates a new system
   generation. This downloads pre-built closures (no on-device compilation).
5. **Apply**: The new generation is atomically activated. Services are
   restarted as needed by systemd.
6. **Schedule reboot**: If a reboot is required (kernel update, etc.), the
   OTA Updater schedules it during a **low-activity window** or a
   **user-configured timeslot** (e.g., 3:00–5:00 AM).
7. **Verify**: After reboot, the OTA Updater confirms the system is healthy.
8. **Rollback**: If the system fails to boot, NixOS automatically rolls back
   to the previous generation. The OTA Updater reports the rollback via
   the API Server.

Rollback
--------

NixOS provides **built-in rollback** at two levels:

1. **Automatic**: If a new generation fails to boot, the bootloader
   automatically falls back to the previous generation. No user action needed.
2. **Manual**: The user can roll back from the Web Interface (API call to
   ``POST /api/ota/rollback``). This triggers ``nixos-rebuild --rollback``.

Previous system generations are preserved until disk space is reclaimed.
The OTA Updater keeps the last N generations (configurable, default 3).

Low-Activity Reboot Scheduling
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Reboots are disruptive (playback stops, sources disconnect). The OTA Updater
avoids reboots during active use:

- **User-configured timeslot**: The Web Interface Config section lets the
  user set a maintenance window (e.g., "3:00 AM – 5:00 AM"). Reboots only
  happen in this window.
- **Activity detection**: The OTA Updater queries the API Server for
  playback state. No reboot while audio is playing.
- **Grace period**: After a reboot is scheduled, the API Server pushes a
  ``system_status`` WebSocket event warning all clients that a reboot is
  pending. The Web UI can show a countdown.

GitHub Releases Integration
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The OTA Updater monitors a GitHub repository's releases:

- **Version check**: ``GET /repos/{owner}/{repo}/releases/latest``
- **Asset download**: Release assets contain the Nix closure and a SHA256
  manifest for verification
- **Signing**: Releases should be signed. The OTA Updater verifies signatures
  before applying. (Signing scheme TBD — GPG, minisign, or Nix's built-in
  trust model.)

.. todo::

   Define release signing strategy. Nix's content-addressable store provides
   integrity guarantees, but we also need **authenticity** (verification that
   the release came from the project maintainers, not a MITM attacker).

What Gets Updated
-----------------

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Component
     - Updated via
   * - OS kernel, drivers
     - NixOS generation (requires reboot)
   * - All platform services (API Server, Audio Player, etc.)
     - NixOS generation (service restart by systemd)
   * - CamillaDSP YAML profiles
     - NixOS generation (included in flake) or API Server config reload
   * - Piper voice models
     - NixOS generation (included in flake)
   * - User config (radio stations, alarms, tokens)
     - NOT updated by OTA — persisted across generations in /data partition

User data (radio stations, podcast feeds, alarms, API tokens, BT pairing)
is stored on a **persistent data partition** that survives system generations.
NixOS's ``impermanence`` pattern or a separate mount point ensures user data
is not lost on update.

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol
     - Purpose
   * - API Server
     - Internal (Rust IPC)
     - Report update status, receive apply/rollback commands
   * - GitHub Releases
     - HTTPS (GitHub API)
     - Check for new versions, download release assets
   * - NixOS
     - nixos-rebuild CLI
     - Apply new system generation, rollback
   * - systemd
     - D-Bus
     - Schedule reboot, service status

Component pages will be created during the component interview phase.
