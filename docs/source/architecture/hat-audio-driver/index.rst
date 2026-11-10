HAT Audio Driver
================

ALSA driver for the IQaudIO Pi-Codec Zero HAT — the hardware audio output
layer of the Kids Desktop Speaker software platform.

Hardware
--------

The `IQaudIO Pi-Codec Zero`_ is a HAT (Hardware Attached on Top) for the
Raspberry Pi Zero that provides:

- Onboard DAC (digital-to-analogue converter) with headphone amplifier
- Onboard ADC (analogue-to-digital converter)
- Stereo speaker amplifier (2× 2 W class-D)
- Line-level input and output
- Hardware volume control via ALSA mixer

.. _IQaudIO Pi-Codec Zero: https://www.iqaudio.com/accessories/75-pi-codec-zero.html


.. toctree::
   :maxdepth: 2

   solution-design

Technology Choice
-----------------

The driver layer relies on the **ALSA** (Advanced Linux Sound Architecture)
driver provided by the Linux kernel for the Dialog DA7213 codec chip
on the Pi-Codec Zero. No custom kernel driver is needed — the standard
``snd-soc-iqaudio-codec`` module is used.

Raspberry Pi OS Configuration
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

On Raspberry Pi OS, the HAT driver is configured via standard Linux mechanisms
rather than NixOS declarative configuration:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Aspect
     - Configuration Method
   * - Device tree overlay
     - ``/boot/config.txt``: ``dtoverlay=iqaudio-codec``. Standard Raspberry
       Pi OS mechanism for enabling HAT device tree overlays.
   * - Kernel module
     - ``snd-soc-iqaudio-codec`` is loaded automatically when the device tree
       overlay is present. ``snd-aloop`` is loaded via
       ``/etc/modules-load.d/snd-aloop.conf`` for the audio pipeline.
   * - ALSA mixer
     - Initial mixer state set via a systemd oneshot service that runs
       ``amixer`` commands at boot. CamillaDSP takes over volume control
       after startup.
   * - ALSA loopback
     - ``snd-aloop`` kernel module loaded with 6 channels (3 stereo pairs)
       via ``/etc/modprobe.d/snd-aloop.conf``:
       ``options snd-aloop enable=1,1,1,1,1,1,1,1 index=0 pcm_substreams=8``

All driver configuration is deployed as files in the rootfs and managed by
RAUC as part of the system image.

ALSA Loopback Channels
~~~~~~~~~~~~~~~~~~~~~~~

The ``snd-aloop`` kernel module provides a single ALSA loopback device with
6 channels (3 stereo pairs), used for the audio pipeline:

- **Channels 0–1**: Main audio (Spotify/AirPlay/Radio/Podcasts)
- **Channels 2–3**: TTS Engine
- **Channels 4–5**: Bluetooth A2DP (if enabled)

All 6 channels are captured simultaneously by CamillaDSP. The ``snd-aloop``
module supports up to 8 substreams per loopback device, so 6 channels is
well within limits. All subdevices must use the same sample rate and format;
use ``plughw`` for automatic format conversion if needed.

Interfaces
----------

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - Talks To
     - Protocol
     - Purpose
   * - CamillaDSP
     - ALSA PCM (hw:0,0)
     - Receives processed audio for playback
   * - Linux Kernel
     - Device tree / ALSA / systemd
     - Driver initialisation, mixer control, loopback config

Component pages will be created during the component interview phase.
