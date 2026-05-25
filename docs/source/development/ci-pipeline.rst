CI Pipeline — Local Image Build & Test
========================================

This document proposes approaches for building and testing Raspberry Pi OS
images locally in CI, targeting the **Pi Zero W (ARMv6)** with **RAUC** OTA
updates.

.. note::

   NixOS was evaluated but abandoned for v1 due to uncertain ARMv6 support.
   The chosen path is **Raspberry Pi OS (Debian Bookworm) + RAUC**.

The Core Problem
----------------

We need a CI pipeline that can:

1. **Build** a Raspberry Pi OS image with our software stack pre-installed
2. **Test** the image boots and services start correctly
3. **Produce** a RAUC bundle (``.raucb``) as a build artifact
4. Run on **x86_64** hardware (developer machines / GitHub Actions) — no ARM
   hardware required

The challenge: Pi OS is ARM, our CI is x86_64. We need cross-architecture
support.

Solution 1: QEMU User-Mode Emulation + Docker (Recommended)
-------------------------------------------------------------

**How it works:** Run ARM containers on x86_64 using QEMU user-mode
emulation (``binfmt_misc``). Docker images are built for ``linux/arm/v7``
and executed via transparent binary translation.

**Verified working:**

.. code-block:: bash

   # ARM container runs on x86_64 via QEMU user-mode emulation
   docker run --rm --platform linux/arm/v7 arm32v7/debian:bookworm-slim uname -m
   # Output: armv7l

   # RAUC installs and runs inside the container
   docker run --rm --platform linux/arm/v7 arm32v7/debian:bookworm-slim \
       sh -c "apt-get update -qq && apt-get install -y -qq rauc && rauc --version"
   # Output: rauc 1.8

**Pros:**

- ✅ Works today on any x86_64 Linux host with Docker
- ✅ No special hardware or cloud ARM runners needed
- ✅ Full ARM userspace — ``apt``, ``systemd``, ``rauc``, ``pip`` all work
- ✅ GitHub Actions compatible (``ubuntu-latest`` + Docker)
- ✅ Can build RAUC bundles as part of the Docker build
- ✅ Deterministic — same Dockerfile = same image

**Cons:**

- ⚠️ Slower than native (QEMU translation overhead). Acceptable for CI:
   image build ~5-15 min, not hours.
- ⚠️ Cannot test actual hardware (no GPIO, no I2S, no Wi-Fi). This is
   acceptable — hardware validation happens separately on real Pi Zero W.
- ⚠️ ARMv7 (armhf) not ARMv6. The Pi Zero W is ARMv6, but Debian's
   armhf port runs on ARMv6. The emulation is arm32v7 which is a superset.
   For CI testing of userspace services this is fine.

**Architecture:**

.. plantuml::
   :caption: CI Pipeline — QEMU User-Mode Approach
   :name: fig-ci-qemu

   @startuml
   title CI Pipeline — QEMU User-Mode Emulation

   actor "Developer" as dev
   rectangle "GitHub Actions\n(ubuntu-latest, x86_64)" as ci {
       rectangle "Stage 1: Build" as build {
           component [Dockerfile\narm32v7/debian:bookworm-slim] as dockerfile
           component [QEMU user-mode\n(binfmt_misc)] as qemu
           dockerfile --> qemu : cross-arch build
       }
       rectangle "Stage 2: Test" as test {
           component [Container boot\nwith systemd] as boot
           component [Service smoke tests\ncurl, rauc status, etc.] as smoke
           boot --> smoke : verify
       }
       rectangle "Stage 3: Bundle" as bundle {
           component [rauc bundle\n→ .raucb artifact] as raucb
       }
   }
   artifact "RAUC Bundle\n(.raucb)" as artifact

   dev --> ci : git push / PR
   build --> test
   test --> bundle
   bundle --> artifact

   @enduml

**Implementation sketch:**

