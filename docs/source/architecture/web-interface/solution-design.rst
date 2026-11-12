Solution Design — Web Interface
=================================

.. meta::
   :component: Web Interface
   :status: implemented
   :version: 0.2.0
   :author: wave 5 (DEC-045, DEC-046)

This document describes the Web Interface as built: a Preact and TypeScript
app (``web-ui/``), built in CI and served by the API Server (DEC-045), with
live updates from the API Server (DEC-046). The screens follow the reviewed
:doc:`journeys`.

Design Goals
------------

1. **Phone first** — the main use is a parent's phone; large touch targets,
   one column, a tab bar at the bottom. Light and dark follow the phone.
2. **Nothing built on the Pi** — static files from CI; the Pi Zero only serves
   them (DEC-045).
3. **Live** — changes made elsewhere show within 2 s (UI_REQ_004, DEC-046);
   polling keeps it working when live updates are down.
4. **Reviewable without a speaker** — every screen runs against a mocked
   speaker in Storybook and on a single-file review page.

Architecture
------------

.. plantuml::
   :caption: Web Interface — Components
   :name: fig-webui-arch

   @startuml
   skinparam componentStyle rectangle
   skinparam backgroundColor white

   title Web Interface — Components

   package "Browser (web-ui/src)" as browser {
       component ["App\nphases, tabs, toasts"] as app
       component ["Screens\nSetup, SignIn, Player,\nRadio, Messages, Settings"] as screens
       component ["Live State\nlast event per topic"] as live
       component ["ApiClient\nHttpClient | MockClient"] as client
       component ["Recorder\nWAV 24 kHz"] as rec
   }

   package "API Server" as server {
       component ["REST\n/api/*"] as rest
       component ["Event Hub\n/api/events (SSE)"] as hub
       component ["Static files\n/usr/share/kds/web"] as files
   }

   files --> browser : index.html, JS, CSS
   app --> screens
   screens --> client : commands, lists
   screens --> live : topics
   client --> rest : fetch + Bearer token
   client --> hub : fetch stream + Bearer token
   hub --> live : events
   screens --> rec : record a message
   @enduml

- **App** (``App.tsx``) decides the phase: loading, setup (no admin token yet,
  ``/health`` ``set_up: false``), sign-in, ready or offline. It reads a shared
  token from ``#token=`` and keeps the token in ``localStorage``.
- **Screens** are one per tab. A child's device (a non-admin token) sees only
  Player and Radio (DEC-045).
- **ApiClient** is an interface with two implementations: ``HttpClient``
  talks to the speaker; ``MockClient`` is a stateful pretend speaker for the
  tests, Storybook and the review page.
- **Live State** keeps the last event of each topic and whether the stream is
  connected; screens read their topic from it.

Live Updates
------------

``ApiClient.events(onEvent, onConnected)`` subscribes to ``/api/events``
(DEC-046). ``HttpClient`` reads the stream with ``fetch``, so the token goes in
the ``Authorization`` header, parses the ``event:``/``data:`` lines, and on a
dropped stream reconnects after 1, 2, 4 … up to 15 s. ``MockClient`` produces
the same events from its own state.

A screen uses ``useTopic(topic, load, every)``: it loads the endpoint once, then
takes each event of its topic. While the stream is down it falls back to
polling every ``every`` ms. After its own change a screen still reloads, so the
result shows even before the event arrives.

Counters (the sleep timer, Bluetooth pairing, the reboot countdown) tick down
in the browser from the last value (``useCountdown``); the server sends them
only when they really change.

**Update reboot banner** (OTA_REQ_010): while the ``ota`` topic has a
``reboot_at``, every screen shows "Højttaleren genstarter om m:ss for at
blive opdateret", with **Genstart nu** for a parent (OTA_REQ_007).

Offline Behaviour
-----------------

If ``/health`` doesn't answer, the app shows "Højttaleren svarer ikke. Er du på
samme Wi-Fi?" with a retry. If the live stream drops while the speaker still
answers, screens poll until it reconnects.

Build, Test and Review
----------------------

- ``npm run build``: type check and Vite build into ``web-ui/dist``; the image
  copies it to ``/usr/share/kds/web`` (``make -C ci web-ui``).
- ``npm test``: Vitest with jsdom: units, app flows and every review scenario.
- ``npm run storybook`` / ``build-storybook``: one story per scenario in
  ``src/scenarios.tsx``.
- ``npm run build-demo``: the same scenarios on one HTML page in a phone frame.
- CI uploads the Storybook and the review page with every change;
  ``make -C ci hw-web-test`` checks the served page on a real Pi.

Deferred
--------

- **Text-to-speech settings** for cloud voices: deferred with cloud TTS
  (DEC-031). The page would refuse to send credentials unless loaded over
  HTTPS.
- **Album art** and **DSP profiles**: not in v1.
