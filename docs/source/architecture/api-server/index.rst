Api Server
==========

Local-network REST API and WebSocket server. Serves as the central control hub
for the entire software platform. Runs as a single Axum binary that also hosts
the Leptos WASM frontend.

.. toctree::
   :maxdepth: 2

   solution-design

Technology Choice
-----------------

- **Framework**: Axum (Rust)
- **Frontend hosting**: Same binary serves Leptos WASM assets (SSR + hydration)
- **API style**: REST + WebSocket
- **Authentication**: Simple API token (bearer token in ``Authorization`` header)
- **Transport**: HTTPS on local network only

The API Server and the Web Interface share a single Axum binary. The API routes
and the Leptos SSR routes are mounted side-by-side. This simplifies deployment
(one process) and avoids CORS issues since the UI and API share the same origin.

Authentication
--------------

All API endpoints require a valid API token except:

- ``POST /api/auth/login`` — exchange a setup code / device PIN for a token
- ``GET /`` and static assets — the web UI itself

Tokens are simple opaque strings (cryptographically random), stored in a local
file or database on the Pi. Tokens are created and revoked through the Config
section of the Web Interface.

API Surface
-----------

Playback
~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/playback/state``
     - Current source, track, position, playing/paused state
   * - ``POST /api/playback/play``
     - Resume or start playback
   * - ``POST /api/playback/pause``
     - Pause current source
   * - ``POST /api/playback/stop``
     - Stop playback and idle
   * - ``POST /api/playback/next``
     - Skip to next track
   * - ``POST /api/playback/prev``
     - Skip to previous track
   * - ``GET /api/volume``
     - Current volume level
   * - ``POST /api/volume``
     - Set volume level
   * - ``POST /api/sleep-timer``
     - Set or cancel sleep timer

Sources
~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/sources``
     - List available audio sources and their status
   * - ``POST /api/sources/select``
     - Switch to a specific audio source
   * - ``GET /api/radio``
     - List configured internet radio stations
   * - ``POST /api/radio``
     - Add an internet radio station
   * - ``DELETE /api/radio/{id}``
     - Remove an internet radio station
   * - ``GET /api/podcasts``
     - List configured podcast feeds
   * - ``POST /api/podcasts``
     - Add a podcast feed
   * - ``DELETE /api/podcasts/{id}``
     - Remove a podcast feed
   * - ``GET /api/podcasts/{id}/episodes``
     - List episodes for a podcast feed

Config & Automation
~~~~~~~~~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/alarms``
     - List configured alarms
   * - ``POST /api/alarms``
     - Create an alarm
   * - ``PUT /api/alarms/{id}``
     - Update an alarm
   * - ``DELETE /api/alarms/{id}``
     - Delete an alarm
   * - ``POST /api/tts``
     - Submit text message for TTS announcement
   * - ``GET /api/messages``, ``POST /api/messages``
     - List recorded voice messages; upload one (WAV, at most 30 s; plays at once
       unless ``?play=false``) (MSG_REQ_001-003, DEC-037)
   * - ``PATCH /api/messages/{id}``, ``DELETE /api/messages/{id}``
     - Name or rename a message; delete it
   * - ``POST /api/messages/{id}/play``, ``GET /api/messages/{id}/audio``
     - Play a message again; download the recording
   * - ``GET /api/radio/stations``, ``POST /api/radio/stations``,
       ``DELETE /api/radio/stations/{id}``, ``POST /api/radio/play``,
       ``POST /api/radio/stop``
     - Internet radio stations and playback (RAD_REQ_001)
   * - ``POST /api/bluetooth/pair``
     - Open a 3-minute Bluetooth pairing window (BT_REQ_002)
   * - ``GET /api/podcasts``, ``POST /api/podcasts``, ``DELETE /api/podcasts/{id}``
     - Podcast feeds: list, add by RSS URL, remove (POD_REQ_001)
   * - ``GET /api/podcasts/{id}/episodes``, ``POST /api/podcasts/play``,
       ``POST /api/podcasts/stop``
     - The latest 50 episodes with saved positions; play one (continues where it
       stopped); stop and save the position (POD_REQ_002)
   * - ``POST /api/setup/start``, ``POST /api/setup``
     - Ask the speaker to read a setup code aloud; exchange it for an admin token
       (AUTH_REQ_003, no token needed)
   * - ``GET /api/tokens``, ``POST /api/tokens``, ``DELETE /api/tokens/{id}``
     - Manage API tokens (admin token over HTTPS, AUTH_REQ_002)
   * - ``GET /api/tokens``
     - List API tokens (metadata only, no secrets)
   * - ``POST /api/tokens``
     - Create a new API token
   * - ``DELETE /api/tokens/{id}``
     - Revoke an API token

