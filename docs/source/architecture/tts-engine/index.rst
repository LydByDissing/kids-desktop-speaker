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

Audio Output & ALSA Channel Architecture
-----------------------------------------

**Open architectural question**: How many ALSA loopback channels does the
platform need, and how does CamillaDSP mix them?

Current design has two channels:

- **ch0**: Main audio (Spotify, AirPlay, radio, podcasts)
- **ch1**: TTS announcements

However, this raises a broader question:

.. todo::

   **Investigation needed — ALSA channel architecture**:

   Should **each audio source get its own ALSA loopback channel**, with
   CamillaDSP mixing them? For example:

   - ch0: Spotify Connect (librespot)
   - ch1: AirPlay receiver
   - ch2: MPD (radio / podcasts)
   - ch3: TTS Engine
   - ch4: (future: Bluetooth A2DP, line-in, etc.)

   **Benefits of per-source channels**:

   - CamillaDSP mixer can control per-source volume independently
   - No need for the Audio Player to stop/pause sources on switch — just
     mute the old channel and unmute the new one in CamillaDSP
   - TTS ducking becomes natural: duck all channels except TTS
   - Source switching via CamillaDSP mixer API rather than ALSA-level stop/start
   - Cleaner separation; easier to add sources later

   **Concerns**:

   - How many loopback channels can CamillaDSP's mixer handle?
   - Does the CamillaDSP API support per-channel volume control at runtime?
   - Do we need to extend CamillaDSP's mixer capabilities?
   - ALSA loopback device has a limited number of channels (typically 8 substreams
     per device; may need multiple loopback devices)
   - Pi Zero W CPU impact of a more complex CamillaDSP pipeline

   **Action items**:

   - Investigate CamillaDSP mixer capabilities and API for per-input volume
   - Investigate ALSA loopback channel limits on Pi Zero W
   - Determine if CamillaDSP needs patching/extending for our use case
   - Prototype a multi-channel CamillaDSP config

   If per-source channels work, this simplifies the Audio Player significantly
   and makes source switching, ducking, and future expansion much cleaner.

Regardless of the final channel count, the TTS Engine writes its audio to a
dedicated ALSA loopback channel. CamillaDSP mixes it with the main audio and
applies ducking via per-input volume control. No CamillaDSP config reload is
needed for TTS — the mixer is always in the pipeline.

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
   api -> cdsp : duck ch0 (main audio)
   tts -> tts : synthesize → write to ALSA loopback ch3
   tts -> queue : playback complete
   api -> cdsp : restore ch0 volume

   queue -> tts : dequeue("Time for bed!")
   api -> cdsp : duck ch0
   tts -> tts : synthesize → write to ALSA loopback ch3
   tts -> queue : playback complete
   api -> cdsp : restore ch0 volume

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
