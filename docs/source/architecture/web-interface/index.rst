Web Interface
==============

Browser-based control panel for the Kids Desktop Speaker software platform.
Built as a Rust WebAssembly application using Leptos, served by the Axum-based
API Server. Provides two main sections: **Player** and **Config**.


.. toctree::
   :maxdepth: 2

   solution-design

Technology Choice
-----------------

- **Frontend framework**: Leptos (Rust → WebAssembly)
- **Backend/serving**: Axum (shared with API Server)
- **Rendering**: Server-side rendering (SSR) with client-side hydration
- **Styling**: TBD (CSS framework decision pending)

Leptos was chosen to keep the entire software platform in Rust where possible,
benefiting from type safety, small WASM bundles, and no JS/Node.js toolchain
on the build host.

Structure
---------

The web interface is organised into two main sections:

Player
~~~~~~

The primary interface for day-to-day use. Always visible or one tap away.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Feature
     - Description
   * - Playback control
     - Play, pause, stop for the current audio source
   * - Next / previous track
     - Skip forward or backward in the current queue (Spotify, podcasts)
   * - Volume
     - Master volume slider with optional volume-limit indicator (parental control)
   * - Sleep timer
     - Set a countdown after which playback stops and the speaker goes idle

Config
~~~~~~

Settings and source management. Accessed via a separate view or tab.

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Feature
     - Description
   * - Internet radio
     - Add, edit, and remove internet radio stream URLs
   * - Podcasts
     - Add, edit, and remove podcast RSS feed URLs
   * - Alarm settings
     - Set alarm time, source, and volume for scheduled wake-up playback
   * - API tokens
     - Create and revoke API tokens for automation (Home Assistant, scripts, etc.)

Additional Pages
~~~~~~~~~~~~~~~~

Beyond Player and Config, the web interface also provides:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Feature
     - Description
   * - Wi-Fi setup
     - Initial network configuration when no network is configured
   * - TTS messages
     - Compose and send text messages to be read aloud by the TTS engine
   * - OTA updates
     - View update status and trigger software/firmware updates
   * - System info
     - Diagnostics: CPU temp, memory, uptime, service status, ALSA mixer state

Authentication
--------------

The web interface does **not** implement its own authentication. All auth is
handled by the API Server. The UI sends credentials (or a stored token) with
each request; the API Server enforces access control. The UI simply presents
a login form and stores the returned token.

Offline Behaviour
-----------------

When the API Server is unreachable the UI shows a "Connecting…" overlay with
an automatic retry. Cached server-side rendered content remains visible but
controls are disabled. The WebSocket reconnects automatically when the server
comes back.

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - Talks To
     - Protocol
     - Purpose
   * - API Server
     - REST (HTTPS)
     - Playback commands, config changes, source browsing, TTS, OTA
   * - API Server
     - WebSocket
     - Real-time state updates (now playing, volume, queue, service status)

User Journeys & Wireframes
---------------------------

.. todo::

   Create user journey maps and wireframes for:

   - First-time setup (Wi-Fi config → pairing → first playback)
   - Daily use: Player section (select source → play → adjust volume → sleep timer)
   - Adding an internet radio / podcast source
   - Setting an alarm
   - Creating an API token for Home Assistant
   - Sending a TTS message
   - Triggering an OTA update

   Wireframes should cover both mobile (primary) and desktop viewports.

Component pages will be created during the component interview phase.
