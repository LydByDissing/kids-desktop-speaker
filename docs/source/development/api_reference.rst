API Reference
=============

The Kids Desktop Speaker exposes a REST API for controlling playback and
configuration.

Base URL: ``http://<speaker-ip>:8000/api``

Authentication
--------------

Most endpoints require no authentication on the local network. Parental control
changes require a PIN:

.. code-block:: none

   POST /api/auth/verify
   Content-Type: application/json

   { "pin": "0000" }

Playback
--------

.. list-table::
   :header-rows: 1

   * - Method
     - Endpoint
     - Description
   * - ``POST``
     - ``/api/playback/play``
     - Start or resume playback
   * - ``POST``
     - ``/api/playback/pause``
     - Pause playback
   * - ``POST``
     - ``/api/playback/stop``
     - Stop playback
   * - ``POST``
     - ``/api/playback/skip``
     - Skip to next track
   * - ``POST``
     - ``/api/playback/previous``
     - Go to previous track
   * - ``GET``
     - ``/api/playback/state``
     - Get current playback state

**Example — Start Spotify playback:**

.. code-block:: none

   POST /api/playback/play
   Content-Type: application/json

   {
     "source": "spotify",
     "uri": "spotify:track:4iV5W9uYEdYUVa79Axb7Rh"
   }

Sources
-------

.. list-table::
   :header-rows: 1

   * - Method
     - Endpoint
     - Description
   * - ``GET``
     - ``/api/sources``
     - List available sources
   * - ``GET``
     - ``/api/sources/{source}/browse``
     - Browse content for a source
   * - ``GET``
     - ``/api/sources/podcasts``
     - List subscribed podcasts
   * - ``POST``
     - ``/api/sources/podcasts/subscribe``
     - Subscribe to a podcast

Volume
------

.. list-table::
   :header-rows: 1

   * - Method
     - Endpoint
     - Description
   * - ``POST``
     - ``/api/volume``
     - Set volume (0–100)
   * - ``GET``
     - ``/api/volume``
     - Get current volume

**Example:**

.. code-block:: none

   POST /api/volume
   Content-Type: application/json

   { "level": 60 }

TTS Announcements
-----------------

.. code-block:: none

   POST /api/tts/speak
   Content-Type: application/json

   { "message": "Time for bed!" }

Parental Controls
-----------------

.. list-table::
   :header-rows: 1

   * - Method
     - Endpoint
     - Description
   * - ``GET``
     - ``/api/parental``
     - Get parental control settings
   * - ``PUT``
     - ``/api/parental``
     - Update parental control settings
   * - ``POST``
     - ``/api/parental/disable``
     - Temporarily disable controls (requires PIN)

WebSocket
---------

Connect to ``ws://<speaker-ip>:8000/ws`` for real-time state updates:

.. code-block:: json

   {
     "type": "playback_state",
     "data": {
       "state": "playing",
       "source": "spotify",
       "track": "Happy",
       "artist": "Pharrell Williams",
       "volume": 60,
       "position": 45.2,
       "duration": 233.1
     }
   }
