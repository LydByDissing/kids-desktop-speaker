Architecture
============

This section describes the architecture of the **software platform** for the
Kids Desktop Speaker using the C4 model. It covers the software running on the
Raspberry Pi Zero W — not the physical speaker enclosure, electronics, or
mechanical design. Start here for the big picture, then drill into each container.

.. note::
   This documentation describes the **software platform** only. The full
   speaker product also includes the physical hardware (enclosure, speakers,
   IQaudIO Pi-Codec Zero HAT, Raspberry Pi Zero W), which is documented
   separately.

System Context
--------------

.. plantuml::
   :caption: System Context — Kids Desktop Speaker Software Platform
   :name: fig-system-context

   @startuml
   !include ../c4/C4_Context.puml

   title System Context — Kids Desktop Speaker Software Platform

   Person(child, "Child", "Plays audio from their smart devices (iPad, iPhone, etc.)")
   Person(parent, "Parent", "Controls playback when child has no device; sends TTS messages")

   System_Boundary(speaker, "Kids Desktop Speaker\nSoftware Platform") {
       System(core, "Kids Desktop Speaker\nSoftware Platform", "Software running on Raspberry Pi Zero W — audio streaming, DSP, control, and HAT driver")
   }

   System_Ext(spotify, "Spotify", "Music streaming service")
   System_Ext(airplay, "AirPlay", "Apple audio streaming protocol")
   System_Ext(ha, "Home Assistant", "Smart home automation platform")
   System_Ext(hat, "IQaudIO Pi-Codec Zero", "Hardware audio HAT (DAC, ADC, speaker amp)")

   Rel(child, core, "Controls playback, selects source", "Spotify Connect / AirPlay / HTTPS (web UI)")
   Rel(parent, core, "Controls playback, sends TTS messages", "Spotify Connect / AirPlay / HTTPS (web UI) / TBD (TTS)")
   Rel(core, spotify, "Streams audio", "Spotify Connect")
   Rel(core, airplay, "Receives audio", "AirPlay (single connection, AirPlay 1)")
   Rel(core, ha, "Control functions, auto-discovered", "TBD (API, auth, local network, auto-discovery)")
   Rel(core, hat, "Drives audio output", "ALSA (snd-soc-iqaudio-codec)")

   SHOW_LEGEND()
   @enduml

**Users**

- **Child**: Plays audio from their smart devices. Uses Spotify Connect via the
  Spotify app, AirPlay from any iOS app, or the device's web UI for podcasts
  and internet radio.
- **Parent**: Controls playback when the child does not have their own device.
  Also sends custom text messages to the speaker for TTS announcements
  (interface TBD).

**Audio Sources**

- **Spotify**: Streams audio via Spotify Connect. No Spotify Web API needed —
  the speaker acts as a Spotify Connect receiver.
- **AirPlay**: Receives audio from any iOS app. Single connection, AirPlay 1
  only (AirPlay 2 has no open source implementation).

**Infrastructure**

- **Home Assistant**: Smart home integration. The speaker exposes a local-network
  API (protocol TBD, authenticated, auto-discovered) that Home Assistant can
  use to control playback, volume, and TTS.

**Hardware**

- **IQaudIO Pi-Codec Zero**: The hardware HAT that provides DAC, ADC, speaker
  amplifier, and headphone output. The software platform drives this hardware
  through the standard ALSA kernel driver (``snd-soc-iqaudio-codec``).

System Requirements
~~~~~~~~~~~~~~~~~~~

The following system-level requirements are defined in :doc:`/requirements/requirements`.
Specifications below show how the architecture fulfills them.

.. spec:: Auto Source Select — Architecture
   :id: SPEC_ARCH_001
   :status: open
   :fulfills: SYS_REQ_001
   :tags: architecture, system

   The Audio Player container manages source selection. When a new source requests
   playback, the Audio Player stops the current source and activates the new one.
   CamillaDSP handles the audio fade transition.

