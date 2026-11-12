User Journeys and Wireframes
============================

.. note::

   **Reviewed** (KDS-9pb, 2026-11-11). The open questions were answered by the
   user and are recorded in DEC-045: Preact and TypeScript with a Storybook,
   four tabs, a reduced UI for children's devices, and QR codes for sharing
   access.

The Web UI is a phone-first web app served by the speaker over HTTPS
(WEB_REQ_001) at ``https://<hostname>.local``. It talks only to the HTTP API,
with a token (AUTH_REQ_001). It is in Danish, with English as an option
(LOC_REQ_001). Four tabs along the bottom: **Afspiller** (Player), **Radio**,
**Beskeder** (Messages) and **Indstillinger** (Settings).

Journeys
--------

1. First-time setup (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: take a new speaker into use. AUTH_REQ_003, DEC-038.*

1. The parent opens ``https://kidsspeaker-xxxx.local`` (the name is on the box,
   or found through Bonjour/mDNS) and accepts the certificate warning once.
2. The speaker has no admin yet, so the UI shows **Sæt højttaleren op**. The
   speaker is already reading a 6-digit code aloud every ~17 s; the page says so
   and offers **Læs koden igen** (asks for a fresh code, once a minute at most).
3. The parent types the code and a name for this phone ("Tues telefon") and
   taps **Fortsæt**. The UI stores the admin token in the browser and opens the
   Player. A wrong code shows the attempts left; after 5, the UI asks for a new
   code.

2. Everyday listening (child or parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: see what plays and control it. UI_REQ_002, AUD_REQ_001.*

- The Player shows the active source (AirPlay, Spotify, Bluetooth or a radio
  station) with its programme or title, big play/pause and stop buttons, and a
  volume slider that stops at the volume limit (wave 6).
- Below: the favourite radio stations as large tiles; one tap plays.
- When nothing plays: "Intet spiller" and the station tiles.

3. Send a voice message (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: speak to the child through the speaker. MSG_REQ_001-003, DEC-037.*

1. Messages tab, **Optag** (Record). The browser asks for the microphone once.
2. A ring counts down 30 s; **Stop** ends sooner.
3. Preview: play it back, **Send** or **Optag igen**. Optional name.
4. **Send**: the speaker pauses the music, chimes, plays it, chimes, resumes.
5. The list below shows earlier messages (date, duration, name). Each can be
   played again on the speaker, renamed or deleted. Unnamed ones disappear after
   90 days without being played; the list says so.

4. Announce a text (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: a quick spoken message without recording. TTS_REQ_002/003.*

On the Messages tab, under the recorder: a text field, Danish/English, **Læs
op**. The speaker speaks it with chimes.

5. Pair a Bluetooth device (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: play from a phone over Bluetooth, or pair a remote. BT_REQ_002.*

Settings, **Bluetooth**, **Par en ny enhed**: a 3-minute countdown and the
instruction "Vælg kidsspeaker-xxxx i telefonens Bluetooth-indstillinger".
Paired devices are listed.

6. Give another device access (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: the child's tablet, a grandparent's phone or Home Assistant can use the
speaker. AUTH_REQ_002.*

Settings, **Adgang**, **Ny adgang**: a name, and whether it may change settings
(admin). The UI shows the token once, with **Kopiér** and a QR code holding a
link (``https://kidsspeaker-xxxx.local/#token=...``). Scanning it on the
tablet signs that browser in; the token travels in the URL fragment, which the
browser never sends to the server. The list shows each token's name, admin
flag and when it was last used, with **Fjern**.

7. Software updates (parent)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~

*Goal: see the version and update now. OTA_REQ_007.*

Settings, **Software**: the running version, the update state (idle,
downloading, installed and waiting for the night-time restart), **Søg efter
opdatering** and **Genstart nu**. Changes arrive live (Server-Sent Events, DEC-046).

8. Sign in on a new browser
~~~~~~~~~~~~~~~~~~~~~~~~~~~

A browser without a token shows **Log ind**: scan an access QR code (journey 6)
or paste a token. A parent who lost every admin token uses **Glemt adgang?**,
which asks the speaker for a spoken code (journey 1, steps 2-3).

Wireframes
----------

Phone-width sketches; the layout widens to two columns on a tablet or laptop.

.. plantuml::
   :caption: Setup (journey 1)

   @startsalt
   {+
   <b>Sæt højttaleren op
   .
   Højttaleren læser en kode op.
   Skriv den her:
   { "      " | "      " | "      " | "      " | "      " | "      " }
   .
   Navn på denne enhed
   "Tues telefon              "
   .
   [        Fortsæt         ]
   .
   [ Læs koden igen ]
   }
   @endsalt

.. plantuml::
   :caption: Player (journey 2)

   @startsalt
   {+
   <b>Afspiller
   ==
   { <&radio> DR P3 }
   Go' Morgen P3
   .
   { [  <&media-pause>  ] | [  <&media-stop>  ] }
   .
   <&volume-low> {+ ========|------ } <&volume-high>
   ..
   <b>Radio
   { [ DR P1 ] | [ DR P2 ] | [ DR P3 ] }
   ==
   { [Afspiller] | [Radio] | [Beskeder] | [Indstillinger] }
   }
   @endsalt

.. plantuml::
   :caption: Messages (journeys 3 and 4)

   @startsalt
   {+
   <b>Beskeder
   ==
   [     <&microphone> Optag      ]
   op til 30 sekunder
   ..
   Læs en tekst op
   "Aftensmaden er klar       "
   { (X) Dansk | () Engelsk } | [ Læs op ]
   ..
   <b>Tidligere beskeder
   { Godnat     | 0:12 | i går 19:30 | [<&media-play>] | [...] }
   { (uden navn)| 0:05 | man. 08:02  | [<&media-play>] | [...] }
   ==
   { [Afspiller] | [Radio] | [Beskeder] | [Indstillinger] }
   }
   @endsalt

.. plantuml::
   :caption: Settings (journeys 5-7)

   @startsalt
   {+
   <b>Indstillinger
   ==
   { <&bluetooth> Bluetooth         | [Par en ny enhed] }
   { <&key> Adgang                  | [Ny adgang] }
   { <&cloud-download> Software v0.4.0 | [Søg efter opdatering] }
   { <&globe> Sprog                 | ^Dansk^ }
   ==
   { [Afspiller] | [Radio] | [Beskeder] | [Indstillinger] }
   }
   @endsalt

Decisions
---------

1. Four tabs as drawn.
2. A child's device (a non-admin token) sees only **Afspiller** and **Radio**.
3. Sharing access shows a QR code and a Copy button.
4. Technology: Preact and TypeScript, built in CI, with a Storybook (DEC-045).
