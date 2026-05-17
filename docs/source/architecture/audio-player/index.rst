Audio Player
============

Core audio playback engine. Manages audio sources, handles source switching
with fade transitions, and coordinates the audio pipeline from source to
hardware output. Runs as a **separate Rust process** from the API Server.

Technology Choice
-----------------

- **Language**: Rust
- **Process**: Standalone binary, communicates with API Server via internal API
- **Spotify Connect**: `librespot`_ (Rust, v0.8.0, 6 787 stars, mature and widely used)
- **AirPlay**: Under investigation (see below)
- **Internet radio / Podcasts**: Under investigation (see below)

.. _librespot: https://github.com/librespot-org/librespot

AirPlay Receiver — Investigation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

Two Rust AirPlay receiver libraries exist on crates.io:

.. list-table::
   :header-rows: 1
   :widths: 20 15 15 50

   * - Crate
     - Version
     - Stars
     - Notes
   * - `shairplay`_
     - 0.2.0
     - 0
     - Pure Rust, claims AP1+AP2 audio & video. Very new (April 2026),
       449 downloads. Repo appears to be a work-in-progress fork.
   * - `rairplay`_
     - 1.0.0
     - 38
     - Rust AirPlay 2 receiver. Actively maintained (commits as recent as
       May 2026). 22 downloads.

.. _shairplay: https://github.com/metaneutrons/shairplay-rust
.. _rairplay: https://github.com/r4v3n6101/rairplay

**Maturity assessment**: Both Rust AirPlay libraries are early-stage. For
comparison, `shairport-sync`_ (C) is the de facto standard with 8 650 stars
and 23 open issues, actively maintained.

.. _shairport-sync: https://github.com/mikebrady/shairport-sync

.. todo::

   **Decision needed**: Choose AirPlay strategy:

   - **Option A**: Use ``rairplay`` (Rust, AP2). Risk: early-stage, may lack
     features or have bugs. Benefit: all-Rust stack, AP2 support.
   - **Option B**: Use ``shairport-sync`` (C). Risk: C dependency, AP1 only.
     Benefit: battle-tested, reliable.
   - **Option C**: Start with shairport-sync, migrate to a Rust library when
     one matures.

Internet Radio & Podcasts — Investigation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

How other platforms handle internet radio and podcast playback:

**Volumio** (Node.js + MPD)
   Uses `MPD`_ (Music Player Daemon) as its core playback engine. MPD natively
   supports HTTP stream playback (internet radio) via playlist entries pointing
   to stream URLs. Podcasts are added as RSS feed playlists, with individual
   episodes playable as tracks. Volumio's Node.js UI layer talks to MPD over
   its socket protocol.

**moOde audio** (PHP + MPD)
   Same MPD-based architecture. Radio stations are stored as MPD playlists
   with stream URLs. Podcasts handled similarly.

**RuneAudio** (PHP + MPD)
   MPD-based again. The pattern is consistent: MPD handles all stream decoding
   and playback.

.. _MPD: https://www.musicpd.org/

**Rust ecosystem options**:

.. list-table::
   :header-rows: 1
   :widths: 25 75

   * - Approach
     - Trade-offs
   * - Use MPD (C daemon)
     - Battle-tested, handles all stream formats, playlist management built in.
       But adds a C dependency and a separate daemon. Communicate via MPD
       protocol (``mpd_client`` Rust crate exists, v1.4.1, 63k downloads).
   * - Build custom (Rust)
     - Use ``rodio`` + ``stream-download`` crates for HTTP stream decoding.
       Full Rust control but must handle format support (codec, sample rate),
       buffering, and playlist management ourselves. More work, more risk on
       the Pi Zero W's limited CPU.
   * - Use GStreamer (Rust bindings)
     - ``gstreamer`` Rust crate wraps the GStreamer C library. Handles all
       formats, proven pipeline model. Heavy dependency, complex build for
       cross-compilation on Pi Zero W.

.. todo::

   **Decision needed**: Choose internet radio / podcast strategy:

   - **Option A**: Use MPD as a backend daemon. Communicate from the Audio
     Player via the ``mpd_client`` Rust crate. Simple, reliable, well-proven
     by Volumio/moOde.
   - **Option B**: Build custom HTTP stream playback in Rust using ``rodio``
     and ``stream-download``. Full Rust stack but more engineering effort.
   - **Option C**: Use GStreamer Rust bindings. Powerful but heavy.

Audio Pipeline
---------------

