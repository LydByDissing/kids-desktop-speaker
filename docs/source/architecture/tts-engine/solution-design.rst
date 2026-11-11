Solution Design — TTS Engine
==============================

.. meta::
   :component: TTS Engine
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the TTS Engine component.
It covers the Piper integration, message queue, ALSA output, and IPC
protocol with the API Server.

Design Goals
------------

1. **Fully offline** — No cloud services. All synthesis on-device.
2. **Low latency** — First audio within ~500ms of receiving a text message.
3. **Non-blocking** — TTS playback doesn't block the API Server or other
   services. The message queue handles sequencing.
4. **Multi-voice** — Support multiple Piper voice models, selectable per
   message or as a default.

Architecture Overview
---------------------

The TTS Engine is a standalone Rust binary that:

1. Connects to the API Server via Unix domain socket
2. Maintains a FIFO message queue
3. Synthesizes text to audio using Piper (via FFI or CLI)
4. Writes audio to a dedicated ALSA loopback channel (``loopback_tts``)
5. Notifies the API Server when messages start/finish (for CamillaDSP ducking)

.. plantuml::
   :caption: TTS Engine — Internal Architecture
   :name: fig-tts-internal

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title TTS Engine — Internal Architecture

   package "TTS Engine Process" as tts {
       component ["Message Queue\nFIFO, max 10"] as queue
       component ["Synthesis Worker\n(single thread)"] as worker
       component ["Audio Cache\n/data/tts-cache, 90-day TTL"] as cache
       component ["Synthesis Backend\nespeak-ng (da, en)"] as piper
       component ["ALSA Writer\nloopback_tts"] as alsa
   }

   interface "API Server\nUnix socket" as api
   component "ALSA Loopback\nhw:Loopback,0,1" as loopback

   api --> queue : enqueue(text, voice)
   queue --> worker : dequeue
   worker --> cache : lookup(text, engine, voice)
   cache --> piper : synthesize on a miss
   piper --> cache : WAV
   cache --> worker : WAV
   worker --> alsa : write PCM
   alsa --> loopback : write
   worker --> api : started/finished events

   @enduml

Synthesis Backend
-----------------

**Current: espeak-ng on the device (DEC-030)**, in Danish (``da``, the default)
and English (``en``), chosen per announcement (TTS_REQ_003). It outputs 22.05 kHz
WAV, which the default ALSA output resamples to 48 kHz for the HAT.

**Deferred: cloud providers (DEC-031).** The rest of this section describes the
cloud design, kept for later. It would add ElevenLabs and Google ahead of
espeak-ng without changing the cache.

.. code-block:: rust

   trait Synthesizer: Send + Sync {
       /// Provider, model, voice and audio format: everything that changes the
       /// audio for the same text, so the cache never mixes their output.
       fn id(&self, language: &str) -> String;
       fn synthesize(&self, text: &str, language: &str, output: &Path)
           -> Result<(), TtsError>;
   }

   struct ElevenLabs { api_key: Secret, model: String, voices: HashMap<String, String> }
   struct GoogleTts  { credentials: Secret, voices: HashMap<String, String> }

   /// Providers ranked by quality tier. For each announcement, tries only those
   /// that support its language and are usable, best first; the first success is
   /// cached. espeak-ng is always last.
   struct Chain { providers: Vec<Box<dyn Synthesizer>> }

.. list-table::
   :header-rows: 1
   :widths: 22 39 39

   * -
     - ElevenLabs
     - Google Cloud TTS
   * - Request
     - ``POST https://api.elevenlabs.io/v1/text-to-speech/{voice_id}``, header
       ``xi-api-key``, JSON ``{"text", "model_id"}``
     - ``POST https://texttospeech.googleapis.com/v1/text:synthesize``, JSON
       ``{"input": {"text"}, "voice": {"languageCode", "name"}, "audioConfig"}``
   * - Language
     - Model ``eleven_multilingual_v2`` (the default) detects the language; the
       voice is chosen per language
     - ``voice.languageCode``, with a voice name per language
   * - Audio requested
     - ``output_format=wav_24000`` (44.1 kHz and above needs the Pro tier)
     - ``LINEAR16`` at ``sampleRateHertz: 24000``; returned base64, with a WAV
       header
   * - Limits
     - Not stated in the API reference
     - 5,000 bytes per request; 1,000 requests per minute per project
   * - Credentials
     - API key, header ``xi-api-key``
     - API key restricted to the Text-to-Speech API, header ``x-goog-api-key``;
       service account only if a project rejects keys

