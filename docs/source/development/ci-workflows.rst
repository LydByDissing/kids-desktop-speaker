CI Workflow Design
==================

This document defines the GitHub Actions workflow structure for the
Kids Desktop Speaker project. The pipeline is split into two layers:
per-component PR validation and a unified release pipeline.

See :doc:`ci-pipeline` for the architectural rationale and :ref:`DEC-007
<dec-007>` / :ref:`DEC-008 <dec-008>` for the mono-repo and
cross-compilation decisions that inform this design.

.. contents::
   :local:
   :depth: 2

Overview
--------

.. code-block:: text

   ┌─────────────────── Pull Request ───────────────────────┐
   │                                                         │
   │  api-server PR CI  ──►  cross build (ARM)              │
   │  audio-player PR CI ──► cross build (ARM)              │
   │  ota-updater PR CI ──►  cross build (ARM)              │
   │  bluetooth-service PR CI ► cross build (ARM)           │
   │                                                         │
   │  Each fires only when its own folder changes.           │
   └─────────────────────────────────────────────────────────┘

   ┌─────────────────── Release (tag: v*.*.*) ───────────────┐
   │                                                         │
   │  build-api-server ──────────────────────────────────┐  │
   │  build-audio-player ────────────────────────────────┤  │
   │  build-ota-updater  ────────────────────────────────┼──► build-image
   │  build-bluetooth-service ───────────────────────────┘  │     │
   │        (parallel, all cross-compile + upload artifact)  │     ▼
   │                                                         │  smoke tests
   │                                                         │     │
   │                                                         │     ▼
   │                                                         │  rauc bundle
   │                                                         │     │
   │                                                         │     ▼
   │                                                         │  upload .raucb
   └─────────────────────────────────────────────────────────┘

Layer 1 — Per-Component PR CI
------------------------------

One workflow file per service. Each fires independently on pull requests
when files inside that component's folder change.

**Trigger**: ``pull_request`` with a ``paths`` filter.

**Purpose**: Validate that the binary compiles for the ARM target before a
PR is merged. Does not upload artifacts — artifact production is reserved
for the release pipeline.

**Workflow files**:

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - File
     - Trigger path filter
   * - ``.github/workflows/ci-api-server.yml``
     - ``api-server/**``
   * - ``.github/workflows/ci-audio-player.yml``
     - ``audio-player/**``
   * - ``.github/workflows/ci-ota-updater.yml``
     - ``ota-updater/**``
   * - ``.github/workflows/ci-bluetooth-service.yml``
     - ``bluetooth-service/**``

**Per-component workflow structure**:

.. code-block:: yaml

   name: CI — <service>

   on:
     pull_request:
       paths:
         - '<service>/**'

   jobs:
     build:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Install Rust
           uses: dtolnay/rust-toolchain@stable

         - name: Cache cargo
           uses: actions/cache@v4
           with:
             path: |
               ~/.cargo/registry
               ~/.cargo/git
               target/
             key: ${{ runner.os }}-cargo-arm-${{ hashFiles('**/Cargo.lock') }}

         - name: Install cross
           run: cargo install cross --git https://github.com/cross-rs/cross

         - name: Cross-compile for ARM
           run: cross build --target arm-unknown-linux-gnueabihf --release -p <service>

**Acceptance criteria**:

- Workflow fires only when files under ``<service>/`` change on a PR
- ``cross build`` exits 0 for each service
- No artifact is uploaded (PR builds are validation only)

Layer 2 — Release Pipeline
---------------------------

A single workflow file (``release.yml``) fires on version tag push
(``v*.*.*``). It runs all four component builds in parallel as separate
jobs, then runs the image pipeline job once all component jobs succeed.

**Trigger**: ``push`` with a tag filter ``v*.*.*``.

**Workflow file**: ``.github/workflows/release.yml``

