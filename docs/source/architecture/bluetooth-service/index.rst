Bluetooth Service
=================

Bluetooth interface for the Kids Desktop Speaker. Provides **AVRCP remote
control** (play/pause, volume from a paired keypad or push knob) and
optionally **A2DP audio input** (receive audio from a phone over Bluetooth).

Technology Choice
-----------------

- **Bluetooth stack**: BlueZ (Linux standard, pre-installed on Raspberry Pi OS)
- **Rust interface**: `bluer`_ (v0.17.4, 2.4 M downloads, official Rust BlueZ wrapper)
- **Audio backend** (if A2DP enabled): PipeWire or PulseAudio with BlueZ A2DP sink profile
- **Process**: Standalone Rust binary, communicates with API Server via internal API

.. _bluer: https://crates.io/crates/bluer

The ``bluer`` crate wraps BlueZ's D-Bus interface, giving full access to
adapter management, device pairing, and profile registration. It is the most
mature Rust Bluetooth crate for Linux by a wide margin.

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