All audio flows through a dual-channel ALSA loopback pipeline:

.. plantuml::
   :caption: Audio Pipeline — Source to Output
   :name: fig-audio-pipeline

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Audio Pipeline — Source to Output

   package "Audio Sources" as sources {
       component [librespot\n"Spotify Connect"] as spotify
       component [AirPlay receiver\n"AirPlay 1"] as airplay
       component [MPD / stream\n"Radio, Podcasts"] as mpd
       component [TTS Engine\n"Announcements"] as tts
   }

   package "ALSA Loopback\nhw:Loopback,0 (6ch)" as loopback {
       component ["ch0-1: Main audio\nSpotify/AirPlay/Radio"] as ch0
       component ["ch2-3: TTS Engine"] as ch1
       component ["ch4-5: Bluetooth A2DP"] as ch2
   }

   component [CamillaDSP\n"Volume filters, Mixer, DSP, EQ"] as dsp
   component ["HAT Audio Driver\n"hw:0,0"] as hat

   spotify --> ch0 : ALSA PCM
   airplay --> ch0 : ALSA PCM
   mpd --> ch0 : ALSA PCM
   tts --> ch1 : ALSA PCM

   ch0 --> dsp : 6ch loopback capture
   ch1 --> dsp
   ch2 --> dsp

   dsp --> hat : ALSA PCM

   @enduml

All audio sources write to **different channel pairs** of a single
``hw:Loopback,0`` device with 6 channels (3 stereo pairs):

- **ch0–1**: Main audio (Spotify/AirPlay/Radio/Podcasts) — only one source
  active at a time, managed by the Audio Player
- **ch2–3**: TTS Engine — independent, always available
- **ch4–5**: Bluetooth A2DP (if enabled) — independent, always available

CamillaDSP captures all 6 channels simultaneously. Each channel pair passes
through its own Volume filter + fader, then the mixer combines them. TTS
ducking is done by lowering the Main fader — no config reload, no source
stopping. See :doc:`/architecture/camilladsp/index` for the full pipeline.

Source Switching & Priority
-----------------------------

Source activation follows these rules:

1. **New source starts**: Any currently playing source is paused/stopped/
   ignored. The new source takes over.

2. **TTS announcement (highest priority)**: When a TTS message arrives:
   - Current source volume is **ducked** (lowered) via CamillaDSP
   - TTS audio plays at normal volume
   - After TTS completes, the previous source volume is **restored** to its
   original level
   - The previous source is **not stopped** — it continues playing at reduced
   volume during the announcement

3. **Fade transitions**: All volume changes (source switch, TTS duck/restore)
   are handled by **CamillaDSP** via its TCP API. CamillaDSP applies smooth
   fade curves — the Audio Player does not implement fading itself.

   Source routing is handled at the ALSA level: only one source writes to
   the main loopback channel at a time. CamillaDSP's config stays stable
   across source switches. See :doc:`/architecture/camilladsp/index` for the
   full pipeline design.

Source Priority Order
~~~~~~~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Priority
     - Source
   * - 1 (highest)
     - TTS announcement (ducks others)
   * - 2
     - AirPlay (interrupts lower-priority sources)
   * - 3
     - Spotify Connect (interrupts lower-priority sources)
   * - 4 (lowest)
     - Internet radio / Podcasts / Local files

When two same-priority sources conflict (unlikely), the most recently
activated one wins.

Sleep Timer
-----------

The Audio Player implements the sleep timer feature from the Web Interface
Player section:

- User sets a countdown duration (e.g., 15 min, 30 min, 60 min)
- Audio Player starts a timer; when it expires, playback is paused
- The timer can be cancelled or extended at any time
- Timer state is reported via the API Server's WebSocket for UI display

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
     - Receives playback commands, reports state changes
   * - CamillaDSP
     - WebSocket (CamillaDSP API)
     - Fader volume, mute, fade transitions via SetFaderVolume/SetFaderMute
   * - ALSA loopback
     - ALSA PCM
     - Sources write audio here; CamillaDSP reads from it
   * - librespot
     - Internal (Rust)
     - Spotify Connect receiver (runs within Audio Player process)
   * - AirPlay receiver
     - Internal (Rust or C child process)
     - AirPlay audio receiver (TBD: rairplay or shairport-sync)
   * - MPD / stream player
     - Internal or TCP
     - Internet radio and podcast playback (TBD: MPD or custom)

Component pages will be created during the component interview phase.
