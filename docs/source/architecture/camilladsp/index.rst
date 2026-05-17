Camilladsp
==========

Software DSP engine for the Kids Desktop Speaker. Handles EQ, crossover, bass
enhancement, and **master volume control**. All audio passes through CamillaDSP
before reaching the hardware. Runs as a **systemd service**.

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
- Real-time volume control via TCP socket
- Config hot-reload without restart
- ALSA, PipeWire, and file I/O support
- Low-latency processing suitable for the Pi Zero W

Master Volume
-------------

CamillaDSP owns the **master volume** for the entire speaker. All volume
changes — from the Web Interface, Bluetooth AVRCP, API, or TTS ducking — go
through CamillaDSP's volume control via its internal TCP socket API.

The API Server is the sole internal client of the CamillaDSP TCP socket. No
other container talks to CamillaDSP directly. The API Server translates
external requests (REST, WebSocket, BT) into CamillaDSP volume commands.

**Volume range**: 0–100 % (mapped to CamillaDSP's internal dB range).

**Volume operations via TCP socket**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Operation
     - Description
   * - Get volume
     - Read current master volume level
   * - Set volume
     - Set master volume (with fade ramp)
   * - Mute / unmute
     - Toggle mute state
   * - Get signal levels
     - Read signal levels for UI level meters (input, output)

.. important::

   The CamillaDSP TCP socket is **internal only**. It binds to
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

TTS Ducking Pipeline
~~~~~~~~~~~~~~~~~~~~~

TTS announcements need to play over the current music source. The approach:

1. **Two ALSA loopback channels**: one for the main audio source, one for TTS
2. **CamillaDSP pipeline** mixes both inputs with independent volume control
3. When a TTS message arrives:
   - API Server ducks the main source volume via CamillaDSP
   - TTS audio plays on the TTS loopback channel at normal volume
   - After TTS completes, API Server restores the main source volume
4. No CamillaDSP config reload is needed — the mixer is always in the pipeline

.. plantuml::
   :caption: TTS Ducking — Volume Timeline
   :name: fig-cdsp-tts-ducking

   @startuml
   title TTS Ducking — Volume Timeline

   participant "API Server" as api
   participant "CamillaDSP\n(mixer ch0 vol)" as cdsp
   participant "TTS Engine\n(loopback ch1)" as tts

   note over api: TTS message received
   api -> cdsp: Set ch0 volume to duck level (fade ramp)
   cdsp -> cdsp: Fade main audio down
   api -> tts: Start TTS playback
   tts -> tts: Write TTS audio to loopback ch1
   note over cdsp: Main audio ducked\nTTS playing on ch1 at normal vol
   tts -> tts: TTS playback complete
   api -> tts: Stop TTS
   api -> cdsp: Set ch0 volume to restore level (fade ramp)
   cdsp -> cdsp: Fade main audio back up
   note over api: Announcement complete
   @enduml

TTS Priority & Source Switching (updated from Audio Player spec):

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Priority
     - Source
   * - 1 (highest)
     - TTS announcement (ducks main source via CamillaDSP volume, plays on ch1)
   * - 2
     - AirPlay (Audio Player stops previous source on ch0, starts AirPlay on ch0)
   * - 3
     - Spotify Connect (same — switch on ch0)
   * - 4 (lowest)
     - Internet radio / Podcasts (same — switch on ch0)

CamillaDSP TCP Socket API
--------------------------

Reference: `CamillaDSP WebSocket/REST API`_

The TCP socket provides these operations used by the API Server:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Purpose
   * - ``/setvolume``
     - Set master volume (with ramp for smooth fade)
   * - ``/getvolume``
     - Read current volume
   * - ``/setmute``
     - Mute/unmute output
   * - ``/getmute``
     - Read mute state
   * - ``/configreload``
     - Reload config from file (switch DSP profile)
   * - ``/getconfigname``
     - Get name of currently loaded config
   * - ``/getsiglev``
     - Signal levels for UI meters
   * - ``/getstate``
     - Running state (RUNNING, WAITING, ERROR, etc.)

.. _CamillaDSP WebSocket/REST API: https://github.com/HEnquist/camilladsp/blob/master/doc/restapi.md

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol
     - Purpose
   * - API Server
     - TCP socket (127.0.0.1)
     - Volume, mute, config reload, signal levels, state queries
   * - ALSA loopback
     - ALSA capture
     - Read audio from loopback (main ch0 + TTS ch1)
   * - HAT Audio Driver
     - ALSA playback (hw:0,0)
     - Write processed audio to hardware

Component pages will be created during the component interview phase.
