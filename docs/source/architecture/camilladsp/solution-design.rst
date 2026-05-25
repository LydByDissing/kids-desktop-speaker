Solution Design — CamillaDSP
==============================

.. meta::
   :component: CamillaDSP
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the CamillaDSP component.
It covers the DSP pipeline configuration, volume control architecture,
TTS ducking mechanism, profile management, and systemd integration.

Design Goals
------------

1. **Stable pipeline** — CamillaDSP config never changes at runtime. Source
   switching is handled at the ALSA level, not by reloading DSP config.
2. **Per-source volume** — 5 independent faders (Main + Aux1–Aux4) with
   smooth ramping, controlled via WebSocket.
3. **TTS ducking** — Lower the Main fader when TTS plays. No config reload.
4. **Profile switching** — Hot-reload DSP profiles (EQ, crossover) without
   restarting the service.
5. **Internal only** — WebSocket binds to localhost, never exposed on LAN.

Architecture Overview
---------------------

CamillaDSP runs as a **systemd service** that:

1. Reads its pipeline config from ``/etc/camilladsp/config.yml``
2. Captures 6 channels from the ALSA loopback device
3. Routes each channel pair through a Volume filter + fader
4. Mixes all channels to stereo
5. Applies DSP (EQ, crossover)
6. Outputs to the HAT hardware (``hw:0,0``)
7. Listens on ``127.0.0.1:1234`` for WebSocket control commands

Pipeline Configuration
----------------------

The CamillaDSP pipeline config (``/etc/camilladsp/config.yml``):

.. code-block:: yaml

   # /etc/camilladsp/config.yml
   # Kids Desktop Speaker — CamillaDSP Pipeline
   # ============================================
   # 6-channel capture from ALSA loopback:
   #   ch0-1: Main audio (Spotify/AirPlay/Radio/Podcasts)
   #   ch2-3: TTS Engine
   #   ch4-5: Bluetooth A2DP (if enabled)
   #
   # Pipeline: Volume filter per channel pair → Mixer → DSP → Output

   devices:
     samplerate: 48000
     chunksize: 1024
     queuelimit: 4
     silence_threshold: 0
     silence_timeout: 0

     capture:
       type: Alsa
       channels: 6
       device: "hw:Loopback,0"
       format: S32_LE

     playback:
       type: Alsa
       channels: 2
       device: "hw:0,0"
       format: S32_LE

   filters:
     # Main audio volume (ch0-1)
     main_volume:
       type: Volume
       parameters:
         ramp_time: 400

     # TTS volume (ch2-3)
     tts_volume:
       type: Volume
       parameters:
         ramp_time: 200

     # Bluetooth A2DP volume (ch4-5)
     bt_volume:
       type: Volume
       parameters:
         ramp_time: 400

     # Master output volume
     master_volume:
       type: Volume
       parameters:
         ramp_time: 400

   mixers:
     to_stereo:
       channels:
         in: 6
         out: 2
       mapping:
         - dest: 0
           sources:
             - channel: 0
               gain: 0.0
               inverted: false
               scale: dB
             - channel: 2
               gain: 0.0
               inverted: false
               scale: dB
             - channel: 4
               gain: 0.0
               inverted: false
               scale: dB
         - dest: 1
           sources:
             - channel: 1
               gain: 0.0
               inverted: false
               scale: dB
             - channel: 3
               gain: 0.0
               inverted: false
               scale: dB
             - channel: 5
               gain: 0.0
               inverted: false
               scale: dB

   pipeline:
     - type: Filter
       channel: 0
       names:
         - main_volume
     - type: Filter
       channel: 1
       names:
         - main_volume
     - type: Filter
       channel: 2
       names:
         - tts_volume
     - type: Filter
       channel: 3
       names:
         - tts_volume
     - type: Filter
       channel: 4
       names:
         - bt_volume
     - type: Filter
       channel: 5
       names:
         - bt_volume
     - type: Mixer
       name: to_stereo
     - type: Filter
       channel: 0
       names:
         - master_volume
     - type: Filter
       channel: 1
       names:
         - master_volume

   # WebSocket control interface (localhost only)
   websocket:
     enabled: true
     bind_address: "127.0.0.1"
     port: 1234

Fader Mapping
-------------

CamillaDSP's built-in faders map to our Volume filters:

.. list-table::
   :header-rows: 1
   :widths: 15 25 60

   * - Fader Index
     - Fader Name
     - Controls
   * - 0
     - Main
     - ``main_volume`` filter (ch0-1) — Spotify/AirPlay/Radio/Podcasts
   * - 1
     - Aux1
     - ``tts_volume`` filter (ch2-3) — TTS Engine
   * - 2
     - Aux2
     - ``bt_volume`` filter (ch4-5) — Bluetooth A2DP
   * - 3
     - Aux3
     - (reserved)
   * - 4
     - Aux4
     - (reserved)

The master volume is controlled via CamillaDSP's built-in ``SetVolume``
command (separate from faders). This is the "final output level" after
mixing.

TTS Ducking Mechanism
---------------------

TTS ducking uses fader commands at runtime — no config reload:

