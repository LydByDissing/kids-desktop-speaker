Solution Design — API Server
==============================

.. meta::
   :component: API Server
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the API Server component.
It covers the internal architecture, IPC with all backend services,
live events, authentication, auto-discovery, and serving the Web UI.

Design Goals
------------

1. **Central control hub** — Single entry point for all API clients.
   Routes commands to the appropriate backend service.
2. **Real-time state** — live events push state changes to every subscribed
   client within 2 s (UI_REQ_004, DEC-046).
3. **Single binary** — the API Server also serves the Web UI's static files
   (DEC-045). No CORS, no separate frontend server.
4. **Token auth** — Simple bearer token authentication. No OAuth, no
   external identity provider.

Architecture Overview
---------------------

The API Server is a single Axum (Rust) binary that:

1. Serves the Web UI's static files (Preact, built in CI; DEC-045)
2. Exposes REST API endpoints
3. Streams live events at ``/api/events`` (Server-Sent Events, DEC-046)
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
       component ["Static Files\n/usr/share/kds/web"] as web
       component ["REST Router\n/api/*"] as rest
       component ["Event Hub\n/api/events (SSE)"] as hub
       component ["Auth\nMiddleware"] as auth
       component ["Service\nRegistry"] as registry
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

   http --> web : everything else
   http --> rest : route /api/*
   rest --> hub : nudge after a change
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
   hub --> registry : poll while subscribed

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
opens a new connection, so a service is used again as soon as it's back.

Event Hub (live events)
-----------------------

Live updates follow DEC-046. ``GET /api/events`` answers with a
``text/event-stream`` that stays open. It is a normal API route: the auth
middleware checks the ``Authorization: Bearer`` header, and any token may
subscribe.

.. code-block:: text

   event: volume
   data: {"volume":60,"limit":75,"db":-24.0}

   event: source
   data: {"active":"radio","sources":{"radio":{"state":"playing",...}}}

Each event is named after its topic, and its data is the JSON its endpoint
returns:

.. list-table::
   :header-rows: 1
   :widths: 15 35 50

   * - Topic
     - Same as
     - Changes when
   * - ``source``
     - ``GET /api/audio/source``
     - something starts or stops (radio, AirPlay, Spotify, Bluetooth, podcast)
   * - ``volume``
     - ``GET /api/audio/volume``
     - the volume or the limit changes (API, remote, sleep timer fade, alarm)
   * - ``timer``
     - ``GET /api/timer``
     - the sleep timer starts, is cancelled or ends
   * - ``bluetooth``
     - ``GET /api/bluetooth/status``
     - a device connects or disconnects, pairing opens or closes
   * - ``ota``
     - ``GET /api/ota/status``
     - an update downloads, installs, waits for a reboot (``reboot_at``)

**How it works**: the hub holds a broadcast channel and the last value of each
topic. While at least one client subscribes it fetches every topic every 2 s
through the same handlers as the REST routes, compares each with the last value
(ignoring ``remaining_s`` counters, which the UI counts down itself) and sends
those that changed. A new subscriber first gets the current value of every
topic. A middleware nudges the hub after every successful non-GET ``/api``
request, so a change made through the API shows at once. Nobody subscribing
costs nothing. A client that falls behind skips to the latest values. A
keep-alive comment every 15 s keeps proxies from closing the stream.

Authentication
--------------

Token-based auth with two mechanisms:

**Web UI**: Token stored in ``localStorage`` after login. Sent as
``Authorization: Bearer <token>`` header on REST requests and on the live
events stream; a token never goes in a URL.

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

Web UI Files
------------

The Web UI (DEC-045) is built in CI into static files, installed in the image
at ``/usr/share/kds/web`` and served by ``web.rs`` for every path the API
doesn't route, without a token, with a Content-Security-Policy that allows only
the speaker's own scripts. Paths the app handles itself get ``index.html``;
unknown ``/api`` paths get a JSON ``404``.

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
     - HTTPS (REST, Server-Sent Events)
     - Serve the Web UI, REST API, live events
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

     **Status (2026-11-11)**: Planned in wave 6 (KDS-sgt.6): mDNS discovery and the REST API with a token (DEC-038).

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
