Solution Design — OTA Updater
===============================

.. meta::
   :component: OTA Updater
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the OTA Updater component.
It covers the RAUC integration, update lifecycle, GitHub Releases
monitoring, reboot scheduling, and IPC protocol with the API Server.

Design Goals
------------

1. **Self-managed lifecycle** — The OTA Updater manages the full update
   process (check → download → install → reboot → verify) independently.
   The API Server only sends "check now" / "apply now" / "rollback" commands
   and receives status events.
2. **Safe updates** — RAUC A/B partitions provide atomic updates with
   automatic rollback. The data partition is never touched.
3. **Non-disruptive** — Downloads happen in the background. Reboots are
   scheduled during user-configured maintenance windows.
4. **Status transparency** — All state changes are reported to the API
   Server via IPC and pushed to the Web UI via WebSocket.

Architecture Overview
---------------------

The OTA Updater is a standalone Rust binary that:

1. Connects to the API Server via Unix domain socket
2. Periodically checks GitHub Releases for new versions
3. Downloads RAUC bundles (``.raucb``) to a staging area
4. Calls ``rauc install`` to write to the inactive slot
5. Schedules reboots during maintenance windows
6. Monitors boot success via RAUC slot status
7. Reports all state changes to the API Server

.. plantuml::
   :caption: OTA Updater — Internal Architecture
   :name: fig-ota-internal

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title OTA Updater — Internal Architecture

   package "OTA Updater Process" as ota {
       component ["Update Checker\nperiodic + on-demand"] as checker
       component ["Bundle Downloader\nbackground"] as downloader
       component ["RAUC Installer\nrauc install"] as installer
       component ["Reboot Scheduler\nmaintenance window"] as rebooter
       component ["Slot Monitor\nboot verification"] as monitor
       component ["State Machine"] as sm
   }

   interface "API Server\nUnix socket" as api
   component "GitHub Releases\nHTTPS" as gh
   component "RAUC\nD-Bus + CLI" as rauc
   component "systemd\nreboot" as systemd

   api --> sm : check_now / apply / rollback
   checker --> gh : GET /releases/latest
   checker --> sm : new_version_available
   downloader --> gh : download .raucb
   downloader --> sm : download_complete
   installer --> rauc : rauc install
   installer --> sm : install_complete
   rebooter --> systemd : schedule reboot
   monitor --> rauc : rauc status
   monitor --> sm : boot_verified / rollback_needed
   sm --> api : status events

   @enduml

Update State Machine
--------------------

.. plantuml::
   :caption: OTA Updater — State Machine
   :name: fig-ota-sm

   @startuml
   title OTA Updater — State Machine

   [*] --> Idle

   Idle --> Checking : periodic_timer / check_now
   Checking --> Idle : no_update
   Checking --> UpdateAvailable : new_version_found

   UpdateAvailable --> Downloading : auto_download / user_apply
   UpdateAvailable --> Idle : user_dismiss

   Downloading --> DownloadComplete : success
   Downloading --> UpdateAvailable : download_failed

   DownloadComplete --> Installing : auto_install / user_confirm
   Installing --> Installed : rauc_install_success
   Installing --> UpdateAvailable : rauc_install_failed

   Installed --> WaitingForReboot : schedule_reboot
   WaitingForReboot --> Rebooting : maintenance_window
   WaitingForReboot --> WaitingForReboot : not_yet

   Rebooting --> Verifying : boot_complete
   Verifying --> Updated : mark_good
   Verifying --> Rollback : boot_failed

   Rollback --> Idle : rollback_complete
   Updated --> Idle : acknowledged

   @enduml

GitHub Releases Integration
---------------------------

The OTA Updater monitors a GitHub repository's releases API:

.. code-block:: text

   GET https://api.github.com/repos/{owner}/{repo}/releases/latest

   Response:
   {
     "tag_name": "v0.2.0",
     "name": "v0.2.0",
     "body": "Release notes...",
     "assets": [
       {
         "name": "kids-desktop-speaker-v0.2.0.raucb",
         "browser_download_url": "https://github.com/.../releases/download/v0.2.0/...",
         "size": 150000000
       }
     ]
   }

**Polling interval**: Every 6 hours (configurable). Also triggered on-demand
via API Server command.

**Download**: The RAUC bundle is downloaded to a staging directory on the
data partition: ``/data/ota/staging/bundle.raucb``. Downloads use
``reqwest`` with progress reporting. Resumable downloads are not needed
for v1 (bundles are ~100-150MB).

RAUC Integration
----------------

The OTA Updater interacts with RAUC via CLI commands:

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Command
     - Purpose
   * - ``rauc status``
     - Query current slot, boot status, bundle info
   * - ``rauc install /data/ota/staging/bundle.raucb``
     - Install bundle to inactive slot
   * - ``rauc mark-good``
     - Mark current slot as good (called after successful boot)
   * - ``rauc mark-bad``
     - Mark current slot as bad (triggers rollback on next reboot)

