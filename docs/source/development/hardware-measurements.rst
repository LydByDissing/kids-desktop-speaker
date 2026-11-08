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
   * - mpv 0.35
     - 23.3%
     - 49 MB

MPD was not measured.

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

Not yet measured
----------------

These need a client device or account:

- AirPlay (shairport-sync, packaged 3.3.8): needs an Apple device.
- Spotify Connect (librespot 0.8.0): needs an ARMv6 build and Spotify Premium.
- Bluetooth A2DP input (bluez-alsa, packaged 4.0.0): needs a phone.
- Any of the above on the audio HAT, with ``CPUQuota`` limits applied.
