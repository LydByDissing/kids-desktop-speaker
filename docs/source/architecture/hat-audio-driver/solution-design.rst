Solution Design — HAT Audio Driver
====================================

.. meta::
   :component: HAT Audio Driver
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the solution design for the HAT Audio Driver
component. It covers the ALSA configuration, device tree setup, loopback
channel provisioning, and systemd integration for the IQaudIO Pi-Codec Zero
on Raspberry Pi OS Lite (Bookworm).

Design Goals
------------

1. **Declarative configuration** — All driver setup via config files, no manual
   steps on first boot.
2. **Multi-channel loopback** — Provide a 6-channel ALSA loopback device for
   the audio pipeline (3 stereo pairs: main, TTS, Bluetooth).
3. **Deterministic boot** — systemd-managed startup sequence ensures the HAT
   and loopback are ready before CamillaDSP and other services start.
4. **Survives OTA updates** — Configuration is part of the rootfs, managed by
   RAUC.

Architecture Overview
---------------------

The HAT Audio Driver is not a running process — it is a **kernel driver +
ALSA configuration** managed by systemd. The component consists of:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Piece
     - What It Is
   * - Device tree overlay
     - Tells the kernel about the DA7213 codec on the I²C bus
   * - Kernel modules
     - ``snd-soc-iqaudio-codec`` (HAT), ``snd-aloop`` (loopback)
   * - ALSA config
     - ``/etc/asound.conf`` — defines PCM devices and mixers
   * - systemd service
     - ``kds-hat-init.service`` — routes the codec to the speaker at boot
   * - modprobe config
     - ``/etc/modprobe.d/snd-aloop.conf`` — loopback channel count

Device Tree Overlay
-------------------

The Pi-Codec Zero uses the standard IQaudIO device tree overlay. This is
configured in ``/boot/config.txt`` (on the boot partition, shared between
RAUC slots)::

   # /boot/config.txt
   dtoverlay=iqaudio-codec

This is a one-time setup on the initial SD card image. The boot partition
is shared between RAUC A/B slots, so it persists across OTA updates.

Kernel Modules
--------------

Two kernel modules are required:

``snd-soc-iqaudio-codec``
   Loaded automatically by the device tree overlay. Provides the ALSA PCM
   device (``hw:0,0``) and mixer controls for the DA7213 codec.

``snd-aloop``
   Loaded via ``/etc/modules-load.d/snd-aloop.conf`` with 6 channels
   (3 stereo pairs) configured via ``/etc/modprobe.d/snd-aloop.conf``::

      # /etc/modules-load.d/snd-aloop.conf
      snd-aloop

      # /etc/modprobe.d/snd-aloop.conf
      options snd-aloop enable=1,1,1,1,1,1 index=0 pcm_substreams=8

   This creates ``hw:Loopback,0`` with 8 subdevices, of which we use 3
   (6 channels). The first subdevice (``hw:Loopback,0,0``) is the "playback"
   side — sources write here. The ``hw:Loopback,0`` without a subdevice
   index is the "capture" side — CamillaDSP reads all channels from here.

   .. note::

      The ``enable=1,1,1,1,1,1`` enables 6 subdevices. The remaining 2
      (of 8 total) are disabled. ``index=0`` ensures the device is
      ``hw:Loopback,0`` (the first loopback device).

ALSA Configuration
------------------

The ALSA configuration in ``/etc/asound.conf`` defines named PCM devices
that all components use:

.. code-block:: bash

   # /etc/asound.conf
   # Kids Desktop Speaker — ALSA device definitions
   # =================================================

   # Hardware output: IQaudIO Pi-Codec Zero
   pcm.hat {
       type hw
       card 0
       device 0
   }

   # Loopback capture: all 6 channels (what CamillaDSP reads)
   pcm.loopback_capture {
       type hw
       card "Loopback"
       device 0
   }

   # Loopback playback subdevices (what sources write to)
   pcm.loopback_main {
       type plug
       slave {
           pcm "hw:Loopback,0,0"
           format S32_LE
           rate 48000
           channels 2
       }
   }

   pcm.loopback_tts {
       type plug
       slave {
           pcm "hw:Loopback,0,1"
           format S32_LE
           rate 48000
           channels 2
       }
   }

   pcm.loopback_bt {
       type plug
       slave {
           pcm "hw:Loopback,0,2"
           format S32_LE
           rate 48000
           channels 2
       }
   }

   # Mixer for the HAT hardware
   ctl.hat {
       type hw
       card 0
   }

   # Default device (used by ALSA clients that don't specify)
   pcm.!default {
       type plug
       slave.pcm "hat"
   }

   ctl.!default {
       type hw
       card 0
   }

