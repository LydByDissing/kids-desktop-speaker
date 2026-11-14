API Reference
=============

The speaker's HTTP API, as implemented (DOC_REQ_003). The Web UI, scripts and
Home Assistant all use it.

Finding the speaker and connecting
----------------------------------

- **Address**: ``https://<hostname>.local``, for example
  ``https://kidsspeaker-2df5.local``. The speaker advertises ``_kidsspeaker._tcp``
  and ``_https._tcp`` on port 443 over mDNS (DSC_REQ_001, DEC-043):
  ``avahi-browse -r _kidsspeaker._tcp`` lists the speakers on the network.
- **HTTPS**: each speaker has its own self-signed certificate for
  ``<hostname>.local`` (WEB_REQ_001). Download it once
  (``openssl s_client -connect kidsspeaker-2df5.local:443 </dev/null | openssl
  x509 > speaker.pem``) and verify against it, or accept it in the browser.
- **Plain HTTP** on port 8080 works with a token too, except where HTTPS is
  required (managing tokens, the volume limit). It exists for health checks and
  the device itself; use HTTPS from other devices.

The Web UI
----------

The speaker serves its Web UI at ``/`` (DEC-045), without a token: the page is
public, the data behind it isn't. Any path the API doesn't route gets the page,
so the app can use its own paths; unknown ``/api/...`` paths answer a JSON
``404``. The page is sent with a Content-Security-Policy that allows only the
speaker's own scripts, ``X-Content-Type-Options: nosniff`` and
``Referrer-Policy: no-referrer``. A link of the form
``https://<hostname>.local/#token=<token>`` signs a device in; the token stays
in the fragment, so it never reaches the server's logs.

Authentication
--------------

Every request needs ``Authorization: Bearer <token>`` (AUTH_REQ_001), except
``/health``, ``POST /api/setup/start``, ``POST /api/setup`` and requests from the
device itself. Without a valid token the answer is ``401``.

**The first token**: a new speaker reads a 6-digit code aloud until it is set
up (AUTH_REQ_003). Exchange it for an admin token:

.. code-block:: none

   POST /api/setup
   {"code": "471293", "name": "Tue's phone"}

   201 {"token": "kds_9f2c...", "info": {"id": "a1b2c3d4e5f6", "name": "Tue's phone", "admin": true, ...}}

The token is shown only in this reply. ``POST /api/setup/start`` asks the speaker
to read a new code (at most once a minute), also to recover a lost admin token.
A wrong code answers ``403`` with ``attempts_left``; after 5, ask for a new one.