Both produce 24 kHz WAV files, which the cache stores as they are and the default
ALSA output resamples to 48 kHz for the HAT.

**Languages.** Danish (``da-DK``) is the default and English (``en-GB``) is
also on; other languages are switched on in the config (TTS_REQ_003). espeak-ng
has voices for both.

**Choosing a provider per language.** Providers are ranked by quality tier, by
default ElevenLabs, then Google, then espeak-ng. For each announcement the chain
skips any provider that:

- doesn't support the announcement's language with the configured model,
- has no credentials or no voice configured for that language, or
- is marked exhausted after a quota error.

The rest are tried best first. Supported languages come from the providers'
APIs when the settings page loads them (ElevenLabs ``GET /v1/models``, each
model's language list; Google ``GET /v1/voices?languageCode=...``), so a model
or account that can't speak a language is skipped for it automatically. At
the time of writing, ElevenLabs' multilingual models (``eleven_multilingual_v2``,
``eleven_flash_v2_5``, ``eleven_v3``) list Danish; ``eleven_flash_v2`` and
``eleven_turbo_v2`` are English only.

**Failures.** Each request has a 10 s timeout. Network errors, timeouts, HTTP 429
and 5xx move on to the next provider. A response saying the quota is used up
(ElevenLabs' quota error, Google's ``RESOURCE_EXHAUSTED``) also marks the provider
as exhausted: it's skipped for the next hour, then tried again, and the Web UI
shows it. There is no character cap; when free quota runs out, speech falls back
to the next provider and finally to espeak-ng, and adding credentials for a paid
account lifts the limit. HTTP 401 or 403 means the credentials are
wrong: it moves on too, and is reported through the API so a parent can fix it.
Text over a provider's limit is refused before sending.

**Credentials** (TTS_REQ_005) live in ``/data/tts/credentials``, mode 0600, set
through the API. They are never logged or returned by the API.

**Privacy** (TTS_REQ_006). Cloud TTS is off until a parent enables it, after
confirming: "Announcements will be sent to [provider] to be turned into speech.
They leave your home network. Messages already spoken are reused without being
sent again."

Audio Cache
-----------

Synthesis takes about 2 seconds on the device, and the same messages repeat, so
results are cached (TTS_REQ_001):

- **Key**: SHA-256 of the synthesizer ID for the language (provider, model,
  voice, audio format), the language and the text, with whitespace collapsed.
  Different providers, voices and languages never share an entry.
- **Storage**: ``/data/tts-cache/<key>.wav`` on the data partition, so the cache
  survives updates. Files are written beside their final name and renamed into
  place, so a crash never leaves a partial entry.
- **Last use**: each hit sets the file's modification time to now.
- **Expiry**: entries unused for 90 days are deleted, at startup and once a day.

Voice Models
~~~~~~~~~~~~

Piper voice models are stored on the data partition:

.. code-block:: text

   /data/piper/
   └── models/
       ├── en_US-amy-low.onnx
       ├── en_US-amy-low.onnx.json
       ├── en_US-lessac-medium.onnx
       └── en_US-lessac-medium.onnx.json

Models survive OTA updates because they're on the data partition. The TTS
Engine loads model paths from a config file (``/etc/kids-desktop-speaker/tts.yml``).

Synthesis Flow
~~~~~~~~~~~~~~

1. Worker thread dequeues a message
2. Calls Piper CLI: ``piper --model <voice>.onnx --output_raw |``
3. Reads raw PCM (16-bit signed, 22050 Hz) from stdout
4. Resamples to 48000 Hz S32_LE (matching the loopback device format)
5. Writes to ``loopback_tts`` via ALSA

.. note::

   Piper outputs at 22050 Hz by default. We resample to 48000 Hz to match
   the loopback device's format. The ``plug`` ALSA plugin handles this
   automatically if we open ``loopback_tts`` (which uses the ``plug`` plugin
   in asound.conf).

Message Queue
-------------

- **FIFO ordering** — Messages are played in the order received
- **Max length: 10** — When full, new messages are rejected with an error
- **Max text length: 500 characters** — Prevents abuse / runaway synthesis
- **Voice selection** — Per-message voice override, or default from config

Queue state is reported to the API Server:
- Queue length
- Currently playing message (if any)
- Estimated remaining time

