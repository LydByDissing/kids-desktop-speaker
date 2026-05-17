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

Technology Choice
-----------------

The driver layer relies on the **ALSA** (Advanced Linux Sound Architecture)
driver provided by the Linux kernel for the Cirrus Logic WM8731 codec chip
on the Pi-Codec Zero. No custom kernel driver is needed — the standard
``snd-soc-iqaudio-codec`` module is used.

NixOS Configuration
~~~~~~~~~~~~~~~~~~~

With NixOS as the operating system, the HAT driver is configured declaratively
in the NixOS flake rather than via manual ``/boot/config.txt`` edits:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Aspect
     - NixOS Configuration
   * - Device tree overlay
     - ``hardware.raspberry-pi.config-overlays`` or
       ``boot.kernelParams`` in the NixOS flake. The ``iqaudio-codec`` overlay
       is enabled declaratively.
   * - Kernel module
     - ``boot.kernelModules`` includes ``snd-soc-iqaudio-codec``. Loaded
       automatically at boot.
   * - ALSA mixer
     - Initial mixer state set via ``systemd.services`` startup script
       (``amixer`` commands). CamillaDSP takes over volume control after
       startup.
   * - ALSA loopback
     - ``snd-aloop`` kernel module loaded with 6 channels (3 stereo pairs)
       for the audio pipeline.

All driver configuration is version-controlled in the system flake/config and
deployed atomically. See :doc:`/architecture/ota-updater/index` for the full
update mechanism.

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
use ``plughh`` for automatic format conversion if needed.

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
   * - NixOS / Linux Kernel
     - Device tree / ALSA / systemd
     - Driver initialisation, mixer control, loopback config

Component pages will be created during the component interview phase.
