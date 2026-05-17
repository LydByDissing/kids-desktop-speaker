Api Server
==========

Local-network REST API and WebSocket server. Serves as the central control hub
for the entire software platform. Runs as a single Axum binary that also hosts
the Leptos WASM frontend.

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
