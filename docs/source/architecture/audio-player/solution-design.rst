Solution Design — Audio Player
================================

.. meta::
   :component: Audio Player
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the Audio Player component.
It covers the audio source management, source switching, ALSA loopback
integration, Spotify Connect via librespot, and the IPC protocol with the
API Server.

Design Goals
------------

1. **Source management** — One active source at a time on the main loopback
   channel. Clean switching with no gaps or glitches.
2. **Spotify Connect** — Integrated via librespot as a library, not a
   separate process.
3. **Extensible** — AirPlay and internet radio/podcast backends can be added
   without changing the core architecture.
4. **CamillaDSP coordination** — All volume changes and fades go through
   CamillaDSP's WebSocket API. The Audio Player never touches ALSA mixer
   controls directly.

Architecture Overview
---------------------

The Audio Player is a standalone Rust binary that:

1. Connects to the API Server via Unix domain socket
2. Manages a set of audio source backends (Spotify, AirPlay, Radio, Podcasts)
3. Writes audio to the ALSA loopback device (``loopback_main``)
4. Controls CamillaDSP faders via WebSocket for volume/fade
5. Implements the sleep timer

.. plantuml::
   :caption: Audio Player — Internal Architecture
   :name: fig-audio-player-internal

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Audio Player — Internal Architecture

   package "Audio Player Process" as ap {
       component ["Source Manager"] as sm
       component ["Spotify Backend\nlibrespot"] as spotify
       component ["AirPlay Backend\nTBD"] as airplay
       component ["Radio/Podcast Backend\nTBD"] as radio
       component ["ALSA Writer\loopback_main"] as alsa
       component ["Sleep Timer"] as timer
       component ["CamillaDSP Client\nWebSocket"] as cdsp
   }

   interface "API Server\nUnix socket" as api
   component "CamillaDSP\n127.0.0.1:1234" as cdsp_svc
   component "ALSA Loopback\nhw:Loopback,0,0" as loopback

   api --> sm : source commands
   sm --> spotify : start/stop
   sm --> airplay : start/stop
   sm --> radio : start/stop
   spotify --> alsa : decoded PCM
   airplay --> alsa : decoded PCM
   radio --> alsa : decoded PCM
   alsa --> loopback : write
   sm --> cdsp : fader commands
   cdsp --> cdsp_svc : WebSocket
   timer --> sm : pause on expiry

   @enduml

Source Manager
--------------

The Source Manager is the core orchestrator. It enforces these rules:

- **Only one source** is active on the main loopback channel at a time
- **Source switching**: stop current source → start new source (no overlap)
- **Priority**: TTS (handled by CamillaDSP, not the Audio Player) > AirPlay >
  Spotify > Radio/Podcasts
- **State tracking**: current source, playback state (playing/paused/stopped),
  track metadata, position

Source Manager State Machine
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. plantuml::
   :caption: Source Manager — State Machine
   :name: fig-source-sm

   @startuml
   title Source Manager — State Machine

   [*] --> Idle

   Idle --> Playing : start_source()
   Playing --> Paused : pause()
   Paused --> Playing : resume()
   Playing --> Idle : stop()
   Paused --> Idle : stop()

   Playing --> Playing : switch_source()
   note right of Playing : Stop current, start new\n(CamillaDSP fade handled\nby API Server)

   @enduml

Source Backends
---------------

Each backend implements a common Rust trait:

.. code-block:: rust

   // Common trait for all audio source backends
   trait AudioSource: Send {
       /// Human-readable name ("Spotify", "AirPlay", "Radio")
       fn name(&self) -> &str;

       /// Start playback. Audio frames are written to the provided ALSA PCM.
       fn play(&mut self, pcm: &AlsaPcm) -> Result<(), SourceError>;

       /// Pause (keep connection alive, stop writing audio)
       fn pause(&mut self) -> Result<(), SourceError>;

       /// Stop and clean up
       fn stop(&mut self) -> Result<(), SourceError>;

       /// Current track metadata
       fn metadata(&self) -> SourceMetadata;

       /// Is audio currently being produced?
       fn is_active(&self) -> bool;
   }

Spotify Backend (librespot)
~~~~~~~~~~~~~~~~~~~~~~~~~~~

librespot is integrated as a **Rust library** (not a subprocess). The Spotify
backend:

1. Connects to Spotify's servers using the librespot protocol
2. Receives audio frames (Ogg Vorbis, up to 320 kbps)
3. Decodes to PCM
4. Writes to the ALSA loopback device via the ``plug`` plugin

.. code-block:: text
   :caption: librespot integration

   librespot (Rust library)
       └── SpotifyConnect session
           └── Audio decoder (Ogg Vorbis → PCM)
               └── ALSA writer (loopback_main)

librespot handles discovery/advertising via mDNS — the speaker appears as a
Spotify Connect device on the local network automatically.

.. note::

   librespot requires a Spotify Premium account for authentication. The
   credentials are stored on the data partition
   (``/data/spotify/credentials.json``) and survive OTA updates.

