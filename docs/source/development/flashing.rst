Flashing a Speaker
==================

How to put the speaker image on a Pi Zero W's SD card and reach it over SSH.

1. Get the image: ``ci/output/kds-sdcard.img.xz``, built by
   ``make -C ci sd-image``.
2. In Raspberry Pi Imager, choose **Use custom**, select the image and your SD
   card, and answer **No** to customisation. Writing erases the card.
3. Remove and reinsert the card so ``KDS-BOOT`` mounts, then run:

   .. code-block:: bash

      make -C ci setup-sd

   It asks for the Wi-Fi network, password and country, and installs your SSH
   public key. Non-interactive use:
   ``KDS_WIFI_PSK=... ci/scripts/setup-sd.sh --ssid NAME --country DK``.
4. Eject the card, boot the Pi and wait about 3 minutes, then:

   .. code-block:: bash

      ssh root@kidsspeaker-XXXX.local

   ``XXXX`` is the last four hex digits of the Pi's Wi-Fi MAC address. The
   address also shows in your router.

The settings live in ``kds/`` on the boot partition, so they survive updates and
never enter the image or the repository (DEC-025). To change them, edit or rerun
``setup-sd`` with the card in your computer.

If the speaker doesn't appear on the network, check that ``kds/wifi.conf`` exists
on ``KDS-BOOT`` and that the network offers 2.4 GHz, which is all the Pi Zero W
supports.
