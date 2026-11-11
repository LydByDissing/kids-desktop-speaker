User Guide
==========

For parents setting up and using the Kids Desktop Speaker (DOC_REQ_001). The
speaker's own pages are in Danish, with English as an option.

.. note::

   The Web UI is being built (wave 5). Until it is ready, the steps marked
   *(Web UI)* are done with the HTTP API (see the API Reference).

Setting up a new speaker
------------------------

1. **Flash the SD card** with the latest release image, and run
   ``make -C ci setup-sd`` (or ``ci/scripts/setup-sd.sh``) to add your Wi-Fi
   network before the first boot (see Flashing).
2. **Power it on.** After a minute the speaker is on your Wi-Fi as
   ``kidsspeaker-xxxx`` (the last four characters differ per speaker).
3. **Open** ``https://kidsspeaker-xxxx.local`` on your phone or computer and
   accept the certificate warning once. The speaker makes its own certificate;
   the warning means your browser hasn't met it before.
4. **Listen.** Until it is set up, the speaker reads a six-digit code aloud
   ("Opsætningskode: 4, 7, 1, ... Jeg gentager: ...") every quarter of a minute.
   Type the code and a name for your phone *(Web UI)*. The speaker stops reading
   the code and your phone now has full access.

Lost access? Choose **Glemt adgang?** *(Web UI)*: the speaker reads a new code;
whoever hears it can get back in.

Playing music
-------------

Only one thing plays at a time: starting something new stops what was playing.

- **AirPlay** (iPhone, iPad, Mac, Linux with PipeWire): choose
  ``Kidsspeaker-xxxx`` as the speaker.
- **Spotify** (Premium): in the Spotify app, open the devices list and choose
  ``Kidsspeaker-xxxx``. The speaker remembers your account.
- **Bluetooth**: choose **Par en ny enhed** *(Web UI)*, then pick
  ``kidsspeaker-xxxx`` in the phone's Bluetooth settings within 3 minutes. The
  pairing is kept, also through updates.
- **Radio**: DR P1, P2 and P3 are there from the start; add others by their
  stream address *(Web UI)*.
- **Podcasts**: add a podcast by its RSS address *(Web UI)*. An episode continues
  where you stopped listening.

Messages
--------

- **Record a message** of up to 30 seconds on your phone and send it *(Web UI)*.
  The speaker pauses the music, chimes, plays your message, chimes and carries
  on. Messages are kept so you can play them again; give one a name to keep it,
  unnamed ones are removed after 90 days without being played.
- **Type a message** and the speaker reads it aloud, in Danish or English.

Sleep timer and alarms
----------------------

- **Sleep timer**: choose 1 to 240 minutes *(Web UI)*. At the end the sound fades
  out over half a minute and stops; the volume is back to normal next time.
- **Alarms**: choose a time, the days, a radio station and a volume *(Web UI)*.
  The station starts quietly and rises over a minute. Without internet the
  speaker says "Godmorgen" instead. Alarms follow Danish time, including summer
  time.

Parental controls
-----------------

- **Maximum volume**: the speaker never plays louder than the maximum, 75 % to
  begin with. Only a parent's (admin) access can change it *(Web UI)*.
- **Access for children's devices**: give a tablet its own access without admin
  rights *(Web UI)*; it can play and change the volume, not settings. Remove the
  access any time.
- **Bluetooth remote**: a paired remote's buttons change the volume (within the
  maximum), pause and resume the radio, and switch stations.

Updates
-------

The speaker updates itself at night, between 03:00 and 05:00, when nothing is
playing. If an update doesn't start properly, it goes back to the previous
version by itself. **Søg efter opdatering** checks now *(Web UI)*.

Troubleshooting
---------------

- **No sound**: check the volume on the sending phone and on the speaker, and
  that the maximum volume isn't low. Starting something else stops what played.
- **The speaker doesn't show up**: it must be on the same Wi-Fi as your phone.
  ``https://kidsspeaker-xxxx.local/health`` answers ``ok`` when it runs.
- **The setup code is not read**: it is only read until the speaker is set up.
  Use **Glemt adgang?** to get a new one.
