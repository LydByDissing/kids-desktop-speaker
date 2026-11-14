Hardware Measurements
=====================

Measured on a real Raspberry Pi Zero W (single-core ARM1176, 1 GHz, about 420 MB
free RAM with ``gpu_mem=16``), running the speaker image. CPU is a percentage of
the single core. Audio went to ALSA's null device, so these are processing costs
without the audio HAT. Reproduce with ``ci/scripts/hw-bench-audio.sh``.

Measured 2026-11-08.

Baseline
--------

Our four services together, idle: about 2.5–5 MB of memory each and close to 0%
CPU (``make -C ci hw-test``).

DSP: CamillaDSP v4.1.3
----------------------

Official ARMv6 build. 6 channels captured from ``snd-aloop`` at 48 kHz S32_LE,
mixed to 2 (the design pipeline in the CamillaDSP solution design, updated to v4
syntax: ``channels: [0, 1]`` replaces ``channel: 0``).

.. list-table::
   :header-rows: 1
   :widths: 50 20 30

   * - Pipeline
     - CPU
     - Memory
   * - Design pipeline (3 volume filters, 6→2 mixer)
     - 18.3%
     - 7 MB
   * - Plus 10 peaking EQ filters per output channel
     - 28.0%
     - 7 MB
   * - The 6-channel test signal feeding the loopback
     - 3.5%
     - 1 MB

No underruns or errors over 40 seconds with EQ. ``snd-aloop`` provides 6 channels
at 48 kHz S32_LE.

Internet radio and podcasts
---------------------------

Decoding the live DR P3 stream (MP3, 128 kbit/s) over Wi-Fi.

.. list-table::
   :header-rows: 1
   :widths: 50 20 30

   * - Player
     - CPU
     - Memory
   * - GStreamer 1.22 (``souphttpsrc ! decodebin``)
     - 8.3%
     - 22 MB
   * - MPD 0.23.12 (null output, 2026-11-10)
     - 7.9%
     - 40 MB
   * - mpv 0.35
     - 23.3%
     - 49 MB
   * - Stream player (symphonia + aplay, DEC-048, 2026-11-12)
     - 9.3% + 4.3% aplay
     - 5 MB
   * - MPD 0.23.12 (speaker output, same day, for comparison)
     - 12.3%
     - 39 MB

MPD takes 6.7 s to start on the Pi Zero W, so it is started at boot rather than
when radio is selected. It reports the stream's metadata (the current
programme) through its protocol. This confirmed DEC_005; DEC-048 later replaced
MPD with the Audio Player's own stream player (the row above).

Streaming receivers
-------------------

Playing through the ``speaker`` device (DEC-033), measured 2026-11-09/10.

.. list-table::
   :header-rows: 1
   :widths: 50 20 30

   * - Receiver
     - CPU
     - Memory
   * - shairport-sync 3.3.8 (AirPlay from PipeWire)
     - 16–18%
     - 16 MB
   * - librespot 0.8.0 (Spotify Connect, 160 kbit/s)
     - 17% (31% while a stream starts)
     - 17 MB

   * - BlueALSA 4.0 (Bluetooth A2DP from an iPad, SBC), whole stack
     - 14–15%
     - 15 MB

Each sender's volume control works (it scales the audio before it reaches the
speaker). An announcement during Spotify or Bluetooth playback stops the player
for a few seconds; it resumes as the same process after one ALSA underrun.
The phone itself does not see the pause (KDS-9jt.2 would send AVRCP pause).

An announcement during Spotify playback stops librespot for a few seconds; it
resumes as the same process after one ALSA underrun, which it recovers from.

Text-to-speech
--------------

The same 100-character sentence.

.. list-table::
   :header-rows: 1
   :widths: 40 20 20 20

   * - Engine
     - Time to synthesise
     - Speech length
     - Sample rate
   * - Piper (official ARMv7 build)
     - —
     - —
     - Illegal instruction: no ARMv6 build exists
   * - espeak-ng 1.51
     - 0.67 s
     - 6.0 s
     - 22 kHz
   * - flite 2.2, default ``kal`` voice
     - 0.49 s
     - 6.6 s
     - 8 kHz (telephone quality)
   * - flite 2.2, ``awb`` voice
     - 1.94 s
     - 6.1 s
     - 16 kHz
   * - flite 2.2, ``slt`` voice
     - 2.06 s
     - 6.2 s
     - 16 kHz

All the working engines are faster than real time. espeak-ng is fastest but sounds
robotic; flite's 16 kHz voices sound more natural at about a third of real time.

Voicing the Box with REW
------------------------

The box EQ (DSP_REQ_002, DEC-054) comes from measurements with Room EQ Wizard
and a calibrated USB measurement microphone. Each iteration is one commit.

1. **Set up.** The speaker on its desk where it will be used, the microphone
   on a stand at listening distance (about 50 cm) and height, pointing at the
   driver; no one between them. The microphone's calibration file loaded in
   REW.
2. **Play the measurement as the speaker plays music.** REW plays its sweep
   through the speaker over AirPlay (or Bluetooth) at a fixed volume below
   the bass fill's knee (DEC-053), so the fill is at its full box correction;
   50 % works. Measure with the EQ in the installed config switched off:
   ``make -C ci dsp-variant PI_HOST=... VARIANT=no-eq``. Keep the volume the
   same for every iteration.
3. **Measure several positions.** Five to nine sweeps, moving the microphone
   about 10 cm around the listening position, then REW's vector average of
   them: one position fits the EQ to its own reflections.
4. **Load the target.** In REW's EQ window, *House curve*:
   ``ci/voicing/house-curve.txt``. Equaliser *Generic*; target level so
   the curve sits on the measurement between 300 Hz and 3 kHz.
5. **Fit within the limits.** Auto EQ with at most 10 filters, match range
   60 Hz-12 kHz, maximum boost 6 dB (prefer cuts: *flatness target* loose,
   individual max boost 3-6 dB), maximum Q 10. Leave out the shelves unless
   needed.
6. **Export** the filter settings as text (*Export filter settings as text*,
   or the Equalizer APO format) and import them:
   ``make -C ci dsp-import-rew FILE=~/rew-eq.txt``. It refuses filters outside
   the limits and says which; it then checks the config offline.
7. **Listen and verify.** ``make -C ci dsp-variant PI_HOST=... VARIANT=repo``
   against ``VARIANT=no-eq``; measure once more with ``repo`` to see the
   result against the target. Commit ``ci/voicing/rew-export.txt`` and the
   config together, with the REW measurement files' names in the message.

Not yet measured
----------------

These need a client device or account:

- AirPlay (shairport-sync, packaged 3.3.8): needs an Apple device.
- Spotify Connect (librespot 0.8.0): needs an ARMv6 build and Spotify Premium.
- Bluetooth A2DP input (bluez-alsa, packaged 4.0.0): needs a phone.
- Any of the above on the audio HAT, with ``CPUQuota`` limits applied.