.. spec:: Fade Transitions — Architecture
   :id: SPEC_ARCH_002
   :status: open
   :fulfills: SYS_REQ_002, SYS_REQ_003
   :tags: architecture, system

   The Audio Player coordinates with CamillaDSP to fade out the current source
   (mute) before switching and fade in the new source from mute to the previous
   volume level.

.. spec:: Local-Network API — Architecture
   :id: SPEC_ARCH_003
   :status: open
   :fulfills: SYS_REQ_004, SYS_REQ_005
   :tags: architecture, system

   The API Server container exposes a local-network REST API with authentication.
   All control operations (playback, volume, TTS, config) are accessible through
   this API.

.. spec:: Auto-Discovery — Architecture
   :id: SPEC_ARCH_004
   :status: open
   :fulfills: SYS_REQ_006
   :tags: architecture, system

   The system advertises itself on the local network via mDNS and/or SSDP, allowing
   Home Assistant and other controllers to discover it automatically.

Specifications fulfilling system requirements:

.. needlist::
   :types: spec
   :tags: system
   :status: open

Containers
----------

.. plantuml::
   :caption: Container — Kids Desktop Speaker Software Platform
   :name: fig-container

   @startuml
   !include ../c4/C4_Container.puml

   title Container — Kids Desktop Speaker Software Platform

   Person(child, "Child", "")
   Person(parent, "Parent", "")

   System_Ext(spotify, "Spotify", "")
   System_Ext(airplay, "AirPlay", "")
   System_Ext(ha, "Home Assistant", "")
   System_Ext(hat, "IQaudIO Pi-Codec Zero", "Hardware audio HAT")

   System_Boundary(speaker, "Kids Desktop Speaker Software Platform") {

       Container(webui, "Web Interface", "Leptos WASM + Axum", "Player (playback, volume, sleep timer) and Config (radio, podcasts, alarms, API tokens)")
       Container(api, "API Server", "Axum (Rust)", "Local-network REST + WebSocket API with token auth; also hosts Leptos frontend")
       Container(player, "Audio Player", "Rust", "Supervises shairport-sync, librespot, bluealsa-aplay; radio and podcasts in its own stream player (DEC-048); one source at a time; announcements and messages; device volume via CamillaDSP")
       Container(bt, "Bluetooth Service", "Rust + zbus + BlueZ", "Pairing agent and window; AVRCP pause/play of phones; remote buttons from input events; A2DP via BlueALSA")
       Container(dsp, "CamillaDSP", "Rust (external, built in CI) + systemd", "One stereo stream from the loopback to the HAT; device volume; limiter at -1 dBFS; EQ to come")
       Container(tts, "TTS Engine", "Rust library + espeak-ng", "Danish and English speech with a 90-day disk cache; used by the Audio Player's announcer")
       Container(wifisetup, "Wi-Fi Setup", "Rust + hostapd + dnsmasq", "Setup access point and captive portal when Wi-Fi is missing or failing (DEC-044)")
       Container(ota, "OTA Updater", "Rust + RAUC", "Atomic system updates via RAUC A/B partitions; GitHub Releases; auto-rollback")
       Container(hatdrv, "HAT Audio Driver", "ALSA / Linux kernel", "Kernel driver for IQaudIO Pi-Codec Zero (DA7213 codec)")

   }

   Rel(child, webui, "Controls playback", "HTTPS / WebSocket")
   Rel(child, player, "Streams via Spotify / AirPlay", "Spotify Connect / AirPlay")
   Rel(parent, webui, "Controls playback, sends TTS", "HTTPS / WebSocket")

   Rel(webui, api, "Playback commands, state queries", "REST / WebSocket")
   Rel(api, player, "Play, pause, skip, volume, source", "Internal API")
   Rel(player, dsp, "Device volume", "CamillaDSP websocket")
   Rel(player, tts, "Synthesise announcements", "Rust library")
   Rel(parent, wifisetup, "Enters the home Wi-Fi", "Captive portal on the setup AP")
   Rel(api, ota, "Trigger update check", "Internal API")
   Rel(api, bt, "Bluetooth status", "Internal API")
   Rel(api, ha, "Control & status", "HTTPS REST with a token; found over mDNS (DEC-043)")

   Rel(player, dsp, "Audio pipeline", "ALSA loopback")
   Rel(player, spotify, "Spotify Connect receiver", "Spotify Connect protocol")
   Rel(player, airplay, "AirPlay receiver", "AirPlay 1 protocol")
   Rel(bt, player, "Remote buttons, phone streaming state", "IPC")

   Rel(dsp, hatdrv, "Processed audio output", "ALSA PCM (hw:0,0)")
   Rel(hatdrv, hat, "Analogue audio output", "I²S / I²C")

   SHOW_LEGEND()
   @enduml