IPC Protocol — TTS Engine ↔ API Server
----------------------------------------

Communication via Unix domain socket at
``/run/kids-desktop-speaker/tts-engine.sock``.

Length-prefixed JSON (same as Audio Player).

**API Server → TTS Engine (commands)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Payload
   * - ``enqueue``
     - ``{"text": "...", "voice?": "en_US-amy-low", "id": "uuid"}``
   * - ``cancel``
     - ``{"id": "uuid"}`` (cancel specific message)
   * - ``cancel_all``
     - ``{}`` (clear queue)
   * - ``set_default_voice``
     - ``{"voice": "en_US-amy-low"}``
   * - ``get_queue``
     - ``{}`` (returns queue state)

**TTS Engine → API Server (events)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Event
     - Payload
   * - ``message_started``
     - ``{"id": "uuid", "text_preview": "..."}``
   * - ``message_finished``
     - ``{"id": "uuid"}``
   * - ``message_cancelled``
     - ``{"id": "uuid"}``
   * - ``queue_state``
     - ``{"length": u32, "playing?": {...}, "estimated_secs": u32}``
   * - ``error``
     - ``{"code": "...", "message": "..."}``

CamillaDSP Ducking Integration
-------------------------------

The TTS Engine itself does **not** control CamillaDSP ducking. The API Server
orchestrates this:

1. TTS Engine sends ``message_started`` to API Server
2. API Server sends ``SetFaderVolume(0, -20.0)`` to CamillaDSP (duck main)
3. TTS Engine writes audio to loopback_tts
4. TTS Engine sends ``message_finished`` to API Server
5. API Server sends ``SetFaderVolume(0, previous_level)`` to CamillaDSP

This keeps the TTS Engine simple and puts coordination logic in the API
Server where it belongs.

ALSA Output
-----------

The TTS Engine writes to ``loopback_tts`` (``hw:Loopback,0,1`` via
``plug`` plugin). Format: 48000 Hz, S32_LE, stereo. The ``plug`` plugin
handles resampling from Piper's native 22050 Hz.

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/tts-engine.service
   [Unit]
   Description=Kids Desktop Speaker — TTS Engine
   After=camilladsp.service
   Requires=camilladsp.service

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/tts-engine
   Restart=on-failure
   RestartSec=5

   RuntimeDirectory=kids-desktop-speaker
   RuntimeDirectoryMode=0755

   MemoryMax=64M
   CPUQuota=30%

   [Install]
   WantedBy=multi-user.target

.. note::

   ``CPUQuota=30%`` because Piper synthesis is CPU-intensive on the Pi Zero
   W. A single synthesis uses ~20-30% of the CPU for a few seconds. Since
   TTS is intermittent (not continuous), this is acceptable — it won't
   conflict with CamillaDSP's 50% because they're rarely active
   simultaneously. During TTS playback, the main source is ducked anyway.

Startup Dependencies
--------------------

.. code-block:: text

   camilladsp.service
       └── tts-engine.service

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - API Server
     - Unix socket ``/run/kds/tts-engine.sock``
     - Receive TTS messages, send queue events
   * - Piper CLI
     - Subprocess (stdin/stdout)
     - Text-to-audio synthesis
   * - ALSA loopback
     - ALSA PCM ``loopback_tts``
     - Write TTS audio to dedicated channel

Open Questions
--------------

.. todo::

   - **Resampling quality**: The ``plug`` plugin's default resampling may not
     be ideal. Test quality of 22050→48000 resampling. If poor, add a
     dedicated resampling step in the TTS Engine using the ``rubato`` crate.

     **Status (2026-11-11)**: Resolved: the ``speaker`` plug converts 22.05 kHz speech to 48 kHz (DEC-033); accepted by ear on the speaker.

   - **Voice model download**: How do voice models get onto the device
     initially? Options: (a) included in the RAUC bundle, (b) downloaded
     on first boot from GitHub, (c) user uploads via Web UI. For v1,
     include a default English voice in the RAUC bundle.

     **Status (2026-11-11)**: Obsolete: espeak-ng ships its voices in the image (DEC-030).

   - **Piper startup time**: Measure Piper CLI startup time on Pi Zero W.
     If >200ms, consider keeping a warm Piper process or switching to FFI.


     **Status (2026-11-11)**: Obsolete: Piper does not run on ARMv6 (DEC-030).