.. code-block:: dockerfile

   # Dockerfile.pi-image
   FROM --platform=linux/arm/v7 arm32v7/debian:bookworm-slim

   # Install base system
   RUN apt-get update && apt-get install -y \
       rauc \
       systemd \
       alsa-utils \
       bluez \
       python3 \
       && rm -rf /var/lib/apt/lists/*

   # Install our services (API Server, Audio Player, etc.)
   COPY target/arm-unknown-linux-gnueabihf/release/api-server /usr/local/bin/
   COPY target/arm-unknown-linux-gnueabihf/release/audio-player /usr/local/bin/
   COPY target/arm-unknown-linux-gnueabihf/release/ota-updater /usr/local/bin/
   COPY target/arm-unknown-linux-gnueabihf/release/bluetooth-service /usr/local/bin/

   # Install CamillaDSP, Piper TTS, etc.
   COPY deploy/ /etc/kids-desktop-speaker/

   # systemd units
   COPY systemd/ /etc/systemd/system/

   # RAUC config
   COPY rauc/system.conf /etc/rauc/system.conf
   COPY rauc/rauc-mark-good.service /etc/systemd/system/

   ENTRYPOINT ["/sbin/init"]

**GitHub Actions workflow:**

.. code-block:: yaml

   # .github/workflows/build-image.yml
   name: Build Pi Image

   on:
     push:
       branches: [main]
     pull_request:

   jobs:
     build:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Set up QEMU
           uses: docker/setup-qemu-action@v3
           with:
             platforms: arm

         - name: Set up Docker Buildx
           uses: docker/setup-buildx-action@v3

         - name: Build Pi OS image
           run: |
             docker buildx build \
               --platform linux/arm/v7 \
               -f Dockerfile.pi-image \
               --output type=local,dest=./output \
               .

         - name: Run smoke tests
           run: |
             docker run --rm --platform linux/arm/v7 \
               --privileged \
               -v /sys/fs/cgroup:/sys/fs/cgroup:rw \
               kids-desktop-speaker:latest \
               /bin/sh -c "systemctl is-active api-server && rauc status"

         - name: Build RAUC bundle
           run: |
             docker run --rm --platform linux/arm/v7 \
               -v ./output:/output \
               kids-desktop-speaker:latest \
               /bin/sh -c "rauc bundle /output/rootfs /output/image.raucb"

         - name: Upload RAUC bundle
           uses: actions/upload-artifact@v4
           with:
             name: rauc-bundle
             path: output/image.raucb

Solution 2: QEMU System Emulation (Full VM)
---------------------------------------------

**How it works:** Run a full ARM VM with ``qemu-system-arm``, booting an
actual Raspberry Pi OS kernel. The VM has its own init system, can run
``systemd``, and behaves like a real Pi.

**Pros:**

- ✅ Most realistic emulation — boots a real kernel, real systemd, real init
- ✅ Can test kernel modules, bootloader, partition layout
- ✅ Can simulate A/B partition switching for RAUC testing
- ✅ No Docker abstraction — works with raw disk images

**Cons:**

- ❌ Requires ``qemu-system-arm`` (not installed on this system; needs
  ``qemu-system-arm`` + ``qemu-efi-arm`` packages)
- ❌ Much slower than user-mode emulation (full CPU emulation)
- ❌ Complex setup: need kernel, dtb, initrd, or a pre-built Pi OS image
- ❌ GitHub Actions compatible but slow (10-30 min per test run)
- ❌ Overkill for testing userspace services

**Best for:** Validating the RAUC A/B partition layout, bootloader
integration, and full system boot — not for every CI run.

**Implementation sketch:**

.. code-block:: bash

   # Download Raspberry Pi OS Lite image
   wget https://downloads.raspberrypi.org/raspios_lite_armhf/images/\
   raspios_lite_armhf-2024-03-15/2024-03-15-raspios-bookworm-armhf-lite.img.xz

   # Resize to add space for A/B partitions
   qemu-img resize 2024-03-15-raspios-bookworm-armhf-lite.img +2G

   # Boot in QEMU
   qemu-system-arm \
     -M versatilepb \
     -cpu arm1176 \
     -m 256 \
     -drive file=2024-03-15-raspios-bookworm-armhf-lite.img,format=raw \
     -kernel kernel.img \
     -append "root=/dev/sda2 rootfstype=ext4 rw" \
     -net nic -net user

Solution 3: Cross-Compile + pi-gen (Image Factory)
----------------------------------------------------

**How it works:** Use `pi-gen`_ (the tool behind official Raspberry Pi OS
images) to build a custom image. pi-gen runs in Docker and uses
``debootstrap`` + QEMU user-mode to create the image. Our software is
added as a custom stage.

**Pros:**

- ✅ Produces an actual Raspberry Pi OS image (flashable to SD card)
- ✅ Well-documented, used by the Raspberry Pi Foundation
- ✅ Staged build system — add our software as a custom stage
- ✅ Can produce both the test image and the RAUC bundle

**Cons:**

- ❌ pi-gen is complex and opinionated — steep learning curve
- ❌ Build times are long (30-60 min) even on fast hardware
- ❌ Designed for full Pi OS images, not minimal custom images
- ❌ Overkill if we only need to test userspace services in CI

**Best for:** Producing the final flashable SD card image for initial
setup, not for rapid CI iteration.

.. _pi-gen: https://github.com/RPi-Distro/pi-gen

Recommendation
--------------

**Start with Solution 1 (QEMU user-mode + Docker).** It's the fastest to
set up, works on any x86_64 Linux host and GitHub Actions, and covers
90% of what we need: building the software stack, running smoke tests, and
producing RAUC bundles.

**Add Solution 2 (full VM) later** for validating the RAUC A/B partition
layout and bootloader integration — this only needs to run on release
tags, not every PR.

**Add Solution 3 (pi-gen) when needed** for producing flashable SD card
images for initial device setup. This is a one-time-per-release task.

Implementation Order
--------------------

1. **Dockerfile** for the Pi OS image (Solution 1)
2. **GitHub Actions workflow** with QEMU + smoke tests
3. **RAUC bundle** production as a CI artifact
4. **Full VM boot test** for release validation (Solution 2)
5. **pi-gen integration** for SD card images (Solution 3)

See :doc:`ci-tooling` for the evaluation of Dagger vs plain Docker for
driving the pipeline.