Bluetooth
~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/bluetooth/status``
     - Current Bluetooth adapter status and paired devices
   * - ``POST /api/bluetooth/pair``
     - Enter pairing mode (discoverable)
   * - ``DELETE /api/bluetooth/devices/{addr}``
     - Remove a paired device

CamillaDSP
~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/dsp/status``
     - CamillaDSP running state and current config name
   * - ``GET /api/dsp/config``
     - Current DSP configuration (EQ profile, pipeline)
   * - ``POST /api/dsp/config``
     - Apply a new DSP configuration (change EQ profile, reload pipeline)
   * - ``GET /api/dsp/profiles``
     - List available EQ/DSP profiles

OTA Updates
~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/ota/status``
     - Current software version and update availability
   * - ``POST /api/ota/check``
     - Trigger an update check
   * - ``POST /api/ota/apply``
     - Apply a pending update (triggers reboot)

System Info
~~~~~~~~~~~

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Description
   * - ``GET /api/system/info``
     - CPU temp, memory, uptime, disk usage
   * - ``GET /api/system/services``
     - Status of all platform services

Not Exposed
~~~~~~~~~~~

The following are **not** exposed through the API:

- **Network / Wi-Fi configuration** — managed via OS tools or a separate
  setup mechanism
- **ALSA mixer settings** — hardware mixer is controlled internally by
  CamillaDSP; direct exposure would bypass the DSP pipeline

Auto-Discovery
--------------

The API Server advertises itself on the local network using:

- **mDNS**: ``_kids-speaker._tcp`` service on the API port
- **SSDP**: UPnP device description for basic discoverability

This allows Home Assistant and other controllers to find the speaker
automatically without manual IP configuration.

WebSocket
---------

The API Server pushes real-time state updates to all connected WebSocket
clients on ``ws://{host}/api/ws`` (or ``wss://`` over HTTPS). Messages are
JSON-encoded and cover:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Event
     - Payload
   * - ``playback_state``
     - Source, track, position, playing/paused
   * - ``volume_changed``
     - New volume level
   * - ``source_changed``
     - New active source
   * - ``bluetooth_status``
     - Adapter state, paired device changes
   * - ``dsp_status``
     - CamillaDSP config changes, errors
   * - ``ota_status``
     - Update availability, update progress
   * - ``system_status``
     - Service state changes (crash, restart)
   * - ``tts_announcement``
     - TTS message being spoken

Clients subscribe to all events on connection. No per-topic subscription
filtering in v1.

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol
     - Purpose
   * - Web Interface
     - REST + WebSocket
     - Serves UI assets, handles UI API calls, pushes state updates
   * - Audio Player
     - Internal (Rust)
     - Playback commands, source selection, state queries
   * - CamillaDSP
     - TCP
     - CamillaDSP REST-like API for config, volume, status
   * - TTS Engine
     - Internal (Rust)
     - TTS requests, playback status
   * - OTA Updater
     - Internal (Rust)
     - Trigger update checks, apply updates, query status
   * - Bluetooth Service
     - Internal (Rust)
     - Pairing commands, device status
   * - Home Assistant
     - REST (HTTPS) + mDNS
     - External automation control and auto-discovery

Component pages will be created during the component interview phase.

C4 Components
-------------

The components as implemented (KDS-9ym, 2026-11-11).

.. plantuml::
   :caption: Components — API Server

   @startuml
   !include ../../c4/C4_Component.puml
   LAYOUT_TOP_DOWN()

   Container_Boundary(api, "API Server") {
       Component(http, "Listeners", "axum, axum-server", "HTTPS on 443 with the device certificate (tls.rs); HTTP on 8080 for health checks; each marks its Transport")
       Component(auth, "Authentication", "auth.rs", "Bearer tokens (hashed in /data/api/tokens.json); loopback is the device itself; admin guard")
       Component(setup, "Setup", "setup.rs, routes/setup.rs", "Spoken 6-digit code; reads it until set up; exchanges it for an admin token")
       Component(routes, "Routes", "routes/*", "audio, radio, podcasts, tts, messages, bluetooth, ota, tokens")
       Component(msgs, "Message store", "messages.rs", "WAV recordings in /data/messages; 90-day expiry of unnamed ones")
       Component(ipc, "Service client", "kds-ipc", "One JSON request per connection to /run/kds/*.sock")
   }
   Container_Ext(audio, "Audio Player", "audio.sock")
   Container_Ext(ota, "OTA Updater", "ota.sock")
   Container_Ext(bt, "Bluetooth Service", "bt.sock")
   Rel(http, auth, "every request")
   Rel(auth, routes, "authorised requests")
   Rel(routes, setup, "/api/setup*")
   Rel(routes, msgs, "/api/messages")
   Rel(routes, ipc, "commands")
   Rel(setup, ipc, "announce the code")
   Rel(ipc, audio, "IPC")
   Rel(ipc, ota, "IPC")
   Rel(ipc, bt, "IPC")
   @enduml
