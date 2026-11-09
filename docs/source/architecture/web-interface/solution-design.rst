Solution Design — Web Interface
=================================

.. meta::
   :component: Web Interface
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the Web Interface component.
It covers the Leptos architecture, page structure, state management,
API client, and the build/deployment model.

Design Goals
------------

1. **Single binary** — Frontend compiled to WASM, embedded in the API
   Server binary. No separate web server or build pipeline.
2. **Mobile-first** — Primary use case is controlling the speaker from a
   phone. Touch-friendly UI with large controls.
3. **Real-time** — WebSocket connection for instant state updates.
   No polling.
4. **Offline-aware** — Graceful handling when the API Server is unreachable.

Architecture Overview
---------------------

The Web Interface is a Leptos (Rust → WASM) application that:

1. Is served by the API Server via SSR (server-side rendering)
2. Hydrates on the client for reactive interactivity
3. Connects to the API Server via WebSocket for real-time state
4. Calls REST endpoints for commands

.. plantuml::
   :caption: Web Interface — Architecture
   :name: fig-webui-arch

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Web Interface — Architecture

   package "Browser" as browser {
       package "Leptos WASM\n(client-side)" as wasm {
           component ["Page Router"] as router
           component ["State Store\nreactive signals"] as store
           component ["API Client\nREST + WebSocket"] as client
           component ["Pages"] as pages
       }
   }

   package "API Server\n(server-side)" as server {
       component ["Leptos SSR\nrenderer"] as ssr
       component ["Static Assets\nembedded"] as assets
       component ["REST API\n/api/*"] as rest
       component ["WebSocket\n/ws"] as ws
   }

   browser --> server : HTTPS request
   server --> ssr : render HTML
   ssr --> assets : include WASM bundle
   assets --> browser : WASM + JS
   wasm --> client : hydrate
   client --> rest : REST calls (commands)
   client --> ws : WebSocket (state updates)
   ws --> store : push state
   store --> pages : reactive re-render

   @endl

Page Structure
--------------

.. code-block:: text

   /                       → Player (default landing page)
   /player                 → Player (same as /)
   /config                 → Config section
   /config/radio           → Internet radio stations
   /config/podcasts        → Podcast feeds
   /config/alarms          → Alarm settings
   /config/tokens          → API token management
   /config/bluetooth       → Bluetooth paired devices
   /config/dsp             → DSP profiles
   /config/ota             → OTA update status
   /config/tts             → Text-to-speech: providers, credentials, voices
   /config/system          → System info
   /login                  → Login page (token entry)

Player Page
~~~~~~~~~~~

The primary interface. Always visible or one tap away.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Element
     - Description
   * - Now Playing
     - Current track title, artist, album art (if available)
   * - Source indicator
     - Icon showing active source (Spotify, AirPlay, Radio, etc.)
   * - Playback controls
     - Play/Pause (large center button), Next, Previous
   * - Volume slider
     - Vertical or horizontal slider, 0-100%
   * - Sleep timer
     - Countdown display + set/cancel buttons (15/30/60 min presets)
   * - TTS quick-send
     - Text input + send button for quick TTS messages

Config Pages
~~~~~~~~~~~~

Settings and source management. Each sub-page follows a consistent pattern:
list view → add/edit form → save/cancel.

Text-to-speech (``/config/tts``)
^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^

**Draft (DEC-031).** Where a parent sets up cloud text-to-speech (TTS_REQ_004 to
TTS_REQ_006):

- **Enable cloud TTS**: off by default. Turning it on shows which provider will
  receive announcement text, and needs an explicit confirmation.
- **Providers**: one card each for ElevenLabs and Google Cloud TTS, in priority
  order, which can be changed.
- **Credentials**: a password-type field per provider. After saving, the field is
  cleared and the card shows only "Configured, key ending in 1234". A saved key is
  never sent back to the browser. "Remove" deletes it.
- **Test**: speaks a short phrase through that provider alone and reports success
  or the provider's error, such as a rejected key.
- **Voices**: a voice per language per provider, chosen from the provider's list.

The page refuses to submit credentials unless the page itself was loaded over
HTTPS.

State Management
----------------