The software platform consists of eight containers running on the Raspberry Pi
Zero W:

.. list-table::
   :header-rows: 1
   :widths: 20 15 65

   * - Container
     - Technology
     - Responsibility
   * - Web Interface
     - Leptos WASM + Axum
     - Player section (playback, track skip, volume, sleep timer) and Config
       section (radio/podcast sources, alarms, API tokens). Also Wi-Fi setup,
       TTS, OTA, and system info.
   * - API Server
     - Axum (Rust)
     - Local-network REST + WebSocket API with token auth. Central control hub
       that routes commands to all containers. Also hosts the Leptos frontend.
       Exposes playback, sources, config, Bluetooth pairing, CamillaDSP config,
       TTS, OTA, and system info endpoints. Handles mDNS/SSDP auto-discovery.
   * - Audio Player
     - Rust + librespot
     - Core audio playback engine. Manages audio sources (Spotify Connect via
       librespot, AirPlay, podcasts, internet radio), handles source switching
       with CamillaDSP-managed fades, TTS priority ducking, and sleep timer.
       Separate process from API Server. AirPlay and radio/podcast backends TBD.
   * - Bluetooth Service
     - Rust + bluer + BlueZ
     - AVRCP remote control from paired BT keypad/knob (play/pause, volume,
       track skip). Optional A2DP audio input under evaluation (CPU/complexity
       trade-off on Pi Zero W). Recommendation: AVRCP-only for v1.
   * - CamillaDSP
     - Rust (external) + systemd
     - Software DSP engine (master volume, EQ, bass enhancement). Runs as
       systemd service. Predefined DSP profiles + user-uploaded configs.
       Two-channel ALSA loopback mixer for TTS ducking. TCP socket (localhost
       only) controlled by API Server.
   * - TTS Engine
     - Piper + piper-tts-rs
     - Offline text-to-speech using Piper (neural TTS, fast on Pi). Writes to
       a dedicated ALSA loopback channel. FIFO message queue, multi-language
       voice support (English primary). Fully offline — no cloud services.
   * - OTA Updater
     - Rust + RAUC
     - Atomic full-system updates using RAUC A/B partitions. Downloads RAUC
       bundles from GitHub Releases, applies via ``rauc install``, auto-rollback
       on boot failure. Scheduled reboots in user-configured low-activity
       windows. User data persisted on separate data partition.
   * - HAT Audio Driver
     - ALSA / Linux kernel / systemd
     - Kernel driver for the IQaudIO Pi-Codec Zero HAT (Dialog DA7213
       codec). Configured via Raspberry Pi OS device tree overlay and systemd.
       Provides ALSA PCM device, mixer controls, and loopback channels.

.. toctree::
   :maxdepth: 2
   :caption: Containers

   web-interface/index
   api-server/index
   audio-player/index
   bluetooth-service/index
   camilladsp/index
   tts-engine/index
   ota-updater/index
   wifi-setup/index
   hat-audio-driver/index

.. toctree::
   :maxdepth: 2
   :caption: Cross-Cutting

   data-model/index
   ipc/index