**More tokens** (admin, over HTTPS): ``POST /api/tokens {"name": "Home
Assistant", "admin": false}`` returns a new token once; ``GET /api/tokens``
lists them without secrets; ``DELETE /api/tokens/{id}`` revokes one (not the
last admin token).

Errors
------

Errors are JSON: ``{"error": "..."}``. ``400`` or ``422``: the request was
wrong. ``401``: no valid token. ``403``: an admin token or HTTPS is needed.
``404``: no such station, message, podcast, episode, alarm or token. ``429``:
too soon (setup code) or the announcement queue is full. ``502``: a backend (MPD,
a feed, the network) failed. ``503``: a service on the speaker isn't running.

Endpoints
---------

.. list-table::
   :header-rows: 1
   :widths: 10 32 58

   * - Method
     - Path
     - Body and reply
   * - GET
     - ``/health``
     - No token. ``{"status": "ok", "version": "...", "set_up": true}``;
       ``set_up`` is false until the first admin token exists (the Web UI then
       shows setup).
   * - GET
     - ``/api/events``
     - Live updates (DEC-046): a ``text/event-stream`` that stays open. First
       every topic, then each topic when it changes: ``source``, ``volume``,
       ``timer``, ``bluetooth``, ``ota``, each with the JSON of its endpoint.
       Try ``curl -N -H 'Authorization: Bearer ...' https://.../api/events``.
   * - GET
     - ``/api/system/time``
     - The speaker's clock.
   * - GET
     - ``/api/audio/source``
     - ``{"active": "radio" | "airplay" | "spotify" | "bluetooth" | null,
       "sources": {"radio": {"state", "station", "title"}, ...}}``
   * - GET, POST
     - ``/api/audio/volume``
     - ``{"volume": 0-100}``; capped by the limit. Reply ``{"volume", "limit",
       "db"}`` (VOL_REQ_001).
   * - GET, POST
     - ``/api/audio/bass``
     - The bass fill over the volume (DEC-053): ``{"max_db", "top_db"}``,
       ``top_db`` at 100 % and 1 dB more per dB of lower gain, up to
       ``max_db``. POST needs an admin token over HTTPS.
   * - GET, POST
     - ``/api/audio/volume-limit``
     - ``{"limit": 0-100}``; POST needs an admin token over HTTPS.
   * - POST
     - ``/api/tts``
     - ``{"text": "...", "lang": "da-DK" | "en-GB"}`` (Danish if omitted).
       ``202 {"position": n}``: spoken with chimes, pausing the music.
   * - GET, POST
     - ``/api/radio/stations``
     - List; add ``{"name", "url"}`` (http/https stream), ``201``.
   * - DELETE
     - ``/api/radio/stations/{id}``
     - Remove a station.
   * - POST
     - ``/api/radio/play``, ``/api/radio/stop``
     - ``{"id": "dr-p3"}`` to play; stop takes no body.
   * - GET, POST
     - ``/api/podcasts``
     - List feeds; add ``{"url": "<rss>"}`` (the title is read from the feed).
   * - DELETE
     - ``/api/podcasts/{id}``
     - Remove a feed.
   * - GET
     - ``/api/podcasts/{id}/episodes``
     - The latest 50, newest first, each with ``position_s`` if part-heard.
   * - POST
     - ``/api/podcasts/play``, ``/api/podcasts/stop``
     - ``{"feed", "episode"}``: continues where it stopped. Stop saves the
       position.
   * - GET, POST
     - ``/api/messages``
     - List recorded messages; upload a 16-bit PCM WAV (at most 30 s) as the
       body. Plays at once unless ``?play=false``; ``?name=`` names it.
   * - PATCH, DELETE
     - ``/api/messages/{id}``
     - ``{"name": "..."}`` (empty removes the name); delete.
   * - POST, GET
     - ``/api/messages/{id}/play``, ``/api/messages/{id}/audio``
     - Play again on the speaker; download the WAV.
   * - GET, POST, DELETE
     - ``/api/timer``
     - Sleep timer: ``{"minutes": 1-240}``; reply ``{"running", "minutes",
       "remaining_s"}``. At the end the sound fades out and stops (SLP_REQ_001).
   * - GET, POST
     - ``/api/alarms``
     - ``{"time": "07:00", "days": [1, 2, 3, 4, 5], "station": "dr-p3",
       "volume": 50, "enabled": true, "name": "..."}``; days are 1 (Monday) to
       7, empty is every day; local time (ALM_REQ_001).
   * - PUT, DELETE
     - ``/api/alarms/{id}``
     - Replace or remove an alarm.
   * - GET, POST
     - ``/api/bluetooth/status``, ``/api/bluetooth/pair``
     - Status, including an open pairing window; open a 3-minute window
       (``{"seconds"}`` optional).
   * - GET, POST
     - ``/api/ota/status``, ``/api/ota/check``, ``/api/ota/reboot``
     - Update state ``{"state", "detail", "version", "pending", "reboot_at"}``;
       check now; reboot now (OTA_REQ_007). ``reboot_at`` (Unix seconds) is
       set while an update's reboot waits out its grace period (OTA_REQ_010).
   * - POST
     - ``/api/setup/start``, ``/api/setup``
     - No token: see Authentication.
   * - GET, POST, DELETE
     - ``/api/tokens``, ``/api/tokens/{id}``
     - Admin over HTTPS: see Authentication.

Home Assistant
--------------

Create a token for Home Assistant (``POST /api/tokens {"name": "Home
Assistant"}``) and store it in ``secrets.yaml`` as ``kids_speaker_token:
"Bearer kds_..."``. Then in ``configuration.yaml``:

.. code-block:: yaml

   rest:
     - resource: https://kidsspeaker-2df5.local/api/audio/source
       verify_ssl: false   # the speaker's certificate is self-signed
       headers:
         Authorization: !secret kids_speaker_token
       scan_interval: 15
       sensor:
         - name: Kids speaker source
           value_template: "{{ value_json.active or 'idle' }}"
         - name: Kids speaker programme
           value_template: "{{ value_json.sources.radio.title }}"

   rest_command:
     kids_speaker_announce:
       url: https://kidsspeaker-2df5.local/api/tts
       method: POST
       verify_ssl: false
       headers:
         Authorization: !secret kids_speaker_token
       content_type: application/json
       payload: '{"text": "{{ text }}", "lang": "da-DK"}'
     kids_speaker_radio:
       url: https://kidsspeaker-2df5.local/api/radio/play
       method: POST
       verify_ssl: false
       headers:
         Authorization: !secret kids_speaker_token
       content_type: application/json
       payload: '{"id": "{{ station }}"}'

Home Assistant's ``verify_ssl`` takes only true or false, so it cannot pin the
speaker's own certificate; ``false`` still encrypts, but doesn't prove it talks
to the speaker, so keep it to your own network. An automation can then announce
dinner with
``service: rest_command.kids_speaker_announce`` and ``data: {text: "Aftensmaden
er klar"}``.