AirPlay Backend — Decision
~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_004
   :status: resolved
   :date: 2026-05-25
   :tags: audio, airplay

**Decision**: Use **shairport-sync** (C) as a subprocess for v1.

**Rationale**:

- Both Rust AirPlay libraries (rairplay, shairplay) are early-stage with
  minimal adoption. rairplay has 22 downloads, shairplay has 0.
- shairport-sync is the de facto standard: 8 650 stars, actively maintained,
  battle-tested on Raspberry Pi.
- It outputs to ALSA directly — we configure it to write to ``loopback_main``,
  same as any other source.
- The Audio Player starts/stops shairport-sync as a child process, same as
  it would manage any external backend.

**Integration**:

.. code-block:: text

   Audio Player (source manager)
       └── shairport-sync (child process, supervised: AUD_REQ_002)
           └── ALSA "speaker" (plug, 48 kHz) → IQaudIO Pi-Codec Zero

shairport-sync reads ``/etc/shairport-sync.conf`` (``ci/device/shairport-sync.conf``):
it advertises the hostname, uses ``basic`` interpolation (soxr is too heavy for
the Pi Zero W) and writes to the ``speaker`` device from ``/etc/asound.conf``,
the same path the announcements use. The package's own
``shairport-sync.service`` is disabled; the Audio Player starts the process,
reaps it if it exits, and restarts it with backoff (1 s doubling to 60 s).

