Solution Design — Bluetooth Service
=====================================

.. meta::
   :component: Bluetooth Service
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the Bluetooth Service
component. It covers the AVRCP remote control, device pairing flow,
A2DP evaluation, and IPC protocol with the API Server.

Design Goals
------------

1. **AVRCP control** — Receive play/pause/next/prev/volume commands from a
   paired Bluetooth keypad, knob, or remote.
2. **Simple pairing** — User initiates pairing from the Web UI; the adapter
   becomes discoverable for 60 seconds.
3. **Auto-reconnect** — Paired devices reconnect automatically when in range.
4. **AVRCP-only for v1** — A2DP audio input is deferred (see decision below).

Architecture Overview
---------------------

The Bluetooth Service is a standalone Rust binary that:

1. Connects to the API Server via Unix domain socket
2. Interfaces with BlueZ via D-Bus (using the ``bluer`` crate)
3. Manages the Bluetooth adapter (discoverability, pairing)
4. Subscribes to AVRCP events and forwards them to the API Server
5. Maintains a list of paired devices

.. plantuml::
   :caption: Bluetooth Service — Internal Architecture
   :name: fig-bt-internal

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Bluetooth Service — Internal Architecture

   package "Bluetooth Service Process" as bt {
       component ["Adapter Manager"] as adapter
       component ["Pairing Controller"] as pairing
       component ["AVRCP Handler"] as avrcp
       component ["Device Registry"] as devices
   }

   interface "API Server\nUnix socket" as api
   component "BlueZ\nD-Bus" as bluez

   api --> pairing : start_pairing / stop_pairing
   api --> devices : list_devices / remove_device
   bluez --> adapter : adapter events
   bluez --> avrcp : AVRCP key events
   avrcp --> api : volume_up/down, play, pause, next, prev
   devices --> bluez : get_paired, remove
   adapter --> bluez : set_discoverable

   @enduml

A2DP Decision
-------------

.. meta::
   :decision-id: DEC_006
   :status: resolved
   :date: 2026-05-25
   :tags: bluetooth, a2dp

**Decision**: **AVRCP only for v1.** A2DP audio input is deferred.

**Rationale**:

- AirPlay and Spotify Connect already cover the primary audio streaming
  use cases (iOS and Spotify users).
- A2DP adds significant complexity: PulseAudio or PipeWire dependency,
  additional audio routing, source switching logic.
- SBC audio quality is noticeably worse than Spotify Connect (Ogg Vorbis
  320k) or AirPlay (ALAC).
- The Pi Zero W's single core is already heavily loaded (CamillaDSP 50% +
  Audio Player 40%).
- A2DP can be added later as a feature toggle without architecture changes
  — the loopback channel (ch4-5) is already reserved.

**Consequence**: The Bluetooth Service only implements AVRCP (remote control).
No A2DP sink profile is registered. The ``loopback_bt`` ALSA channel
remains unused in v1.

AVRCP Command Mapping
---------------------

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - AVRCP Command
     - API Server Action
     - Notes
   * - Play
     - ``resume`` on current source
     - Forwarded to Audio Player
   * - Pause
     - ``pause`` on current source
     - Forwarded to Audio Player
   * - Stop
     - ``stop`` on current source
     - Forwarded to Audio Player
   * - Next
     - ``next`` on current source
     - Forwarded to Audio Player
   * - Previous
     - ``prev`` on current source
     - Forwarded to Audio Player
   * - Volume Up
     - ``AdjustFaderVolume(0, +3dB)``
     - Forwarded to CamillaDSP
   * - Volume Down
     - ``AdjustFaderVolume(0, -3dB)``
     - Forwarded to CamillaDSP
   * - Fast Forward / Rewind
     - (not supported in v1)
     - Ignored

Pairing Flow
------------

1. User clicks "Pair new device" in Web UI
2. API Server sends ``start_pairing`` to Bluetooth Service
3. Bluetooth Service sets adapter discoverable for 60 seconds
4. User puts their keypad/knob in pairing mode
5. BlueZ completes pairing (Just Works — no PIN)
6. Bluetooth Service reports new paired device to API Server
7. API Server pushes ``bluetooth_status`` WebSocket event
8. Adapter returns to non-discoverable mode (after timeout or successful pair)