All sources use ``plughw`` (via the ``plug`` plugin) for automatic format
conversion. This means sources can write at different sample rates and
ALSA will convert to the loopback device's rate (48000 Hz).

.. important::

   The first source to open a loopback subdevice sets the sample rate and
   format for the entire loopback device. All subsequent sources must use
   the same rate or rely on the ``plug`` plugin for conversion. We standardize
   on **48000 Hz, S32_LE** for all channels.

Boot Initialization
-------------------

Resolved on real hardware: the DA7213 control names below are the real ones, and
48 kHz plays correctly, so sources and CamillaDSP use 48 kHz.

The DA7213 codec starts with its outputs unrouted, so the HAT is silent until its
mixer is set. ``kds-hat-init.service`` runs ``/usr/local/bin/kds-hat-init.sh``,
started by a udev rule when the HAT's sound card appears, because the card is
probed late in boot. ALSA's own mixer save and restore is disabled: on a real
boot it restored a stale state after the script ran and silenced the speaker. Verified on
a real Pi-Codec Zero with a speaker on 2026-11-09.

.. list-table::
   :header-rows: 1
   :widths: 45 20 35

   * - Control
     - Value
     - Why
   * - ``DAC Left/Right Source MUX``
     - DAI Input Left/Right
     - Play what the Pi sends over I²S
   * - ``DAC Mono Switch``
     - on
     - One speaker: mix left and right
   * - ``DAC Volume``
     - 112
     - Fixed; volume is handled in software
   * - ``Mixout Left/Right DAC Switch``
     - on
     - Route the DAC to the output mixer
   * - Other ``Mixout`` inputs (Aux, Mixin)
     - off
     - Nothing but the DAC reaches the speaker
   * - ``Lineout Switch`` / ``Lineout Volume``
     - on / 48
     - The line output drives the speaker
   * - ``Headphone Switch``
     - off
     - Unused

``/etc/asound.conf`` makes ``speaker`` the default ALSA device. It resamples to
48 kHz, because the codec rejects some rates, such as 22.05 kHz, instead of
converting them.

The overlay ``iqaudio-codec.dtbo`` and the ``dtoverlay`` line in ``config.txt`` are
part of the boot files that updates carry (DEC-029).
Startup Dependencies
--------------------

The HAT must be ready before CamillaDSP and the Audio Player start:

.. code-block:: text

   sound.target
       └── hat-init.service (oneshot, sets mixer)
           └── camilladsp.service (requires hat-init)
           └── audio-player.service (requires camilladsp)
           └── tts-engine.service (requires camilladsp)

Device Summary
--------------

.. list-table::
   :header-rows: 1
   :widths: 30 20 50

   * - ALSA Device
     - Purpose
     - Notes
   * - ``hw:0,0`` / ``hat``
     - IQaudIO Pi-Codec Zero hardware output
     - Stereo playback to speakers
   * - ``hw:Loopback,0`` / ``loopback_capture``
     - Loopback capture
     - 6 channels, read by CamillaDSP
   * - ``hw:Loopback,0,0`` / ``loopback_main``
     - Loopback playback subdevice 0
     - Main audio (Spotify/AirPlay/Radio)
   * - ``hw:Loopback,0,1`` / ``loopback_tts``
     - Loopback playback subdevice 1
     - TTS engine output
   * - ``hw:Loopback,0,2`` / ``loopback_bt``
     - Loopback playback subdevice 2
     - Bluetooth A2DP (reserved for v2)

Open Questions
--------------

.. todo::

   - **Loopback timing**: Test that the ``snd-aloop`` module reliably creates
     all 6 channels on every boot. If not, we may need a systemd service to
     verify and retry.


     **Status (2026-11-11)**: Obsolete: the loopback carries one stereo stream (DEC-036); snd-aloop loads at boot and CamillaDSP retries until the HAT is there.