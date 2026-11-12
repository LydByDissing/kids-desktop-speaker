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

**When it checks** (OTA_REQ_011): at every boot, then once a week
(``OTA_CHECK_INTERVAL_SECS``, default 604800), and when a user asks from the
Web UI (``POST /api/ota/check``). With many speakers, checking more often would
only load the release server. A repository with no release yet (GitHub answers
``404`` for ``releases/latest``, also when there are only pre-releases) is a
normal answer: nothing to install. A failed check (no network or clock yet just
after boot, rate limited) is retried 3 times, after 1, 2 and 4 minutes, and then
waits for the next weekly check (KDS-eio).

**Download**: The RAUC bundle is downloaded to a staging directory on the
data partition: ``/data/ota/staging/bundle.raucb``. Before it starts, the updater
checks that the bundle plus 32 MB fits; otherwise it reports "not enough space
for the update" and tries again at the next check (DEC-047). Downloads use
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
2. **Activity check**: Ask the Audio Player whether a source is active. Don't
   reboot while audio is playing (unless the user explicitly overrides). If the
   Audio Player can't be reached, nothing is playing.
3. **Grace period**: After scheduling a reboot, wait 5 minutes, then check
   activity again. Notifying WebSocket clients for a Web UI countdown is not
   built yet.
4. **Force reboot**: User can trigger an immediate reboot from the Web UI
   (bypasses maintenance window).

IPC Protocol — OTA Updater ↔ API Server
-----------------------------------------

The OTA Updater serves ``/run/kds/ota.sock`` using the shared
:doc:`IPC protocol <../ipc/index>`: newline-delimited JSON, one request and one
response per connection.

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Command
     - Effect
     - Response fields
   * - ``status``
     - None
     - ``state`` (idle, downloading, installing, done, error), ``detail``,
       ``version`` (running), ``pending`` (installed, waiting for a reboot)
   * - ``check``
     - Checks for an update now
     - None
   * - ``install``
     - Downloads and installs ``url``, verifying ``sha256``. Refused while an
       install is running
     - None
   * - ``reboot``
     - Reboots now, skipping the maintenance window
     - None

The API Server exposes these as ``GET /api/ota/status``, ``POST /api/ota/check``
and ``POST /api/ota/reboot``. Pushed events, and the Web UI countdown during the
grace period, wait for the WebSocket work in KDS-izr.

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

     **Status (2026-11-11)**: Resolved: releases are signed with a stable key in CI and verified against the keyring in the image (CI_REQ_005).

   - **GitHub API rate limiting**: Unauthenticated GitHub API requests are
     limited to 60/hour. Our 6-hour polling is fine, but on-demand checks
     from the API Server could hit the limit. Consider using a GitHub
     token for authenticated requests (5000/hour).

     **Status (2026-11-12)**: Open: checks are unauthenticated; a check at boot and weekly, at most 3 retries and manual checks stay far below 60 an hour.

   - **Bundle size**: RAUC bundles will be 100-150MB. On a Pi Zero W with
     slow SD card and limited bandwidth, download could take 10-30 minutes.
     The background download approach handles this, but we should show
     progress in the Web UI.

     **Status (2026-11-11)**: Measured: about 456 MB (KDS-9jt.5 tracks trimming MPD); progress in the Web UI comes in wave 5 (KDS-apx.7).

   - **First boot after OTA**: The ``rauc-mark-good.service`` needs to run
     on the first boot after an update. How do we distinguish "first boot
     after update" from "normal boot"? RAUC's boot attempt counter handles
     this — if the counter is > 0, it's a new slot boot.


     **Status (2026-11-11)**: Resolved: rauc-mark-good runs only when booted from a RAUC slot and marks it good once the health check passes (OTA_REQ_009).