1. API Server receives TTS request
2. API Server sends ``SetFaderVolume(0, -20.0)`` via WebSocket to CamillaDSP
3. CamillaDSP ramps the main fader down over 400ms (configurable)
4. TTS Engine writes audio to loopback ch2-3 at normal level
5. When TTS completes, API Server sends ``SetFaderVolume(0, 0.0)``
6. CamillaDSP ramps the main fader back up over 400ms

The ducking depth (-20 dB) and ramp time (400ms) are configurable per
profile. The API Server stores the pre-ducking fader level and restores
it exactly.

DSP Profiles
------------

DSP profiles are stored as separate YAML files in
``/etc/camilladsp/profiles/``:

.. code-block:: text

   /etc/camilladsp/
   ├── config.yml                  # Active config (symlink)
   └── profiles/
       ├── flat.yml                # Flat response, no EQ
       ├── bass_boost.yml          # Enhanced bass for small speaker
       ├── night_mode.yml          # Compressed dynamics, reduced highs
       └── README.md               # Profile documentation

The active config is a symlink::

   /etc/camilladsp/config.yml -> profiles/flat.yml

Profile switching is done by changing the symlink and sending a ``Reload``
command via WebSocket. The API Server manages this:

1. User selects profile in Web UI
2. API Server updates symlink: ``ln -sf profiles/bass_boost.yml config.yml``
3. API Server sends ``Reload`` via WebSocket
4. CamillaDSP validates and applies new config
5. If invalid, CamillaDSP keeps previous config and reports error

.. note::

   Custom user-uploaded profiles are stored on the data partition
   (``/data/camilladsp/profiles/``) and symlinked into the profiles
   directory. This survives OTA updates.

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/camilladsp.service
   [Unit]
   Description=CamillaDSP — Software DSP Engine
   After=hat-init.service sound.target
   Requires=hat-init.service
   Wants=network.target

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/camilladsp /etc/camilladsp/config.yml
   Restart=on-failure
   RestartSec=5

   # WebSocket control socket
   ExecStartPost=/bin/sh -c 'until ss -tln | grep -q ":1234"; do sleep 0.5; done'

   # Resource limits for Pi Zero W
   MemoryMax=128M
   CPUQuota=50%

   [Install]
   WantedBy=multi-user.target

.. note::

   ``CPUQuota=50%`` limits CamillaDSP to half of one CPU core on the Pi
   Zero W. This is important because the Pi Zero W has a single core —
   CamillaDSP must not starve other services. The actual CPU usage for
   our pipeline (volume filters + mixer + basic EQ) should be well under
   50% at 48000 Hz / 1024 chunksize.

   ``MemoryMax=128M`` is a safety limit. CamillaDSP typically uses 20-40MB.

Startup Dependencies
--------------------

.. code-block:: text

   sound.target
       └── hat-init.service (oneshot: set mixer levels)
           └── camilladsp.service (DSP engine, WebSocket on 127.0.0.1:1234)
               └── audio-player.service (requires camilladsp)
               └── tts-engine.service (requires camilladsp)
               └── api-server.service (requires camilladsp for volume control)

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - ALSA loopback (capture)
     - ALSA PCM ``hw:Loopback,0``
     - Read 6 channels of audio from sources
   * - HAT Audio Driver (playback)
     - ALSA PCM ``hw:0,0``
     - Write processed stereo audio to hardware
   * - API Server
     - WebSocket ``127.0.0.1:1234``
     - Volume, faders, mute, config reload, signal levels, state

WebSocket API (selected commands)
---------------------------------

The API Server uses these CamillaDSP WebSocket commands:

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - Command
     - Usage
   * - ``SetVolume(dB)``
     - Set master output volume
   * - ``GetVolume()``
     - Read master volume
   * - ``SetMute(true/false)``
     - Master mute
   * - ``SetFaderVolume(index, dB)``
     - Set per-source volume (0=Main, 1=Aux1/TTS, 2=Aux2/BT)
   * - ``GetFaderVolume(index)``
     - Read per-source volume
   * - ``SetFaderMute(index, true/false)``
     - Per-source mute
   * - ``GetFaders()``
     - Read all fader states at once
   * - ``Reload()``
     - Hot-reload config (profile switch)
   * - ``GetState()``
     - Running state (RUNNING, PAITING, ERROR)
   * - ``GetCaptureSignalPeak()``
     - Per-channel peak levels for UI meters
   * - ``GetProcessingLoad()``
     - Pipeline CPU utilization

Open Questions
--------------

.. todo::

   - **Chunksize**: 1024 is a starting point. Test on real hardware — lower
     values reduce latency but increase CPU. 1024 at 48000 Hz = 21ms buffer,
     which is acceptable for a streaming speaker.

   - **Ducking depth**: -20 dB is the initial default. Should this be
     user-configurable? Suggested: fixed at -20 dB for v1, configurable in
     a future release.

   - **Custom profile validation**: What validation does CamillaDSP perform
     on reload? Do we need additional validation in the API Server before
     triggering a reload? Need to test with malformed YAML.

   - **CPU usage on Pi Zero W**: The ``CPUQuota=50%`` limit needs validation
     on real hardware. If CamillaDSP exceeds this with our pipeline, we may
     need to reduce chunksize or simplify the DSP chain.
