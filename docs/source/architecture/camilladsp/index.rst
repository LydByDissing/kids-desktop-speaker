Camilladsp
==========

.. note::

   Superseded in part by DEC-033 (2026-11-10): announcements pause the source
   instead of being mixed over it, so the pipeline is one stereo stream. All
   sources and the announcer write to the ``speaker`` device (a plug in front
   of a dmix on the HAT). The 6-channel design below is kept for reference
   until CamillaDSP is added.

Software DSP engine for the Kids Desktop Speaker. Handles EQ, crossover, bass
enhancement, and **master volume control**. All audio passes through CamillaDSP
before reaching the hardware. Runs as a **systemd service**.


.. toctree::
   :maxdepth: 2

   solution-design

Technology Choice
-----------------

- **Engine**: `CamillaDSP`_ (Rust, open source)
- **Deployment**: systemd service — starts on boot, managed by the OS
- **Configuration**: YAML config files (predefined profiles + user-uploaded)
- **Control API**: CamillaDSP TCP socket (internal only, not exposed to LAN)

.. _CamillaDSP: https://github.com/HEnquist/camilladsp

CamillaDSP is an existing, mature Rust project. We integrate it rather than
build our own DSP engine. It provides:

- Flexible audio pipeline (filter chains, mixers, conditionals)
- **5 independent volume faders** (Main + Aux1–Aux4) for per-source volume
- **Mixer with per-input gain** — route and combine multiple sources
- Real-time volume and fader control via websocket API
- Config hot-reload without restart
- ALSA, PipeWire, Bluetooth (BlueZ), and file I/O support
- Low-latency processing suitable for the Pi Zero W
- SIMD acceleration (NEON on ARM, AVX on x86)
- **Bluetooth capture support** (v2.0+) — can capture directly from BlueZ

Volume Control — Faders
------------------------

CamillaDSP provides **5 independent volume faders** that can be controlled
in real-time via the websocket API:

.. list-table::
   :header-rows: 1
   :widths: 15 30 55

   * - Fader
     - Assigned To
     - Purpose
   * - Main
     - Primary audio source
     - Master volume for Spotify/AirPlay/Radio/Podcasts
   * - Aux1
     - TTS Engine
     - TTS announcement volume (independent of main source)
   * - Aux2
     - Bluetooth A2DP (if enabled)
     - Bluetooth audio input volume
   * - Aux3
     - (reserved)
     - Future use (e.g., line-in, alarm tones)
   * - Aux4
     - (reserved)
     - Future use

Each fader supports:

- ``SetFaderVolume`` / ``GetFaderVolume`` — set/read in dB (-150 to +50)
- ``SetFaderMute`` / ``GetFaderMute`` / ``ToggleFaderMute`` — per-fader mute
- ``AdjustFaderVolume`` — relative change with optional min/max limits
- ``GetFaders`` — read all fader states in one call

All volume changes include a **smooth ramp** (configurable ``volume_ramp_time``,
default 400 ms) — no audible clicks or pops.

**TTS ducking** uses faders directly:

1. API Server lowers the Main fader (ducking the primary source)
2. TTS plays at Aux1 normal level
3. API Server restores the Main fader when TTS completes

No mixer reconfiguration needed. No config reload. Purely runtime.

.. important::

   The CamillaDSP websocket is **internal only**. It binds to
   ``127.0.0.1`` (localhost) and is **never exposed** on the local network.
   The API Server is the only consumer.

DSP Profiles
-------------

Predefined Profiles
~~~~~~~~~~~~~~~~~~~

The software platform ships with predefined DSP profiles as YAML config files:

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Profile
     - Description
   * - Flat
     - No EQ, flat response — for reference listening
   * - Bass Boost
     - Enhanced low-frequency response for small speaker compensation
   * - Night Mode
     - Reduced dynamic range, softer highs, lower max volume
   * - Custom (user-uploaded)
     - User-provided YAML config file

Custom Profiles
~~~~~~~~~~~~~~~

Users can upload a custom CamillaDSP YAML config via the Web Interface (Config
section). The uploaded file is validated for basic structure and then stored
alongside the predefined profiles. Selecting a custom profile triggers a
CamillaDSP config reload.

Profile switching flow:

