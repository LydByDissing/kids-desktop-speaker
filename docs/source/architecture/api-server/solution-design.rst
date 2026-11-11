Solution Design — API Server
==============================

.. meta::
   :component: API Server
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the API Server component.
It covers the internal architecture, IPC with all backend services,
WebSocket management, authentication, auto-discovery, and the Leptos
frontend integration.

Design Goals
------------

1. **Central control hub** — Single entry point for all API clients.
   Routes commands to the appropriate backend service.
2. **Real-time state** — WebSocket pushes state updates to all connected
   clients with minimal latency.
3. **Single binary** — API Server + Leptos frontend in one Axum process.
   No CORS, no separate frontend server.
4. **Token auth** — Simple bearer token authentication. No OAuth, no
   external identity provider.

Architecture Overview
---------------------

The API Server is a single Axum (Rust) binary that:

1. Serves the Leptos WASM frontend (SSR + hydration)
2. Exposes REST API endpoints
3. Accepts WebSocket connections for real-time state
4. Connects to all backend services via Unix domain sockets
5. Manages CamillaDSP WebSocket connection for volume control
6. Advertises the service via mDNS/SSDP

.. plantuml::
   :caption: API Server — Internal Architecture
   :name: fig-api-internal

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title API Server — Internal Architecture

   package "API Server Process" as api {
       component ["Axum HTTP\nServer"] as http
       component ["Leptos SSR\n+ Hydration"] as leptos
       component ["REST Router\n/api/*"] as rest
       component ["WebSocket\nManager\n/ws"] as ws
       component ["Auth\nMiddleware"] as auth
       component ["Service\nRegistry"] as registry
       component ["State\nAggregator"] as state
   }

   package "IPC Clients" as ipc {
       component ["Audio Player\nClient"] as ap_client
       component ["TTS Engine\nClient"] as tts_client
       component ["OTA Updater\nClient"] as ota_client
       component ["Bluetooth\nClient"] as bt_client
       component ["CamillaDSP\nWebSocket"] as cdsp_client
   }

   interface "Audio Player\n/run/kds/audio-player.sock" as ap_sock
   interface "TTS Engine\n/run/kds/tts-engine.sock" as tts_sock
   interface "OTA Updater\n/run/kds/ota.sock" as ota_sock
   interface "Bluetooth\n/run/kds/bt.sock" as bt_sock
   component "CamillaDSP\n127.0.0.1:1234" as cdsp

   http --> leptos : serve frontend
   http --> rest : route /api/*
   http --> ws : upgrade /ws
   rest --> auth : verify token
   rest --> registry : route to backend
   registry --> ap_client
   registry --> tts_client
   registry --> ota_client
   registry --> bt_client
   registry --> cdsp_client
   ap_client --> ap_sock
   tts_client --> tts_sock
   ota_client --> ota_sock
   bt_client --> bt_sock
   cdsp_client --> cdsp
   state --> ws : push updates

   @enduml

Service Registry
----------------

The Service Registry talks to the backend services over Unix domain sockets,
following the :doc:`IPC protocol <../ipc/index>`: newline-delimited JSON, one
request and one response per connection.

.. code-block:: text

   Service Registry
       ├── Audio Player         → /run/kds/audio.sock
       ├── TTS Engine           → /run/kds/tts.sock (not yet implemented)
       ├── OTA Updater          → /run/kds/ota.sock
       ├── Bluetooth Service    → /run/kds/bt.sock
       └── CamillaDSP           → 127.0.0.1:1234 (WebSocket)

If a backend service is not running, the API Server returns a
``503 Service Unavailable`` for requests routed to that service. Each request
opens a new connection, so a service is used again as soon as it's back. Pushed
events for live updates are not built yet (KDS-izr).

State Aggregator
----------------

The State Aggregator maintains a unified state object that combines state
from all backend services:

.. code-block:: rust

   struct SystemState {
       playback: PlaybackState,      // from Audio Player
       volume: VolumeState,          // from CamillaDSP
       bluetooth: BluetoothState,    // from Bluetooth Service
       tts: TtsState,                // from TTS Engine
       ota: OtaState,                // from OTA Updater
       system: SystemInfo,           // local (CPU, mem, uptime)
   }

When any backend sends an event, the State Aggregator updates the relevant
part of the state and pushes the change to all connected WebSocket clients.

WebSocket Management
--------------------

All WebSocket clients receive the same broadcast stream. No per-topic
filtering in v1.

**Connection flow**:

1. Client connects to ``wss://{host}/ws``
2. Auth middleware validates the token (from query param or cookie)
3. Client receives initial full state snapshot
4. Client receives incremental updates as they happen

**Message format** (JSON):

.. code-block:: json

   {
     "type": "playback_state",
     "data": {
       "source": "spotify",
       "state": "playing",
       "track": {
         "title": "Song Name",
         "artist": "Artist Name",
         "album": "Album Name",
         "duration_ms": 180000,
         "position_ms": 45000
       }
     },
     "timestamp": "2026-05-25T12:00:00Z"
   }

**Event types** (same as documented in the API surface):

- ``playback_state``, ``volume_changed``, ``source_changed`` (from Audio Player)
- ``bluetooth_status`` (from Bluetooth Service)
- ``dsp_status`` (from CamillaDSP)
- ``ota_status`` (from OTA Updater)
- ``tts_announcement`` (from TTS Engine)
- ``system_status`` (local)

Authentication
--------------

Token-based auth with two mechanisms:

**Web UI**: Token stored in ``localStorage`` after login. Sent as
``Authorization: Bearer <token>`` header on REST requests and as
``?token=<token>`` query parameter on WebSocket connections.

**API clients** (Home Assistant, scripts): Token sent as
``Authorization: Bearer <token>`` header.

Token management:

- Tokens are stored in ``/data/auth/tokens.json`` (data partition, survives OTA)
- Each token has: ``id``, ``name``, ``admin``, ``created_at``, ``last_used_at``,
  ``enabled``. Admin tokens are required for credentials and other sensitive
  settings (DEC-032)
- Tokens are created/revoked via the Web UI (Config → API Tokens)
- No token expiry in v1 (manual revocation only)

.. code-block:: json
   :caption: /data/auth/tokens.json

   {
     "tokens": [
       {
         "id": "tok_abc123",
         "name": "Home Assistant",
         "hash": "sha256:...",
         "created_at": "2026-05-25T12:00:00Z",
         "last_used_at": "2026-05-25T13:00:00Z",
         "enabled": true
       }
     ]
   }

.. note::

   Tokens are stored as SHA-256 hashes. The full token is only shown once
   (at creation time) and never stored in plaintext.

Text-to-Speech Configuration
----------------------------

**Deferred with cloud TTS (DEC-031).** Endpoints behind the Web UI's ``/config/tts`` page
(TTS_REQ_005). Credentials are write-only: no endpoint returns a stored key.

.. list-table::
   :header-rows: 1
   :widths: 40 60

   * - Endpoint
     - Purpose
   * - ``GET /api/tts/config``
     - Enabled, provider order, voices per language, and per provider whether a
       key is set plus its last four characters
   * - ``PUT /api/tts/config``
     - Enable or disable cloud TTS, set provider order and voices
   * - ``PUT /api/tts/credentials/{provider}``
     - Store a provider's key. Rejected unless the request arrived over HTTPS
   * - ``DELETE /api/tts/credentials/{provider}``
     - Remove a provider's key
   * - ``POST /api/tts/test/{provider}``
     - Synthesise a short phrase with that provider only, without caching

The API Server passes credentials to the TTS Engine over IPC; the TTS Engine
alone stores them, in ``/data/tts/credentials`` (mode 0600).

Credential endpoints require an admin token (DEC-032). Other tokens can read
``GET /api/tts/config``, which never contains keys.

Auto-Discovery
--------------

The API Server advertises itself via two mechanisms:

**mDNS** (primary):
   Service name: ``_kids-speaker._tcp``
   Properties: ``path=/``, ``version=0.1.0``, ``auth_required=true``

**SSDP** (secondary):
   Device type: ``urn:schemas-upnp-org:device:MediaRenderer:1``
   Location: ``http://{ip}:{port}/description.xml``

Both are implemented using the ``mdns-sd`` and ``ssdp`` Rust crates.
The mDNS advertisement allows Home Assistant to discover the speaker
automatically.

Command Routing
---------------

The API Server routes commands to backends based on the endpoint:

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - Endpoint Prefix
     - Backend
     - Example
   * - ``/api/playback/*``
     - Audio Player
     - ``POST /api/playback/play``
   * - ``/api/volume``
     - CamillaDSP
     - ``POST /api/volume`` → ``SetFaderVolume(0, dB)``
   * - ``/api/sources/*``
     - Audio Player
     - ``POST /api/sources/select``
   * - ``/api/radio/*``, ``/api/podcasts/*``
     - Audio Player (via MPD)
     - ``POST /api/radio``
   * - ``/api/tts``
     - TTS Engine
     - ``POST /api/tts`` → ``enqueue``
   * - ``/api/bluetooth/*``
     - Bluetooth Service
     - ``POST /api/bluetooth/pair``
   * - ``/api/dsp/*``
     - CamillaDSP
     - ``POST /api/dsp/config`` → ``Reload()``
   * - ``/api/ota/*``
     - OTA Updater
     - ``POST /api/ota/apply``
   * - ``/api/alarms/*``
     - Local (API Server)
     - Stored in ``/data/config/alarms.json``
   * - ``/api/tokens/*``
     - Local (API Server)
     - Stored in ``/data/auth/tokens.json``
   * - ``/api/system/*``
     - Local (API Server)
     - Read-only system info

Leptos Frontend Integration
---------------------------

The Leptos frontend is compiled to WASM and served by the same Axum binary:

1. **Server-side rendering (SSR)**: Axum renders the initial HTML on the
   server for fast first paint and SEO (though SEO doesn't matter for a
   local device).
2. **Client-side hydration**: The WASM bundle takes over after initial load,
   enabling reactive UI updates.
3. **Shared types**: Request/response types are shared between server and
   client using Leptos's ``#[server]`` macro for type-safe API calls.

The WASM bundle and static assets are embedded in the binary at compile time
(using ``include_dir!`` or similar), so the deployment is a single binary
with no separate asset directory.

Systemd Integration
-------------------

.. code-block:: ini

   # /etc/systemd/system/api-server.service
   [Unit]
   Description=Kids Desktop Speaker — API Server
   After=camilladsp.service audio-player.service tts-engine.service
        bluetooth-service.service ota-updater.service
   Requires=camilladsp.service

   [Service]
   Type=simple
   ExecStart=/usr/local/bin/api-server
   Restart=on-failure
   RestartSec=5

   RuntimeDirectory=kids-desktop-speaker
   RuntimeDirectoryMode=0755

   MemoryMax=64M
   CPUQuota=15%

   [Install]
   WantedBy=multi-user.target

.. note::

   The API Server depends on CamillaDSP (required) and all other services
   (wanted, not required). If Audio Player or TTS Engine is down, the API
   Server still starts — it returns 503 for those specific endpoints.

Startup Dependencies
--------------------

.. code-block:: text

   camilladsp.service (required)
   audio-player.service (wanted)
   tts-engine.service (wanted)
   bluetooth-service.service (wanted)
   ota-updater.service (wanted)
       └── api-server.service

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 25 25 50

   * - Talks To
     - Protocol / Address
     - Purpose
   * - Web clients
     - HTTPS + WebSocket
     - Serve frontend, REST API, real-time state
   * - Home Assistant
     - HTTPS + mDNS
     - External automation control
   * - Audio Player
     - Unix socket ``/run/kds/audio.sock``
     - Playback commands, state events
   * - TTS Engine
     - Unix socket ``/run/kds/tts.sock``
     - TTS messages, queue events
   * - OTA Updater
     - Unix socket ``/run/kds/ota.sock``
     - Update commands, status events
   * - Bluetooth Service
     - Unix socket ``/run/kds/bt.sock``
     - Pairing, AVRCP events
   * - CamillaDSP
     - WebSocket ``127.0.0.1:1234``
     - Volume, faders, DSP config

Open Questions
--------------

.. todo::

   - **HTTPS certificates**: Resolved by DEC-032, implemented in KDS-cmt
     (WEB_REQ_001): a self-signed certificate made on first boot and kept on
     the data partition. See "HTTPS" below.

   - **Home Assistant protocol**: What specific integration point does Home
     Assistant use? REST API calls? WebSocket? MQTT? This needs to be
     defined before implementation. For now, we expose a standard REST
     API and mDNS — HA can use the ``rest`` integration.

   - **Port configuration**: Resolved: HTTPS on 443 for the Web UI and API,
     plain HTTP on 8080 for health checks and tests (WEB_REQ_001).

HTTPS
-----

Implemented 2026-11-10 (KDS-cmt, WEB_REQ_001, DEC-032).

- On first start the API Server creates a self-signed certificate for
  ``<hostname>.local``, ``<hostname>`` and ``localhost`` (valid 20 years, made
  with ``rcgen``) and keeps it in ``/data/tls`` (key mode 600), so browsers that
  accepted it once keep trusting it across updates. A new hostname gets a new
  certificate.
- HTTPS listens on 443 (``axum-server`` with rustls and ring); plain HTTP stays on
  8080 for the health check, the VM tests and the hardware tests.
- Each listener marks its requests with a ``Transport``. Routes that receive
  credentials or other secrets add the ``require_https`` guard, which answers 403
  over plain HTTP, and also when no marker is present, so a missing layer fails
  safe.
- ``make -C ci hw-https-test`` checks it on a real Pi, connecting by the device's
  ``.local`` name and trusting only the certificate fetched from it.

Authentication
--------------

Implemented 2026-11-11 (KDS-ebt, AUTH_REQ_001-003, DEC-038).

- Every route needs ``Authorization: Bearer <token>``, except ``/health``,
  ``POST /api/setup/start``, ``POST /api/setup`` and requests from the device
  itself (plain HTTP from loopback). Without a valid token: 401.
- Tokens are ``kds_`` plus 64 hex characters (256 random bits), shown once, and
  stored as SHA-256 hashes in ``/data/api/tokens.json`` (mode 600) with a name,
  an admin flag, and when they were created and last used.
- ``GET/POST /api/tokens`` and ``DELETE /api/tokens/{id}`` need an admin token
  over HTTPS (or the device itself). The last admin token can't be revoked.
- Setup: until an admin token exists, the API Server has the Audio Player read
  a 6-digit code aloud in Danish ("Opsætningskode: 4, 7, ... Jeg gentager: ...")
  after start-up, and again 8 s after each reading ends (it waits for the Audio
  Player's ``announcing`` status). ``POST /api/setup {code, name}`` returns an
  admin token. ``POST /api/setup/start`` asks for a new code (once a minute at
  most), also to recover a lost admin token. A code allows 5 attempts and
  expires after 10 minutes unless it is still being read.
- ``make -C ci hw-install`` sets up a development Pi over loopback so it doesn't
  read codes; ``make -C ci hw-setup-test`` checks the readings on a real Pi.