.. code-block:: yaml

   name: Release Pipeline

   on:
     push:
       tags:
         - 'v*.*.*'

   jobs:

     # ── Component builds (parallel) ──────────────────────────
     build-api-server:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4
         - uses: dtolnay/rust-toolchain@stable
         - uses: actions/cache@v4
           with:
             path: |
               ~/.cargo/registry
               ~/.cargo/git
               target/
             key: ${{ runner.os }}-cargo-arm-${{ hashFiles('**/Cargo.lock') }}
         - run: cargo install cross --git https://github.com/cross-rs/cross
         - run: cross build --target arm-unknown-linux-gnueabihf --release -p api-server
         - uses: actions/upload-artifact@v4
           with:
             name: api-server-arm
             path: target/arm-unknown-linux-gnueabihf/release/api-server
             retention-days: 7

     build-audio-player:
       runs-on: ubuntu-latest
       steps: # ... same pattern, -p audio-player
       # artifact: audio-player-arm

     build-ota-updater:
       runs-on: ubuntu-latest
       steps: # ... same pattern, -p ota-updater
       # artifact: ota-updater-arm

     build-bluetooth-service:
       runs-on: ubuntu-latest
       steps: # ... same pattern, -p bluetooth-service
       # artifact: bluetooth-service-arm

     # ── Image pipeline (sequential, depends on all above) ─────
     build-image:
       needs:
         - build-api-server
         - build-audio-player
         - build-ota-updater
         - build-bluetooth-service
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Set up QEMU
           uses: docker/setup-qemu-action@v3
           with:
             platforms: arm

         - name: Set up Docker Buildx
           uses: docker/setup-buildx-action@v3

         - name: Download binaries
           uses: actions/download-artifact@v4
           with:
             pattern: '*-arm'
             path: ci/binaries/
             merge-multiple: true

         - name: Make binaries executable
           run: chmod +x ci/binaries/*

         - name: Build image
           run: make -C ci build

         - name: Run smoke tests
           run: make -C ci test

         - name: Build RAUC bundle
           run: make -C ci bundle

         - name: Upload RAUC bundle
           uses: actions/upload-artifact@v4
           with:
             name: rauc-bundle-${{ github.ref_name }}
             path: ci/output/image.raucb
             retention-days: 90

**Job dependency graph**:

.. plantuml::
   :caption: Release Pipeline — Job Dependencies
   :name: fig-release-pipeline

   @startuml
   title Release Pipeline — GitHub Actions Job Dependencies

   rectangle "Tag push\nv*.*.*" as trigger

   rectangle "build-api-server\n(cross compile, ARM)" as api
   rectangle "build-audio-player\n(cross compile, ARM)" as audio
   rectangle "build-ota-updater\n(cross compile, ARM)" as ota
   rectangle "build-bluetooth-service\n(cross compile, ARM)" as bt

   rectangle "build-image\n(download artifacts → docker build\n→ smoke tests → rauc bundle)" as image

   artifact "rauc-bundle-v*.*.*.raucb\n(GHA artifact, 90 days)" as bundle

   trigger --> api
   trigger --> audio
   trigger --> ota
   trigger --> bt

   api --> image : needs
   audio --> image : needs
   ota --> image : needs
   bt --> image : needs

   image --> bundle

   @enduml

Dockerfile Integration
-----------------------

The ``ci/Dockerfile.pi-image`` is updated to COPY real binaries from the
``ci/binaries/`` staging directory (populated by the ``Download binaries``
step above) rather than the current stub shell scripts.

.. code-block:: dockerfile

   # Stage: final image
   FROM arm32v7/debian:bookworm-slim

   # ... apt-get install rauc, bluez, alsa-utils, systemd ...

   # Platform service binaries (cross-compiled on x86_64 by component CI)
   COPY binaries/api-server       /usr/local/bin/api-server
   COPY binaries/audio-player     /usr/local/bin/audio-player
   COPY binaries/ota-updater      /usr/local/bin/ota-updater
   COPY binaries/bluetooth-service /usr/local/bin/bluetooth-service

During development (no real binaries yet), the stub shell scripts in
``ci/deploy/`` remain in place and the ``Dockerfile.pi-image`` continues
to use them. The switch to real binaries happens when the first component
pipeline produces an artifact.

Layer 3 — Full VM Boot Test
----------------------------

A ``qemu-system-arm`` boot test that validates the complete init path and
RAUC integration. Slower than the user-mode smoke test (~5–15 min) so it
runs on a schedule and is gated on release tags, not on every PR.

See :ref:`DEC-010 <dec-010>` for the full rationale.

**Triggers**:

- **Nightly**: ``schedule: cron: '0 2 * * *'`` on ``main``
- **Release**: ``vm-boot-test`` job added to ``release.yml``, running after
  ``build-image`` and before bundle upload

**Nightly workflow** (``.github/workflows/nightly-vm-test.yml``):

.. code-block:: yaml

   name: Nightly VM Boot Test

   on:
     schedule:
       - cron: '0 2 * * *'    # 02:00 UTC daily on main
     workflow_dispatch:        # allow manual trigger

   jobs:
     vm-boot-test:
       runs-on: ubuntu-latest
       steps:
         - uses: actions/checkout@v4

         - name: Install QEMU
           run: sudo apt-get install -y qemu-system-arm

         - name: Fetch RPi kernel + dtb (cached)
           uses: actions/cache@v4
           id: kernel-cache
           with:
             path: ci/vm/firmware/
             key: rpi-firmware-${{ hashFiles('ci/vm/firmware-pin.txt') }}

         - name: Download RPi firmware
           if: steps.kernel-cache.outputs.cache-hit != 'true'
           run: |
             mkdir -p ci/vm/firmware
             BASE=https://github.com/raspberrypi/firmware/raw/master/boot
             curl -sSL $BASE/kernel.img        -o ci/vm/firmware/kernel.img
             curl -sSL $BASE/bcm2708-rpi-zero.dtb \
                                                -o ci/vm/firmware/bcm2708-rpi-zero.dtb

         - name: Set up QEMU binfmt
           uses: docker/setup-qemu-action@v3
           with:
             platforms: arm

         - name: Set up Docker Buildx
           uses: docker/setup-buildx-action@v3

         - name: Build Pi OS image
           run: make -C ci build

         - name: Export rootfs from Docker image
           run: make -C ci export-rootfs

         - name: Run VM boot test
           run: make -C ci vm-test
           timeout-minutes: 20

**Release pipeline addition** (``release.yml`` extended):

.. code-block:: yaml

     # ── VM boot test (after build-image, before bundle upload) ───
     vm-boot-test:
       needs: build-image
       runs-on: ubuntu-latest
       steps:
         # ... same steps as nightly-vm-test.yml ...
         - name: Run VM boot test
           run: make -C ci vm-test
           timeout-minutes: 20

     # ── Bundle upload (only after VM test passes) ─────────────────
     upload-bundle:
       needs: vm-boot-test
       runs-on: ubuntu-latest
       steps:
         - uses: actions/download-artifact@v4
           with:
             name: rauc-bundle-staging
         - uses: actions/upload-artifact@v4
           with:
             name: rauc-bundle-${{ github.ref_name }}
             path: image.raucb
             retention-days: 90

**Updated release job dependency graph**:

.. plantuml::
   :caption: Release Pipeline — Full Job Dependencies (including VM boot test)
   :name: fig-release-pipeline-full

   @startuml
   title Release Pipeline — Full Job Dependencies

   rectangle "Tag push\nv*.*.*" as trigger

   rectangle "build-api-server" as api
   rectangle "build-audio-player" as audio
   rectangle "build-ota-updater" as ota
   rectangle "build-bluetooth-service" as bt
   rectangle "build-image\n(docker build + smoke tests\n+ rauc bundle)" as image
   rectangle "vm-boot-test\n(qemu-system-arm\nrauc status)" as vmtest
   artifact "rauc-bundle-v*.*.*.raucb\n(upload — 90 days)" as bundle

   trigger --> api
   trigger --> audio
   trigger --> ota
   trigger --> bt

   api --> image : needs
   audio --> image : needs
   ota --> image : needs
   bt --> image : needs

   image --> vmtest : needs
   vmtest --> bundle

   @enduml

**Makefile targets** (to be added to ``ci/Makefile``):

.. code-block:: makefile

   ## export-rootfs — Export Docker image rootfs to ci/vm/rootfs.img
   export-rootfs:
       mkdir -p $(VM_DIR)
       docker export $$(docker create --platform $(PLATFORM) \
           $(IMAGE_NAME):$(IMAGE_TAG)) | tar -C $(VM_DIR)/rootfs -xf -
       dd if=/dev/zero of=$(VM_DIR)/rootfs.img bs=1M count=1024
       mkfs.ext4 -F $(VM_DIR)/rootfs.img
       sudo mount $(VM_DIR)/rootfs.img /mnt
       sudo cp -a $(VM_DIR)/rootfs/. /mnt/
       sudo umount /mnt

   ## vm-test — Boot rootfs in qemu-system-arm and assert RAUC status
   vm-test: export-rootfs
       ci/vm/run-vm-test.sh \
           $(VM_DIR)/firmware/kernel.img \
           $(VM_DIR)/firmware/bcm2708-rpi-zero.dtb \
           $(VM_DIR)/rootfs.img

**Test script** (``ci/vm/run-vm-test.sh``) boots QEMU, waits for the serial
console to confirm ``rauc status`` completes successfully, then exits.
The script times out after 10 minutes and exits non-zero if any assertion
fails.

Caching Strategy
-----------------

.. list-table::
   :header-rows: 1
   :widths: 30 30 40

   * - Cache key
     - Scope
     - Notes
   * - ``cargo-arm-<Cargo.lock hash>``
     - Per-runner, per-target
     - Invalidated only when dependencies change
   * - Docker layer cache
     - Per-runner
     - ``apt-get install`` layer reused across runs if base image unchanged
   * - GHA artifact (7 days)
     - Per-release run
     - Binary artifacts from component jobs consumed by ``build-image`` job
   * - ``rpi-firmware-<pin hash>``
     - Per-runner
     - RPi kernel + dtb pinned by ``ci/vm/firmware-pin.txt``;
       re-downloaded only when the pin file changes

Assumptions & Open Questions
-----------------------------

* [ ] ``cross`` install step adds ~30s per job — evaluate pinning a version
  or pre-building a cached ``cross`` binary
* [ ] Artifact retention for component binaries is 7 days; adjust if
  release cadence is slower
* [ ] Branch protection rules should require the per-component CI workflows
  to pass before merging a PR
* [ ] The release tag format ``v*.*.*`` assumes SemVer — confirm this is
  the intended release convention
* [ ] Validate that ``raspi0`` QEMU machine type boots the official RPi
  kernel correctly — fall back to ``versatilepb`` if not (see :ref:`DEC-010 <dec-010>`)
* [ ] The ``export-rootfs`` target requires ``sudo`` for the bind mount step —
  evaluate running inside a Docker container with ``--privileged`` instead
  to avoid requiring root on the CI runner