1. User selects a profile in the Web UI (or via API)
2. API Server sends reload command to CamillaDSP via TCP socket
3. CamillaDSP validates the new config and applies it
4. If the config is invalid, CamillaDSP keeps the previous config and reports
   an error
5. API Server receives success/failure and pushes status via WebSocket

Audio Pipeline
--------------

CamillaDSP sits in the middle of the audio pipeline:

.. plantuml::
   :caption: CamillaDSP — Pipeline Position
   :name: fig-cdsp-pipeline

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title CamillaDSP — Pipeline Position

   package "ALSA Loopback" as loopback {
       component ["Loopback ch0\n"Main audio capture"] as ch0
       component ["Loopback ch1\n"TTS audio capture"] as ch1
   }

   package "CamillaDSP" as cdsp {
       component ["Mixer\n"Per-input volume"] as mixer
       component ["DSP / EQ\n"Filters, crossover"] as dsp
       component ["Master Volume\n"Final output level"] as vol
   }

   component ["ALSA PCM hw:0,0\n"HAT Audio Driver"] as hw

   ch0 --> mixer : main audio
   ch1 --> mixer : TTS audio
   mixer --> dsp : mixed signal
   dsp --> vol : processed signal
   vol --> hw : final output

   @enduml

Source Routing at ALSA Level
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

CamillaDSP can **reload its config** without restarting, but rewriting the
config on every source change is a bad approach (fragile, slow, risk of
errors). Instead, **source routing is handled at the ALSA level**:

- All audio sources write to the **same ALSA loopback device**
- The Audio Player ensures **only one source is active at a time** (pauses/
  stops the previous source before starting the new one)
- CamillaDSP always reads from the same ALSA loopback capture device — no
  config changes needed for source switches

This keeps CamillaDSP's config stable and moves source management to where
it belongs: the Audio Player controlling ALSA loopback access.

Per-Source Volume — Faders + Volume Filters
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

CamillaDSP's faders combined with Volume filters enable **per-source
independent volume control** without any config reload:

.. plantuml::
   :caption: Per-Source Volume Architecture
   :name: fig-cdsp-per-source

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Per-Source Volume — Faders + Volume Filters

   package "ALSA Loopback" as loopback {
       component ["ch0: Main audio\n(Spotify/AirPlay/Radio)"] as ch0
       component ["ch1: TTS Engine"] as ch1
       component ["ch2: Bluetooth A2DP\n(if enabled)"] as ch2
   }

   package "CamillaDSP Pipeline" as cdsp {
       component ["Volume(Main)\nMaster level"] as vol_main
       component ["Volume(Aux1)\nTTS level"] as vol_tts
       component ["Volume(Aux2)\nBT level"] as vol_bt
       component ["Mixer\ncombine ch0+ch1+ch2"] as mixer
       component ["DSP / EQ"] as dsp
   }

   component ["HAT Audio Driver\nhw:0,0"] as hat

   ch0 --> vol_main
   ch1 --> vol_tts
   ch2 --> vol_bt

   vol_main --> mixer
   vol_tts --> mixer
   vol_bt --> mixer

   mixer --> dsp
   dsp --> hat

   @enduml

Each source passes through a Volume filter assigned to a dedicated fader.
The mixer combines all sources. The API Server controls each fader independently
via the websocket API.

**Source switching** (e.g., from Spotify to AirPlay) does NOT require
CamillaDSP changes. The Audio Player simply:

1. Stops the old source writing to ch0
2. Starts the new source writing to ch0
3. CamillaDSP keeps reading from ch0 — no config change needed

**TTS ducking** lowers the Main fader while TTS plays on Aux1.

**Bluetooth A2DP** (if enabled) gets its own channel and Aux2 fader.

This architecture scales: adding a new source means adding another loopback
channel and optionally another Volume filter + Aux fader.

TTS Ducking — Fader Timeline
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. plantuml::
   :caption: TTS Ducking — Fader Timeline
   :name: fig-cdsp-tts-ducking

   @startuml
   title TTS Ducking — Fader Timeline

   participant "API Server" as api
   participant "CamillaDSP\n(Main fader)" as main
   participant "CamillaDSP\n(Aux1 fader)" as aux1
   participant "TTS Engine\n(loopback ch1)" as tts

   note over api: TTS message received
   api -> main : SetFaderVolume(0, -20dB)
   main -> main : Ramp main source down (400ms)
   api -> tts : Start TTS playback
   tts -> tts : Write TTS audio to loopback ch1
   note over aux1 : TTS at normal level\n(Aux1 fader unchanged)
   tts -> tts : TTS playback complete
   api -> tts : Stop TTS
   api -> main : SetFaderVolume(0, previous_level)
   main -> main : Ramp main source back up (400ms)
   note over api : Announcement complete
   @enduml

Source Priority
~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Priority
     - Source
   * - 1 (highest)
     - TTS announcement (ducks Main fader, plays on Aux1)
   * - 2
     - AirPlay (Audio Player stops previous source on ch0, starts AirPlay on ch0)
   * - 3
     - Spotify Connect (same — switch on ch0)
   * - 4 (lowest)
     - Internet radio / Podcasts (same — switch on ch0)

Multi-Channel Capture
~~~~~~~~~~~~~~~~~~~~~

CamillaDSP captures from a **single ALSA device**. With ``snd-aloop``, we
create a single loopback device with multiple channels (e.g., 6 channels =
3 stereo pairs), where each pair is a separate source:

.. code-block:: yaml

   devices:
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

Each source writes to a different pair of channels:

- Channels 0–1: Main audio (Spotify/AirPlay/Radio/Podcasts)
- Channels 2–3: TTS Engine
- Channels 4–5: Bluetooth A2DP (if enabled)

The CamillaDSP mixer routes each channel pair through its own Volume
filter + fader, then mixes down to stereo output.

.. note::

   ``snd-aloop`` creates a single PCM device with configurable subdevices.
   Each subdevice is an independent stereo pair. All subdevices are captured
   simultaneously when CamillaDSP opens ``hw:Loopback,0`` with
   ``channels: 6``. This is standard ALSA behavior — no special plugins
   needed.

   The only requirement is that all subdevices use the same sample rate
   and format. The first source to open a subdevice sets the rate/format
   for the entire loopback device. Sources must be started in the correct
   order (or use ``plughw`` for automatic format conversion).

.. todo::

   **Test on real hardware**: Verify that 3+ sources can write to different
   subdevices of ``hw:Loopback,0`` simultaneously, and that CamillaDSP
   captures all 6 channels without xruns on Pi Zero W.

CamillaDSP WebSocket API
-------------------------

Reference: `CamillaDSP WebSocket API`_

.. _CamillaDSP WebSocket API: https://github.com/HEnquist/camilladsp/blob/master/websocket.md

The websocket (not raw TCP — it's a JSON-based websocket protocol on port
1234 by default) provides these operations used by the API Server:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Purpose
   * - ``GetVolume`` / ``SetVolume``
     - Read/set master volume in dB
   * - ``AdjustVolume``
     - Relative volume change with optional limits
   * - ``GetMute`` / ``SetMute`` / ``ToggleMute``
     - Master mute control
   * - ``GetFaderVolume`` / ``SetFaderVolume``
     - Per-fader volume (0=Main, 1–4=Aux1–4)
   * - ``AdjustFaderVolume``
     - Per-fader relative change with optional limits
   * - ``GetFaderMute`` / ``SetFaderMute``
     - Per-fader mute
   * - ``GetFaders``
     - Read all 5 fader states in one call
   * - ``GetState``
     - Running state (RUNNING, WAITING, ERROR, etc.)
   * - ``GetCaptureRate``
     - Measured capture sample rate
   * - ``GetSignalRange``
     - Signal level range in last chunk
   * - ``GetCaptureSignalPeak`` / ``Rms``
     - Per-channel peak/RMS for UI meters
   * - ``GetProcessingLoad``
     - Pipeline CPU utilization
   * - ``Reload``
     - Reload config from file (switch DSP profile)
   * - ``GetConfigFilePath``
     - Current config file path
   * - ``PatchConfig``
     - Apply partial config update at runtime

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol
     - Purpose
   * - API Server
     - WebSocket (127.0.0.1:1234)
     - Volume, faders, mute, config reload, signal levels, state queries
   * - ALSA loopback
     - ALSA capture
     - Read audio from loopback (multi-channel)
   * - HAT Audio Driver
     - ALSA playback (hw:0,0)
     - Write processed audio to hardware

Component pages will be created during the component interview phase.