RAUC bundle verification (signature check) is handled by RAUC itself —
the OTA Updater doesn't need to implement signing logic.

Reboot Scheduling
-----------------

Reboots are disruptive. The OTA Updater follows these rules:

1. **Maintenance window**: User-configured time range (default: 03:00–05:00).
   Only reboot within this window.
2. **Activity check**: Query API Server for playback state. Don't reboot
   while audio is playing (unless the user explicitly overrides).
3. **Grace period**: After scheduling a reboot, wait 5 minutes and notify
   all WebSocket clients. The Web UI shows a countdown.
4. **Force reboot**: User can trigger an immediate reboot from the Web UI
   (bypasses maintenance window).

IPC Protocol — OTA Updater ↔ API Server
-----------------------------------------

Communication via Unix domain socket at
``/run/kids-desktop-speaker/ota-updater.sock``.

Length-prefixed JSON (same as other services).

**API Server → OTA Updater (commands)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Payload
   * - ``check_now``
     - ``{}`` (trigger immediate update check)
   * - ``apply_update``
     - ``{"force?": false}`` (download + install + schedule reboot)
   * - ``dismiss_update``
     - ``{}`` (dismiss available update notification)
   * - ``rollback``
     - ``{}`` (mark current slot bad, schedule reboot)
   * - ``set_maintenance_window``
     - ``{"start": "03:00", "end": "05:00"}``
   * - ``get_status``
     - ``{}`` (return current state, version, slot info)

**OTA Updater → API Server (events)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Event
     - Payload
   * - ``update_available``
     - ``{"version": "v0.2.0", "release_notes": "..."}``
   * - ``download_progress``
     - ``{"percent": u32, "bytes_downloaded": u64, "total_bytes": u64}``
   * - ``download_complete``
     - ``{"version": "v0.2.0", "bundle_path": "..."}``
   * - ``install_complete``
     - ``{"version": "v0.2.0", "slot": "B"}``
   * - ``reboot_scheduled``
     - ``{"at": "2026-05-26T03:00:00Z", "reason": "ota"}``
   * - ``reboot_imminent``
     - ``{"seconds_remaining": 300}`` (grace period warning)
   * - ``update_complete``
     - ``{"version": "v0.2.0", "previous_version": "v0.1.0"}``
   - ``rollback_initiated``
     - ``{"from_version": "v0.2.0", "to_version": "v0.1.0"}``
   * - ``error``
     - ``{"code": "...", "message": "..."}``

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/ota-updater.service
   [Unit]
   Description=Kids Desktop Speaker — OTA Updater
   After=network-online.target
   Wants=network-online.target

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/ota-updater
   Restart=on-failure
   RestartSec=30

   RuntimeDirectory=kids-desktop-speaker
   RuntimeDirectoryMode=0755

   MemoryMax=64M
   CPUQuota=10%

   [Install]
   WantedBy=multi-user.target

.. note::

   ``Wants=network-online.target`` — the OTA Updater needs network access
   to check GitHub Releases. It should not start until the network is up.
   ``RestartSec=30`` — longer restart delay because this service is not
   critical for immediate operation.

   The ``rauc-mark-good.service`` (from the RAUC config) runs after
   successful boot to mark the current slot as good. It is a separate
   systemd unit, not managed by the OTA Updater.

Startup Dependencies
--------------------

.. code-block:: text

   network-online.target
       └── ota-updater.service

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - API Server
     - Unix socket ``/run/kds/ota-updater.sock``
     - Receive commands, send status events
   * - GitHub Releases
     - HTTPS (GitHub API)
     - Check for updates, download bundles
   * - RAUC
     - CLI (``rauc`` command)
     - Install bundles, query slot status, mark good/bad
   * - systemd
     - D-Bus / CLI
     - Schedule reboot

Open Questions
--------------

.. todo::

   - **Signing strategy (KDS-8oi)**: RAUC bundles must be signed. The
     signing key management (GPG vs minisign, CI signing workflow, key
     storage) is a separate investigation. For now, the design assumes
     RAUC handles verification — we just need to provide the public key
     in the RAUC keyring on the device.

   - **GitHub API rate limiting**: Unauthenticated GitHub API requests are
     limited to 60/hour. Our 6-hour polling is fine, but on-demand checks
     from the API Server could hit the limit. Consider using a GitHub
     token for authenticated requests (5000/hour).

   - **Bundle size**: RAUC bundles will be 100-150MB. On a Pi Zero W with
     slow SD card and limited bandwidth, download could take 10-30 minutes.
     The background download approach handles this, but we should show
     progress in the Web UI.

   - **First boot after OTA**: The ``rauc-mark-good.service`` needs to run
     on the first boot after an update. How do we distinguish "first boot
     after update" from "normal boot"? RAUC's boot attempt counter handles
     this — if the counter is > 0, it's a new slot boot.
