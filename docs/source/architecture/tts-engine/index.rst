Tts Engine
==========

Text-to-speech engine for the Kids Desktop Speaker. Converts text messages
into audio announcements that play over the current music with automatic
ducking. Runs **fully offline** — no cloud services involved.

Technology Choice
-----------------

- **TTS engine**: `Piper`_ (C++ / ONNX Runtime, 10 957 stars, mature)
- **Rust wrapper**: `piper-tts-rs`_ (v0.1.4, Rust FFI bindings to Piper's C API)
- **Process**: Standalone Rust binary
- **Audio output**: Writes to dedicated ALSA loopback channel (see below)

.. _Piper: https://github.com/rhasspy/piper
.. _piper-tts-rs: https://github.com/WrldEngine/piper-tts-rs

Piper is a fast, local neural text-to-speech system designed for embedded
devices. It uses ONNX Runtime for inference and produces natural-sounding
speech with low latency — suitable for the Pi Zero W.

The ``piper-tts-rs`` crate provides Rust FFI bindings, keeping the entire
software platform in Rust where practical. The underlying Piper C++ shared
library is bundled at build time.

.. note::

   ``piper-tts-rs`` (v0.1.4, 202 downloads, 2 stars) is early-stage. If it
   proves insufficient, the TTS Engine can shell out to the ``piper`` CLI
   binary directly as a fallback. This adds a subprocess dependency but
   avoids FFI complexity.

Offline Only
-------------

All TTS processing happens on-device. No text or audio is sent to external
services. This ensures:

- **Privacy**: User messages never leave the local network
- **Reliability**: TTS works without internet connectivity
- **Latency**: No round-trip to a cloud API

Cloud TTS (Google, Azure, etc.) is **not supported** and not planned.

Languages & Voices
-------------------

Piper ships voice models for many languages. The speaker supports **multiple
voices and languages** where Piper provides models.

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Language
     - Notes
   * - English
     - Primary language. Multiple voice models available (male, female, various accents)
   * - Other
     - Depends on available Piper voice models. If multi-language support proves
       too complex for v1, English-only is the fallback.

Voice selection is configured via the API Server (stored in platform config).
The Web Interface can expose a voice picker in the Config section in a future
release. For v1, a single default English voice is used.

Audio Output & CamillaDSP Integration
-----------------------------------------

The TTS Engine writes synthesized audio to a **dedicated ALSA loopback
channel** (ch1). CamillaDSP captures this alongside the main audio (ch0)
and mixes them using its **fader + Volume filter** architecture.

**CamillaDSP faders provide per-source volume control natively:**

- **Main fader** → Volume filter on ch0 (Spotify/AirPlay/Radio/Podcasts)
- **Aux1 fader** → Volume filter on ch1 (TTS Engine)
- **Aux2 fader** → Volume filter on ch2 (Bluetooth A2DP, if enabled)
- **Aux3, Aux4 faders** → reserved for future sources

Each fader is controlled independently via CamillaDSP's websocket API
(``SetFaderVolume``, ``SetFaderMute``, etc.) with smooth ramping
(configurable ``volume_ramp_time``, default 400 ms).

**TTS ducking uses faders directly — no config reload needed:**

1. API Server calls ``SetFaderVolume(Main, -20dB)`` — ramps main audio down
2. TTS Engine writes to ch1 at normal level (Aux1 fader unchanged)
3. API Server calls ``SetFaderVolume(Main, previous_level)`` — ramps back up

This is a **runtime-only operation**. CamillaDSP's pipeline config never
changes. The mixer and Volume filters are always loaded.

Multi-Channel Capture
~~~~~~~~~~~~~~~~~~~~~

CamillaDSP captures from a **single ALSA device** with 6 channels
(3 stereo pairs). All sources write to different channel pairs of
``hw:Loopback,0``:

- Channels 0–1: Main audio (Spotify/AirPlay/Radio/Podcasts)
- Channels 2–3: TTS Engine
- Channels 4–5: Bluetooth A2DP (if enabled)

The ``snd-aloop`` kernel module supports up to 8 subdevices per loopback
device. All subdevices are captured simultaneously when CamillaDSP opens
the device with ``channels: 6``.

.. note::

   All subdevices must use the same sample rate and format. The first
   source to open a subdevice sets the rate/format for the entire loopback
   device. Use ``plughw`` instead of ``hw`` for automatic format conversion
   if sources use different formats.

Audio Output Method
~~~~~~~~~~~~~~~~~~~

The TTS Engine writes audio to its ALSA loopback channel using one of:

- **Direct ALSA PCM output**: TTS Engine opens the loopback PCM device and
  writes audio frames directly. Simple, low overhead.
- **rodio crate**: Rust audio playback library that wraps ALSA. Easier API
  but adds a dependency.

Direct ALSA output is preferred — fewer dependencies, more control over buffer
sizes and timing on the Pi Zero W.

Message Queue
--------------

TTS messages are **queued and played sequentially**. If multiple messages
arrive while an announcement is playing, they wait in a FIFO queue.

.. plantuml::
   :caption: TTS Message Queue Processing
   :name: fig-tts-queue

   @startuml
   title TTS Message Queue Processing

   participant "API Server" as api
   queue "TTS Queue" as queue
   participant "TTS Engine" as tts
   participant "CamillaDSP" as cdsp

   api -> queue : enqueue("Dinner is ready!")
   api -> queue : enqueue("Time for bed!")
   note over queue: [Dinner is ready!, Time for bed!]

   queue -> tts : dequeue("Dinner is ready!")
   api -> cdsp : SetFaderVolume(Main, -20dB)
   cdsp -> cdsp : Ramp main audio down (400ms)
   tts -> tts : synthesize → write to ALSA loopback ch1
   tts -> queue : playback complete
   api -> cdsp : SetFaderVolume(Main, previous_level)
   cdsp -> cdsp : Ramp main audio back up (400ms)

   queue -> tts : dequeue("Time for bed!")
   api -> cdsp : SetFaderVolume(Main, -20dB)
   cdsp -> cdsp : Ramp main audio down (400ms)
   tts -> tts : synthesize → write to ALSA loopback ch1
   tts -> queue : playback complete
   api -> cdsp : SetFaderVolume(Main, previous_level)
   cdsp -> cdsp : Ramp main audio back up (400ms)

   note over queue: empty

   @enduml

Queue behaviour:

- Messages are processed FIFO (first in, first out)
- A maximum queue length prevents unbounded growth (e.g., 10 messages)
- When the queue is full, new messages are rejected with an API error
- The API Server pushes ``tts_announcement`` WebSocket events for each message
  (queued, playing, completed) so the Web UI can show queue status

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
     - Receives TTS text messages, reports queue and playback status
   * - Piper
     - FFI (piper-tts-rs) or CLI subprocess
     - Text-to-audio synthesis
   * - ALSA loopback
     - ALSA PCM
     - Writes TTS audio to dedicated channel for CamillaDSP mixing

Component pages will be created during the component interview phase.
