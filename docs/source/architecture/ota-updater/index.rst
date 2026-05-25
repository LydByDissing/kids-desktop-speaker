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

Operating System & OTA Mechanism
---------------------------------

.. note::

   **Decision**: Raspberry Pi OS Lite (Bookworm) + RAUC_ with A/B partitions
   (:ref:`DEC-001 <dec-001>`). NixOS was evaluated but ARMv6 support could not be confirmed.

The OTA system uses **RAUC** (Robust Auto-Update Controller) with an A/B
partition scheme. Two root filesystem slots (A and B) hold alternate system
images. Updates are written to the inactive slot, verified, and activated by
switching the bootloader. If the new image fails to boot, the bootloader
automatically falls back to the previous slot.

.. _RAUC: https://github.com/rauc/rauc

OS + OTA Options Considered
~~~~~~~~~~~~~~~~~~~~~~~~~~~

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
     - RAUC (A/B partitions)
     - **CHOSEN.** ARMv6 proven. A/B partition scheme with atomic switch.
       Rollback = switch back. Pi bootloader backend available.
   * - C
     - Raspberry Pi OS
     - Mender (A/B partitions)
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

.. _RAUC: https://github.com/rauc/rauc
.. _Pi bootloader backend: https://github.com/Rtone/raspberrypi-firmware-rauc-bootloader-backend
.. _Mender: https://github.com/mendersoftware/mender


Partition Layout
~~~~~~~~~~~~~~~~

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
     - Persistent across updates

See :ref:`DEC-003 <dec-003>` for the full partition layout decision.

System Architecture
---------------------

The OTA Updater is a Rust service that orchestrates the RAUC update process:

.. plantuml::
   :caption: OTA System Architecture — RAUC A/B Slots
   :name: fig-ota-arch

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title OTA System Architecture — RAUC A/B Slots

   package "SD Card" as sd {
       rectangle "boot (FAT32)" as boot
       rectangle "rootfs-a (ext4)" as roota
       rectangle "rootfs-b (ext4)" as rootb
       rectangle "data (ext4)" as data
   }

   package "Software Platform" as platform {
       component ["OTA Updater\n(Rust)"] as ota
       component ["API Server\n(Axum)"] as api
       component ["All other services\n(systemd units)"] as services
   }

   component ["GitHub Releases\n(.raucb artifact)"] as gh
   component ["RAUC\n(systemd service)"] as rauc

   ota --> gh : download .raucb
   ota --> rauc : rauc install
   rauc --> roota : write to inactive slot
   rauc --> rootb : write to inactive slot
   rauc --> boot : update bootloader flag
   ota --> api : status via IPC
   services --> data : read/write user data

   @enduml

Update Lifecycle
-----------------

.. plantuml::
   :caption: OTA Update Lifecycle — RAUC
   :name: fig-ota-lifecycle

   @startuml
   title OTA Update Lifecycle — RAUC

   participant "GitHub Releases" as gh
   participant "OTA Updater" as ota
   participant "API Server" as api
   participant "RAUC" as rauc
   participant "Web Interface" as ui

   == Background Check ==
   ota -> gh : Check latest release version
   gh -> ota : New version available
   ota -> api : WebSocket event: ota_status (update available)
   api -> ui : Push update notification

   == User Triggers Update ==
   ui -> api : POST /api/ota/apply
   api -> ota : Start update

   == Download & Install ==
   ota -> gh : Download .raucb bundle
   ota -> rauc : rauc install image.raucb
   rauc -> rauc : Write to inactive slot (rootfs-b)
   rauc -> rauc : Verify bundle signature
   rauc -> rauc : Update bootloader flag (tryboot)

   == Apply ==
   ota -> api : WebSocket event: ota_status (applied, reboot needed)

   == Scheduled Reboot ==
   note over ota : Wait for low-activity window
   note over ota : (or user-configured timeslot)
   ota -> rauc : Schedule reboot
   rauc -> rauc : Reboot into new slot

   == Verification ==
   alt Boot succeeds
       rauc -> rauc : Mark new slot as good
       ota -> api : WebSocket event: ota_status (update complete)
   else Boot fails
       rauc -> rauc : Auto-rollback to previous slot
       ota -> api : WebSocket event: ota_status (rollback)
   end

   @enduml

Controlled Update Process
~~~~~~~~~~~~~~~~~~~~~~~~~~

Updates follow a strict, controlled process:

1. **Release**: A new version is published to GitHub Releases. The release
   includes a RAUC bundle (``.raucb``) and a version manifest.
2. **Discovery**: The OTA Updater periodically checks GitHub Releases for
   new versions (or is triggered via API).
3. **Download**: The RAUC bundle is downloaded in the background. No
   interruption to playback.
4. **Install**: ``rauc install <bundle>`` writes the new image to the
   inactive rootfs slot. RAUC verifies the bundle signature before writing.
5. **Schedule reboot**: If a reboot is required, the OTA Updater schedules it
   during a **low-activity window** or a **user-configured timeslot**
   (e.g., 3:00–5:00 AM).
6. **Reboot**: The system reboots into the new slot. The bootloader checks
   the boot attempt counter.
7. **Verify**: After successful boot, ``rauc-mark-good.service`` marks the
   new slot as good via ``rauc status mark-good``.
8. **Rollback**: If the system fails to boot (boot attempt counter exceeded),
   the bootloader automatically switches back to the previous slot.

Rollback
--------

RAUC provides **built-in rollback** at the bootloader level:

1. **Automatic**: If a new slot fails to boot (attempt counter exceeded),
   the bootloader automatically falls back to the previous slot. No user
   action needed.
2. **Manual**: The user can roll back from the Web Interface (API call to
   ``POST /api/ota/rollback``). This triggers ``rauc status mark-bad`` on
   the current slot and schedules a reboot.

The **data partition** is never touched by RAUC. User data survives all
updates and rollbacks.

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
- **Asset download**: Release assets contain the RAUC bundle (``.raucb``)
- **Signing**: RAUC bundles are signed. RAUC verifies signatures before
  installing. (Signing scheme TBD — GPG or minisign.)

.. todo::

   Define release signing strategy. RAUC supports both GPG and CMS signing.
   Need to decide on key management and CI signing workflow.

What Gets Updated
-----------------

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Component
     - Updated via
   * - OS kernel, drivers
     - RAUC bundle (requires reboot)
   * - All platform services (API Server, Audio Player, etc.)
     - RAUC bundle (service restart by systemd)
   * - CamillaDSP YAML profiles
     - RAUC bundle (included in rootfs) or API Server config reload
   * - Piper voice models
     - RAUC bundle (included in rootfs) or data partition download
   * - User config (radio stations, alarms, tokens)
     - NOT updated by OTA — persisted on /data partition

User data (radio stations, podcast feeds, alarms, API tokens, BT pairing)
is stored on a **persistent data partition** that is never touched by RAUC.
This ensures user data survives all updates and rollbacks.

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
     - Check for new versions, download RAUC bundles
   * - RAUC
     - D-Bus / CLI
     - Install bundles, query slot status, mark slots good/bad
   * - systemd
     - D-Bus
     - Schedule reboot, service status

Component pages will be created during the component interview phase.