Measured on the Pi Zero W (2026-11-09, PipeWire RAOP sender on Linux):
16–18 % CPU and 16 MB RSS while playing; about 0.2 s of audio queued in the
HAT's buffer. The roughly 2 s from pause to silence is the sender's latency
(PipeWire's ``raop.latency.ms`` defaults to 1500 ms), not the device.

.. note::

   shairport-sync advertises itself via mDNS/Bonjour. iOS devices discover
   it automatically. No additional discovery configuration needed.

Internet Radio and Podcasts — the Stream Player
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Radio and podcasts play in the Audio Player itself (DEC-048, which replaced
MPD, DEC_005). ``audio-player/src/stream/`` holds:

- ``http.rs``: the stream as a seekable source. Live streams ask for ICY
  metadata; a file with a length is seekable, and a seek asks the server for
  the rest from that byte (``Range``), or reads ahead for a short skip. A
  stalled connection fails after 15 s.
- ``icy.rs``: strips the metadata blocks and keeps the latest ``StreamTitle``
  (UTF-8 or Latin-1).
- ``mod.rs``: the ``Player`` (``play_url``, ``seek``, ``set_paused``,
  ``stop``, ``status``, the same operations the code used on MPD). Each
  playback runs on its own thread: symphonia probes and decodes MP3, AAC
  (ADTS and ``.m4a``), Ogg Vorbis, FLAC and WAV; ``play_url`` returns once the
  stream is recognised, or with the reason it can't play. A live stream that
  drops reconnects up to 3 times (after 1, 2 and 4 s); a file ends at its end.
- ``sink.rs``: ``aplay -D speaker`` reading raw 16-bit samples, the shared
  output that converts any rate for the HAT (DEC-033), as announcements use.
  aplay blocks while its buffer is full, which paces decoding. Pausing closes
  it; resuming opens it again.

**Measured on the Pi Zero W (2026-11-12, DR P1, 128 kbit/s MP3)**: 9.3 % CPU for
decoding plus 4.3 % for aplay, 5 MB of memory; MPD used 12.3 % and 39 MB, and
needed 144 packages (~140 MB) for plugins the speaker never used.

**Stations** live in ``/data/radio/stations.json`` (DR P1–P3 until changed).
Announcements pause the player and resume it afterwards.

**Podcasts (KDS-9jt.4, POD_REQ_001/002)**: feeds live in
``/data/podcasts/feeds.json`` and listening positions in
``/data/podcasts/positions.json``. The Audio Player fetches a feed with
``reqwest`` and parses it with ``roxmltree``, keeping the latest 50 episodes so
a reply fits the 64 KiB IPC limit. An episode's enclosure plays like a station,
then seeks to the saved position; while it plays, the position is saved every
15 s and when it stops. Podcasts count as the radio source for one-at-a-time
(DEC-039).

**Tests**: ``tests/stream_test.rs`` plays every format, a live stream with
titles, pause, seek, stop, failures and reconnects against a local HTTP server
(``tests/common``), with an output that keeps time faster than real time.

ALSA Output
-----------

All backends write to the same ALSA device: ``loopback_main`` (which maps
to ``hw:Loopback,0,0`` via the ``plug`` plugin). The Audio Player opens
this device once and passes it to whichever backend is active.

Standard format: **48000 Hz, S32_LE, stereo**. The ``plug`` plugin handles
automatic format conversion if a backend produces a different format.

CamillaDSP Coordination
-----------------------

The Audio Player does **not** control volume directly. All volume operations
go through CamillaDSP:

- **Source switch fade**: API Server handles this (fades out Main fader,
  switches source, fades in). The Audio Player just starts/stops backends.
- **Volume change**: API Server sends ``SetFaderVolume(0, dB)`` to CamillaDSP.
- **Mute**: API Server sends ``SetFaderMute(0, true/false)`` to CamillaDSP.

The Audio Player connects to CamillaDSP's WebSocket (``127.0.0.1:1234``)
for fader control and state queries.

Sleep Timer
-----------

Implemented in the Audio Player:

1. API Server sends ``SetSleepTimer(duration_secs)`` via IPC
2. Audio Player starts a countdown
3. On expiry: Audio Player pauses the current source
4. API Server is notified via IPC → WebSocket push to Web UI
5. Timer can be cancelled or extended at any time

IPC Protocol — Audio Player ↔ API Server
------------------------------------------

Communication via Unix domain socket at
``/run/kids-desktop-speaker/audio-player.sock``.

**Protocol**: Length-prefixed JSON messages (4-byte big-endian length +
UTF-8 JSON payload).

**API Server → Audio Player (commands)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Command
     - Payload
   * - ``start_source``
     - ``{"source": "spotify|airplay|radio|podcast", "url?": "..."}``
   * - ``stop``
     - ``{}``
   * - ``pause``
     - ``{}``
   * - ``resume``
     - ``{}``
   * - ``set_volume``
     - ``{"level": 0.0-1.0}`` (forwarded to CamillaDSP)
   * - ``set_sleep_timer``
     - ``{"duration_secs": u32}``
   * - ``cancel_sleep_timer``
     - ``{}``

**Audio Player → API Server (events)**:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Event
     - Payload
   * - ``source_started``
     - ``{"source": "spotify", "metadata": {...}}``
   * - ``source_stopped``
     - ``{"source": "spotify"}``
   * - ``playback_state``
     - ``{"state": "playing|paused|stopped", "position_ms": u64}``
   * - ``track_changed``
     - ``{"title": "...", "artist": "...", "album": "...", "duration_ms": u64}``
   * - ``sleep_timer_expired``
     - ``{}``
   * - ``error``
     - ``{"code": "...", "message": "..."}``

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/audio-player.service
   [Unit]
   Description=Kids Desktop Speaker — Audio Player
   After=camilladsp.service
   Requires=camilladsp.service

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/audio-player
   Restart=on-failure
   RestartSec=5

   # Runtime directory for Unix socket
   RuntimeDirectory=kids-desktop-speaker
   RuntimeDirectoryMode=0755

   # Resource limits
   MemoryMax=128M
   CPUQuota=40%

   [Install]
   WantedBy=multi-user.target

.. note::

   ``CPUQuota=40%`` because librespot decoding + ALSA writing is the
   heaviest workload in the system. Combined with CamillaDSP's 50%, this
   leaves 10% headroom for the kernel and other services on the single-core
   Pi Zero W. Needs validation on real hardware.

Startup Dependencies
--------------------

.. code-block:: text

   camilladsp.service
       └── audio-player.service
           └── (spotify backend ready — mDNS advertising starts)
           └── (shairport-sync child process — mDNS advertising starts)

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - API Server
     - Unix socket ``/run/kds/audio-player.sock``
     - Receive commands, send state events
   * - CamillaDSP
     - WebSocket ``127.0.0.1:1234``
     - Fader volume, mute, fade commands
   * - ALSA loopback
     - ALSA PCM ``loopback_main``
     - Write decoded audio to loopback
   * - Spotify
     - Spotify Connect protocol
     - Receive audio stream
   * - shairport-sync
     - Child process (stdout/stderr)
     - AirPlay audio (started/stopped by Audio Player)
   * - Stations and podcast hosts
     - HTTP(S), ICY metadata, ``Range``
     - Internet radio and podcast streams (stream player, DEC-048)

Open Questions
--------------

.. todo::

   - **librespot version**: Confirm librespot v0.8.0 supports the Pi Zero W
     (ARMv6). May need to test compilation with ``arm-unknown-linux-gnueabihf``
     target.

     **Status (2026-11-11)**: Resolved: librespot 0.8.0 is built for ARMv6 in CI and plays on the Pi (DEC-034).

   - **MPD as systemd dependency**: Should MPD be a separate systemd service
     that audio-player.service depends on, or should the Audio Player start
     MPD itself? Recommendation: separate service for clean lifecycle mgmt.

     **Status (2026-11-12)**: Superseded: MPD is gone; the Audio Player plays radio and podcasts itself (DEC-048).

   - **CPU budget**: 40% for Audio Player + 50% for CamillaDSP = 90% of a
     single core. Tight. Need real hardware validation. May need to reduce
     CamillaDSP's CPUQuota or simplify the DSP pipeline.


     **Status (2026-11-11)**: Resolved by measurement: one source at 8-18 % plus CamillaDSP at 8 % (Hardware Measurements, DEC-036); no CPUQuota needed.