Device Management
-----------------

Paired devices are stored by BlueZ (in ``/var/lib/bluetooth/`` on the data
partition). The Bluetooth Service provides:

- **List paired devices** — name, address, connection status, battery level
  (if reported by the device)
- **Remove device** — unpair and disconnect
- **Auto-reconnect** — BlueZ handles this automatically for paired devices

IPC Protocol — Bluetooth Service ↔ API Server
-----------------------------------------------

Communication via Unix domain socket at
``/run/kids-desktop-speaker/bluetooth.sock``.

Length-prefixed JSON (same as other services).

**API Server → Bluetooth Service (commands)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Payload
   * - ``start_pairing``
     - ``{"timeout_secs?": 60}``
   * - ``stop_pairing``
     - ``{}``
   * - ``list_devices``
     - ``{}``
   * - ``remove_device``
     - ``{"address": "AA:BB:CC:DD:EE:FF"}``

**Bluetooth Service → API Server (events)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Event
     - Payload
   * - ``avrcp_command``
     - ``{"command": "play|pause|next|prev|volume_up|volume_down"}``
   * - ``device_paired``
     - ``{"name": "...", "address": "AA:BB:CC:DD:EE:FF"}``
   * - ``device_removed``
     - ``{"address": "AA:BB:CC:DD:EE:FF"}``
   * - ``device_connected``
     - ``{"address": "AA:BB:CC:DD:EE:FF"}``
   * - ``device_disconnected``
     - ``{"address": "AA:BB:CC:DD:EE:FF"}``
   * - ``adapter_status``
     - ``{"discoverable": bool, "powered": bool}``
   * - ``error``
     - ``{"code": "...", "message": "..."}``

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/bluetooth-service.service
   [Unit]
   Description=Kids Desktop Speaker — Bluetooth Service
   After=bluetooth.service dbus.service
   Requires=bluetooth.service dbus.service

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/bluetooth-service
   Restart=on-failure
   RestartSec=5

   RuntimeDirectory=kids-desktop-speaker
   RuntimeDirectoryMode=0755

   # Needs access to D-Bus system bus
   BusName=org.kidsdesktop.BluetoothService

   MemoryMax=32M
   CPUQuota=10%

   [Install]
   WantedBy=multi-user.target

.. note::

   ``CPUQuota=10%`` — the Bluetooth Service is mostly idle, waiting for
   D-Bus events. AVRCP event handling is trivial (forward a JSON message).
   The actual Bluetooth stack runs in the kernel and BlueZ daemon, not in
   our process.

Startup Dependencies
--------------------

.. code-block:: text

   bluetooth.service (BlueZ daemon)
   dbus.service
       └── bluetooth-service.service

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - BlueZ
     - D-Bus (system bus)
     - Adapter control, device pairing, AVRCP events
   * - API Server
     - Unix socket ``/run/kds/bluetooth.sock``
     - Forward AVRCP commands, receive pairing requests

Open Questions
--------------

.. todo::

   - **AVRCP volume granularity**: The ``±3dB`` step size for AVRCP volume
     needs testing. Some BT remotes send repeated volume events quickly —
     we may need rate limiting to avoid overwhelming CamillaDSP with
     fader commands.

     **Status (2026-11-11)**: Open: comes with the AVRCP remote in wave 6 (KDS-sgt.4).

   - **Multiple paired devices**: How many devices can be paired
     simultaneously? BlueZ supports many, but our UI should handle the
     case of multiple paired devices clearly. For v1, limit to 3 paired
     devices.

     **Status (2026-11-11)**: Open: BlueZ keeps every pairing on /data (BT_REQ_003); the Web UI lists them (wave 5).

   - **Battery level**: Some AVRCP devices report battery level via D-Bus.
     Should we expose this in the API? Low priority for v1.


     **Status (2026-11-11)**: Deferred: low priority.