Leptos reactive signals (``RwSignal``, ``Memo``) manage client-side state:

.. code-block:: rust

   // Global state store
   struct AppState {
       playback: RwSignal<PlaybackState>,
       volume: RwSignal<VolumeState>,
       bluetooth: RwSignal<BluetoothState>,
       tts: RwSignal<TtsState>,
       ota: RwSignal<OtaState>,
       system: RwSignal<SystemInfo>,
       ws_connected: RwSignal<bool>,
   }

State is updated exclusively via WebSocket events. The REST API is used
only for commands (mutations). This ensures the UI always reflects the
server's authoritative state.

API Client
----------

The API client handles:

1. **REST calls** — ``reqwest`` (WASM-compatible) for commands
2. **WebSocket** — ``web-sys::WebSocket`` for real-time state
3. **Token management** — Store token in ``localStorage``, attach to all
   requests
4. **Reconnection** — Automatic WebSocket reconnect with exponential
   backoff (1s, 2s, 4s, 8s, max 30s)

.. code-block:: rust

   // WebSocket connection with reconnect
   fn connect_ws(token: &str, state: &AppState) {
       let url = format!("wss://{}/ws?token={}", HOST, token);
       let ws = WebSocket::new(&url).unwrap();

       ws.set_onmessage(Some(move |event| {
           let msg: WsMessage = serde_json::from_str(&event.data()).unwrap();
           update_state(&state, msg);
       }));

       ws.set_onclose(Some(move |_| {
           // Reconnect with backoff
           set_timeout(|| connect_ws(token, state), backoff_delay);
       }));
   }

Offline Behaviour
-----------------

When the WebSocket connection is lost:

1. Show a "Connecting…" banner at the top of the screen
2. Disable all controls (buttons greyed out, sliders frozen)
3. Keep the last known state visible (don't clear the UI)
4. Automatically reconnect — when reconnected, state is re-synced from
   the server

When the API Server is completely unreachable (e.g., wrong network):

1. Show "Unable to connect to speaker" with the speaker's expected IP/hostname
2. Provide a "Retry" button
3. If the user has previously connected, show the last known speaker address

Build & Deployment
------------------

The Leptos frontend is built as part of the API Server's build process:

.. code-block:: text

   cargo build --target wasm32-unknown-unknown   (frontend WASM)
   cargo build --target arm-unknown-linux-gnueabihf  (API Server binary)

The WASM bundle is embedded in the API Server binary using
``include_dir!("../frontend/dist")`` or Leptos's built-in asset embedding.

The final deployment artifact is a **single binary** (``api-server``) that:
- Serves the Leptos frontend
- Handles REST API requests
- Manages WebSocket connections
- Connects to all backend services via IPC

No Node.js, no npm, no separate frontend build pipeline. The entire
frontend is Rust → WASM.

.. note::

   The Leptos build requires ``trunk`` (Rust-based WASM build tool) as a
   build dependency. This is a one-time setup in the CI pipeline, not a
   runtime dependency.

Styling
-------

CSS framework decision: **Tailwind CSS** via ``trunk``'s built-in Tailwind
integration.

**Rationale**:
- Utility-first CSS works well with Leptos's component model
- Small bundle size (only used classes are included)
- No JavaScript runtime (pure CSS)
- Widely used, good documentation

The Tailwind config defines a custom color palette matching the Kids Desktop
Speaker brand (to be defined — for now, a default blue/gray palette).

Open Questions
--------------

.. todo::

   - **Album art**: Where does album art come from? Spotify provides it via
     librespot's metadata. AirPlay may provide it. For radio/podcasts, we
     may need to fetch it from the stream metadata or RSS feed. For v1,
     show a placeholder if no art is available.

   - **Responsive breakpoints**: Define specific breakpoints for phone
     (portrait/landscape), tablet, and desktop. Player page should work
     well on a phone held vertically (the primary use case).

   - **Dark mode**: Should the UI support dark mode? Most speaker UIs do.
     Low priority for v1 — light mode only initially.

   - **Internationalization**: The UI is English-only for v1. If we need
     multi-language support later, Leptos has i18n crates available.
     Defer to a future release.
