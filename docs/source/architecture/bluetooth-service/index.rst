Bluetooth Service
=================

Bluetooth interface for the Kids Desktop Speaker. Provides **AVRCP remote
control** (play/pause, volume from a paired keypad or push knob) and
optionally **A2DP audio input** (receive audio from a phone over Bluetooth).


.. toctree::
   :maxdepth: 2

   solution-design

Technology Choice
-----------------

- **Bluetooth stack**: BlueZ (Linux standard, pre-installed on Raspberry Pi OS)
- **Rust interface**: `zbus`_ (pure-Rust D-Bus) for the pairing agent and
  adapter properties; ``bluetoothctl --agent=off`` for AVRCP events (DEC-035)
- **Audio backend**: BlueALSA (``bluealsa`` A2DP sink; ``bluealsa-aplay``
  started by the Audio Player into the ``speaker`` device, DEC-033, DEC-035)
- **Process**: Standalone Rust binary, communicates with the API Server over IPC

.. _zbus: https://crates.io/crates/zbus

Pairing (BT_REQ_002)
--------------------

The Bluetooth Service registers a BlueZ agent (``org.bluez.Agent1``,
``NoInputNoOutput``) as the default agent. ``pair`` over IPC
(``POST /api/bluetooth/pair``) opens a pairing window: the adapter becomes
discoverable and pairable for 180 seconds, and the agent accepts pairing
requests and marks each new device trusted. Outside the window it rejects
pairing and authorisation requests from unknown devices. Trusted devices can
reconnect at any time. Pairings live in ``/data/bluetooth``, bind-mounted over
``/var/lib/bluetooth`` for bluetoothd, so they survive updates (BT_REQ_003).

The adapter reports the loudspeaker device class (``Class = 0x240414``), which
iOS needs before it offers the speaker for audio.

Bluetooth Profiles
------------------

AVRCP — Remote Control (Primary)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

The primary use case: a paired Bluetooth keypad, push knob, or remote sends
playback commands to the speaker.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - AVRCP Feature
     - Behaviour
   * - Play / Pause
     - Toggle playback on the active source
   * - Next / Previous
     - Skip track in the current queue
   * - Volume Up / Down
     - Adjust master volume via CamillaDSP

The Bluetooth Service receives AVRCP events from BlueZ and forwards them to
the API Server, which routes them to the Audio Player.

A2DP — Audio Input (Optional, Under Evaluation)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Whether to enable A2DP audio input (the speaker acts as a Bluetooth speaker
that receives audio from a phone) depends on hardware and CPU constraints.

**Can the Pi Zero W do both AVRCP + A2DP?**

Yes, technically. The Pi Zero W's BCM43430 chip supports Bluetooth 4.1 Classic
with both A2DP and AVRCP profiles simultaneously. BlueZ registers both
profiles, and a single paired phone uses A2DP for audio + AVRCP for control
as part of the same connection. This is how all Bluetooth speakers work.

**CPU impact on Pi Zero W:**

- SBC decoding (the mandatory A2DP codec) is very lightweight — roughly 5–10%
  of a single Pi Zero W core
- More advanced codecs (AAC, aptX, LDAC) require significantly more CPU;
  SBC-only is the safe choice for Pi Zero W
- The Pi Zero W would also need PulseAudio or PipeWire running to handle the
  A2DP sink audio routing — this adds memory and CPU overhead

**Is it worth it?**

.. list-table::
   :header-rows: 1
   :widths: 15 40 45

   * - Aspect
     - For A2DP
     - Against A2DP
   * - Use case
     - Android phones (no AirPlay); any app
     - AirPlay and Spotify Connect already cover iOS/Spotify
   * - Audio quality
     - SBC is adequate for a small speaker
     - SBC is noticeably worse than Spotify Connect (Ogg Vorbis 320k) or AirPlay (ALAC)
   * - CPU
     - SBC decode is lightweight
     - PulseAudio/PipeWire overhead; Pi Zero W is already running 8 containers
   * - Complexity
     - BlueZ + PulseAudio handle most of it
     - Another audio source to manage; source switching logic gets more complex

.. todo::

   **Decision needed**: Enable A2DP audio input?

   - **Option A**: AVRCP only (control only). Simpler, lower overhead. User
     controls the speaker with a BT keypad/knob.
   - **Option B**: AVRCP + A2DP. Full BT speaker capability. Adds audio
     routing complexity and PulseAudio/PipeWire dependency.
   - **Recommendation**: Start with **AVRCP only** for v1. A2DP can be added
     later as a feature toggle without architecture changes.

Pairing Flow
-------------

When a user initiates pairing from the Web Interface:

1. User clicks "Pair new device" in Config → Bluetooth
2. API Server forwards to Bluetooth Service
3. Bluetooth Service makes the adapter **discoverable** for 60 seconds
   (configurable timeout)
4. User puts their keypad/knob/phone into pairing mode
5. BlueZ completes pairing (no PIN by default — **Just Works** pairing)
6. Bluetooth Service reports the new paired device to the API Server
7. WebSocket pushes ``bluetooth_status`` event to Web Interface
8. Adapter returns to non-discoverable mode

**Security**: Just Works pairing (no PIN) is standard for speakers and
headphones. Since the speaker is on a local network with no sensitive data,
this is acceptable. If needed, pairing can be restricted to a physical button
press on the speaker itself.

Paired Device Management
-------------------------

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Operation
     - Description
   * - List paired devices
     - Show all paired devices with name, address, and connection status
   * - Remove device
     - Unpair and disconnect a device
   * - Enter pairing mode
     - Make adapter discoverable for 60 seconds
   * - Auto-reconnect
     - Paired devices auto-reconnect when in range (BlueZ handles this)

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol
     - Purpose
   * - BlueZ
     - D-Bus
     - Adapter control, device pairing, AVRCP event subscription, A2DP sink
   * - API Server
     - Internal (Rust IPC)
     - Report paired devices, forward AVRCP commands, pairing requests

Component pages will be created during the component interview phase.

C4 Components
-------------

The components as implemented (KDS-9ym, 2026-11-11).

.. plantuml::
   :caption: Components — Bluetooth Service

   @startuml
   !include ../../c4/C4_Component.puml
   LAYOUT_TOP_DOWN()

   Container_Boundary(bts, "Bluetooth Service") {
       Component(agent, "Pairing agent", "agent.rs (zbus)", "org.bluez.Agent1: accepts and trusts devices only while the pairing window is open")
       Component(window, "Pairing window", "pairing.rs", "Open for 180 s on request")
       Component(bluez, "BlueZ client", "bluez.rs (zbus)", "Registers the agent, opens pairing, AVRCP Pause/Play, watches transport state")
       Component(avrcp, "AVRCP monitor", "controller.rs", "bluetoothctl --agent=off: remote-control events")
       Component(ipcb, "IPC server", "ipc.rs", "status, pair, avrcp")
   }
   System_Ext(bluezd, "BlueZ", "bluetoothd over D-Bus")
   System_Ext(audio, "Audio Player", "audio.sock")
   Rel(ipcb, window, "pair")
   Rel(ipcb, bluez, "pair, avrcp")
   Rel(agent, window, "is it open?")
   Rel(bluezd, agent, "RequestConfirmation, AuthorizeService")
   Rel(bluez, bluezd, "D-Bus")
   Rel(bluez, audio, "source_event bluetooth")
   @enduml
