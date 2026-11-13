Architecture Decision Records
==============================

This section captures significant architecture decisions made during the design
of the Kids Desktop Speaker software platform. Each decision includes the
context, options considered, and the rationale for the chosen path.

.. contents::
   :local:
   :depth: 2

OS & OTA Strategy
-----------------

.. _dec-001:

DEC-001 — Operating System Selection
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_001
   :status: resolved
   :date: 2026-05-25
   :tags: os, ota, infrastructure

**Decision**: Use **Raspberry Pi OS Lite (Bookworm)** as the base operating
system, with **RAUC** for atomic OTA updates.

**Context**

The OS choice determines the OTA mechanism. The platform runs on a Raspberry
Pi Zero W (ARMv6). The initial plan used NixOS for its declarative
configuration and atomic upgrades, but ARMv6 support was uncertain.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 10 20 30 40

   * - Option
     - OS
     - OTA Mechanism
     - Trade-offs
   * - A
     - NixOS
     - ``nixos-rebuild switch``
     - Declarative, atomic, rollback built in. **Risk: ARMv6 support
       uncertain.** Community project (nixos-rpi-zero-w) last updated 2022.
   * - B
     - Raspberry Pi OS
     - RAUC_ (A/B partitions)
     - **CHOSEN.** ARMv6 proven. A/B partition scheme with atomic switch.
       Rollback = switch back. Pi bootloader backend available.
   * - C
     - Raspberry Pi OS
     - Mender_ (A/B partitions)
     - ARMv6 proven. Similar A/B approach. Managed cloud option available.
       More complex than RAUC.
   * - D
     - Yocto + RAUC
     - RAUC (A/B partitions)
     - Full control over image. Reproducible builds. Steep learning curve.
       Overkill for v1.
   * - E
     - Raspberry Pi OS
     - Custom script (no atomic)
     - Simplest. No atomic updates, no rollback. Risk of broken state.

**Rationale**

NixOS (Option A) was evaluated but ARMv6 support could not be confirmed
with current NixOS releases. The community project demonstrating NixOS on
Pi Zero W dates from 2022 and has not been maintained. Without confidence
in the base OS, building OTA on top of it was too risky.

Option B (Pi OS + RAUC) was chosen because:

- ARMv6 is fully proven — Raspberry Pi OS officially supports Pi Zero W
- RAUC provides atomic A/B updates with rollback, matching the reliability
  properties we need
- The A/B partition scheme is well-understood and battle-tested on embedded
  Linux devices
- RAUC has a `Pi bootloader backend`_ for partition switching

The trade-off is losing NixOS's declarative configuration. Services will be
managed via systemd unit files and deployment scripts instead.

**Consequences**

- All container pages must be updated to reference Pi OS + systemd instead
  of NixOS
- The OTA Updater container uses ``rauc install`` instead of
  ``nixos-rebuild switch``
- The HAT Audio Driver uses standard ALSA/systemd configuration instead of
  NixOS device tree overlays
- A new RAUC configuration (``system.conf``, slot layout) must be created
- The CI pipeline must build Pi OS images, not NixOS closures

.. _RAUC: https://github.com/rauc/rauc
.. _Mender: https://github.com/mendersoftware/mender
.. _Pi bootloader backend: https://github.com/Rtone/raspberrypi-firmware-rauc-bootloader-backend

CI Pipeline
-----------

.. _dec-002:

DEC-002 — CI Pipeline Architecture
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_002
   :status: resolved
   :date: 2026-05-25
   :tags: ci, infrastructure, ota

**Decision**: Use **Docker + QEMU user-mode emulation** (Solution 1) for
the CI pipeline, and **pi-gen** (Solution 3) for producing flashable SD card
images. Drive the pipeline with **plain Docker + shell scripts** (not Dagger)
for v1.

**Context**

The CI pipeline must build and test Raspberry Pi OS images on x86_64
hardware (developer machines and GitHub Actions). No ARM hardware is
available in CI. The pipeline needs to build images, run smoke tests, and
produce RAUC bundles.

**Pipeline Architecture Options**

.. list-table::
   :header-rows: 1
   :widths: 15 25 35 25

   * - Solution
     - Approach
     - Pros
     - Cons
   * - 1
     - Docker + QEMU user-mode
     - Fast, works today, GitHub Actions compatible, full ARM userspace
     - Cannot test actual hardware (no GPIO/I2S/Wi-Fi)
   * - 2
     - QEMU system emulation (full VM)
     - Most realistic — boots real kernel, real systemd, tests bootloader
     - Slow (10-30 min), complex setup, needs kernel/dtb files
   * - 3
     - pi-gen (image factory)
     - Produces flashable SD card images, well-documented, staged builds
     - Long build times (30-60 min), steep learning curve, opinionated

**Pipeline Driver: Dagger vs Plain Docker**

The pipeline driver orchestrates the build → test → bundle steps. Two
options were evaluated (see :doc:`ci-tooling` for full analysis):

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Tool
     - Pros
     - Cons
   * - Dagger (Python SDK)
     - Programmatic pipelines, content-addressable caching, secret
       management, step-by-step observability
     - New dependency (Dagger engine + Python SDK), overkill for 3 linear
       steps, harder to debug locally, version churn history
   * - Plain Docker + shell
     - Zero new dependencies, transparent (every step is a visible docker
       command), works out of the box on GitHub Actions and developer laptops
     - No built-in caching beyond Docker layers, no structured secret
       management, shell scripts can grow messy

**Rationale**

Solution 1 (Docker + QEMU user-mode) is the primary CI mechanism because:

- It works on any x86_64 Linux host with Docker — no special packages needed
- ARM containers run via transparent QEMU user-mode emulation (``binfmt_misc``)
- Verified: arm32v7 Debian containers run on x86_64, RAUC 1.8 installs and
  operates correctly
- Build times are acceptable (~5-15 min for image build)
- GitHub Actions compatible via ``docker/setup-qemu-action``
- Smoke tests (service status, RAUC status) run inside the container

Plain Docker + shell scripts is chosen over Dagger for v1 because:

- The pipeline is only 3 linear steps (build, test, bundle) — Dagger's DAG
  scheduling and caching shine for complex multi-stage pipelines, not simple
  linear ones
- Every CI step is a visible ``docker`` command — when a build fails, reproduce
  it by pasting the command. No Dagger engine needed locally.
- Zero new dependencies — Docker is already required. No Dagger CLI, no Python
  SDK, no engine lifecycle to manage.
- Dagger can be layered on later without changing the Dockerfile or fundamental
  approach if the pipeline grows complex enough to justify it.

Solution 3 (pi-gen) is used for producing the final flashable SD card image
for initial device setup. This is a one-time-per-release task, not a
per-PR requirement.

Solution 2 (full VM) is deferred. It may be added later for validating the
RAUC A/B partition layout and bootloader integration on release tags.

**Consequences**

- A ``Dockerfile.pi-image`` will define the Pi OS image with all platform
  services
- A ``ci/Makefile`` with ``build``, ``test``, ``bundle`` targets drives the
  pipeline using plain ``docker buildx build`` and ``docker run`` commands
- A GitHub Actions workflow calls the Makefile targets; RAUC bundle is only
  produced on pushes to ``main``, not on every PR
- pi-gen integration will be added as a separate workflow for release tags
- Hardware validation (audio output, Bluetooth, Wi-Fi) still requires real
  Pi Zero W hardware
- Dagger can be introduced later if the pipeline grows beyond ~10
  interdependent steps or caching becomes a bottleneck

RAUC Partition Layout
---------------------

.. _dec-003:

DEC-003 — RAUC A/B Partition Layout
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_003
   :status: resolved
   :date: 2026-05-25
   :tags: ota, storage, infrastructure

**Decision**: Use RAUC with an A/B partition scheme for atomic system
updates with automatic rollback.

**Context**

With NixOS no longer the OS, the OTA mechanism must be RAUC. RAUC requires
an A/B partition layout: two root filesystem slots (A and B). Updates are
written to the inactive slot, and the bootloader switches to it on reboot.

**Partition Layout**

.. list-table::
   :header-rows: 1
   :widths: 20 15 25 40

   * - Partition
     - Size
     - Purpose
     - Notes
   * - boot (FAT32)
     - 256 MB
     - Bootloader, kernel, device trees
     - Shared between slots
   * - rootfs-a (ext4)
     - 1.5 GB
     - Root filesystem slot A
     - Active slot
   * - rootfs-b (ext4)
     - 1.5 GB
     - Root filesystem slot B
     - Inactive slot (update target)
   * - data (ext4)
     - Remainder
     - User data, configs, voice models
     - Persistent across updates, not touched by RAUC

**Rationale**

- A/B partitions provide atomic updates: the active slot is never modified
- Rollback is a bootloader-level operation — switch back to the previous slot
- The data partition is separate and persistent: user configs, radio stations,
  podcast feeds, alarms, API tokens, Bluetooth pairings, and Piper voice
  models survive updates
- 1.5 GB per rootfs slot is sufficient for Pi OS Lite + our services

**Consequences**

- The OTA Updater container calls ``rauc install <bundle>`` instead of
  ``nixos-rebuild switch``
- A ``system.conf`` RAUC configuration file defines the slot layout
- A ``rauc-mark-good.service`` systemd unit marks the current slot as good
  after successful boot
- The initial SD card image must be created with the A/B partition layout
  (handled by pi-gen)

Audio
-----

.. _dec-004:

DEC-004 — AirPlay Receiver
~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_004
   :status: resolved
   :date: 2026-05-25
   :tags: audio, airplay

**Decision**: Use **shairport-sync** (C) as a subprocess for v1.

**Context**

The Audio Player needs an AirPlay receiver. Two Rust libraries exist
(rairplay, shairplay) but both are early-stage. The de facto standard is
shairport-sync (C).

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 30 30

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - rairplay (Rust)
     - All-Rust stack, AP2 support
     - Early-stage (22 downloads), unproven
   * - B
     - shairplay (Rust)
     - Pure Rust
     - Very new (0 downloads), WIP fork
   * - C
     - shairport-sync (C)
     - **CHOSEN.** Battle-tested, 8 650 stars, actively maintained
     - C dependency, AP1 only

**Rationale**

shairport-sync is the only mature option. It outputs to ALSA directly,
advertises via mDNS/Bonjour automatically, and is proven on Raspberry Pi.
The Audio Player manages it as a child process.

**Consequences**

- shairport-sync is installed as a system package (not a Rust crate)
- The Audio Player starts/stops it as a child process
- shairport-sync.conf configures ALSA output to the loopback device
- Can be replaced with a Rust library later without architecture changes

.. _dec-005:

DEC-005 — Internet Radio & Podcast Backend
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_005
   :status: resolved
   :date: 2026-05-25
   :tags: audio, radio, podcast

**Superseded by DEC-048 (2026-11-12)**: the Audio Player now plays radio and
podcasts itself; MPD and its dependencies are gone from the image.

**Decision**: Use **MPD** (Music Player Daemon) as a backend, controlled
from the Audio Player via the MPD protocol.

**Context**

The Audio Player needs to play internet radio streams and podcast episodes.
Volumio, moOde, and RuneAudio all use MPD successfully for this purpose.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 30 30

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - MPD (C daemon)
     - **CHOSEN.** Proven, handles all stream formats, playlist management
     - C dependency, separate daemon
   * - B
     - Custom Rust (rodio + stream-download)
     - Full Rust control
     - More engineering effort, format support risk on Pi Zero W
   * - C
     - GStreamer (Rust bindings)
     - Handles all formats, proven pipeline
     - Heavy dependency, complex cross-compilation

**Rationale**

MPD is proven, handles HTTP streams natively, and has a mature Rust client
crate (``mpd_client`` v1.4.1). The Audio Player controls it via the MPD
protocol on localhost:6600.

**Consequences**

- MPD runs as a separate systemd service
- MPD's data (database, playlists) lives on the data partition
- The Audio Player uses ``mpd_client`` to control playback and playlists
- Radio stations and podcast feeds are managed as MPD playlist entries

Repository & CI
---------------

.. _dec-007:

DEC-007 — Mono-repo Layout & Per-Component CI Pipelines
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_007
   :status: resolved
   :date: 2026-06-21
   :tags: ci, infrastructure, rust, monorepo

**Decision**: Use a **mono-repo** with per-component folders, each with its
own CI pipeline that cross-compiles the Rust binary independently. The main
image pipeline depends on all component pipelines and COPYs their pre-built
binaries into the Dockerfile.

**Context**

The four platform services (api-server, audio-player, ota-updater,
bluetooth-service) are written in Rust and must be cross-compiled from
x86_64 to ``arm-unknown-linux-gnueabihf`` (ARMv7/ARMv6 compatible). The CI
runs on GitHub Actions ``ubuntu-latest`` (x86_64). Two layout strategies
were considered: mono-repo and separate repositories per service.

The Dockerfile (``ci/Dockerfile.pi-image``) currently uses stub shell
scripts as placeholders. When real binaries are ready, the build pipeline
must supply them. Two cross-compilation strategies were evaluated:

- **Option A — Host-side cross-compile, COPY in**: each component's CI
  pipeline builds the binary on the runner and uploads it as a GHA artifact.
  The image pipeline downloads the artifacts and COPYs them into a
  single-stage Dockerfile.
- **Option B — Multi-stage Docker build**: a builder stage inside the
  Dockerfile runs ``cargo build`` inside a cross-compilation image, then a
  final stage COPYs the output.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 10 30 30 30

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - Mono-repo, per-component CI, host-side cross-compile (COPY binaries in)
     - **CHOSEN.** Rust build cache lives on the runner — fast incremental
       builds. Component pipelines are independent — a change to audio-player
       does not rebuild api-server. Clear ownership boundaries.
     - Image pipeline must download artifacts from upstream runs. Artifact
       retention policy must be managed.
   * - B
     - Separate repos per service
     - Maximum isolation; each team owns its repo
     - Cross-repo dependency management overhead; harder to make atomic
       changes across services; no benefit for a single-person or small team
   * - C
     - Multi-stage Docker build (cross-compile inside Docker)
     - Fully hermetic — one ``docker build`` produces everything
     - Rust build cache inside Docker layers is harder to persist across CI
       runs; entire workspace rebuilds on any change; slower iteration

**Rationale**

Mono-repo with per-component CI (Option A) was chosen because:

- A change to ``audio-player`` only triggers the audio-player pipeline.
  The image pipeline re-runs only if a new binary artifact is available.
- Rust incremental compilation is most effective when the build cache
  (``~/.cargo`` and ``target/``) is preserved between runs. On a GitHub
  Actions runner this is done via ``actions/cache``. Inside Docker layers
  it requires ``--mount=type=cache`` and more complex cache key management.
- The cross-compilation toolchain (``cross`` crate or ``cargo`` +
  ``arm-linux-gnueabihf-gcc``) is well-supported in GitHub Actions.
- ``cross`` is the recommended tool: it wraps ``cargo`` and handles the
  ARM toolchain automatically via a Docker image, without requiring the
  cross-compiler to be installed on the runner.

**Repository Layout**

.. code-block:: text

   kids-desktop-speaker/          ← mono-repo root
   ├── Cargo.toml                 ← workspace root (members: api-server, ...)
   ├── api-server/                ← Rust crate
   │   └── src/
   ├── audio-player/              ← Rust crate
   │   └── src/
   ├── ota-updater/               ← Rust crate
   │   └── src/
   ├── bluetooth-service/         ← Rust crate
   │   └── src/
   ├── ci/                        ← Image build pipeline
   │   ├── Dockerfile.pi-image
   │   ├── Makefile
   │   └── create-bundle.sh
   └── docs/                      ← Documentation (Sphinx)

**Pipeline Dependency Chain**

.. code-block:: text

   api-server CI ──────────────────────────┐
   audio-player CI ────────────────────────┤
   ota-updater CI  ────────────────────────┼──► Image Pipeline (build → test → bundle)
   bluetooth-service CI ───────────────────┘

Each component CI pipeline:

1. Triggers on ``push`` / ``pull_request`` when files under its folder change
2. Runs ``cross build --target arm-unknown-linux-gnueabihf --release``
3. Uploads the binary as a GHA artifact (name: ``<service>-arm``)

The image pipeline:

1. Triggers after all component pipelines complete successfully (via
   ``workflow_run`` or manual dispatch for v1)
2. Downloads all four binary artifacts
3. Runs ``make build`` (``docker buildx build --platform linux/arm/v7``,
   which COPYs the binaries in)
4. Runs ``make test`` (smoke tests inside the ARM container)
5. On push to ``main``: runs ``make bundle`` to produce the ``.raucb``
   artifact

**Consequences**

- The Cargo workspace root ``Cargo.toml`` must list all four crates as members
- Each component folder gets a ``Cargo.toml`` with ``[package]`` and
  ``[[bin]]`` sections
- ``Dockerfile.pi-image`` is updated to ``COPY`` real binaries from a
  ``binaries/`` staging directory rather than stub shell scripts
- The image pipeline workflow adds a ``Download artifacts`` step before
  ``make build``
- GHA artifact retention must cover the window between component CI and
  image pipeline runs (default 90 days is sufficient)
- The ``cross`` crate is added to developer setup instructions
- ``DEC-002`` (CI pipeline architecture) is extended by this decision —
  the pipeline driver remains plain Docker + shell scripts

**Consequences**

- Stub shell scripts in ``ci/deploy/`` remain in place until the first
  real Rust binary is built and uploaded by a component pipeline

.. _dec-008:

DEC-008 — Rust Cross-Compilation Toolchain
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_008
   :status: resolved
   :date: 2026-06-21
   :tags: ci, rust, cross-compilation

**Decision**: Use the **``cross`` crate** for Rust cross-compilation to
``arm-unknown-linux-gnueabihf`` in CI and locally.

**Context**

Cross-compiling Rust to ARMv7/ARMv6 requires a C linker and sysroot for
the target architecture. Three approaches were evaluated.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Tool
     - Pros
     - Cons
   * - ``cross`` crate
     - **CHOSEN.** Zero host setup — toolchain lives in a Docker image.
       ``cross build`` is a drop-in replacement for ``cargo build``.
       Consistent between developer laptops and CI runners.
     - Requires Docker on the host. Slightly slower first run (pulls image).
   * - ``cargo`` + host toolchain (``arm-linux-gnueabihf-gcc``)
     - No Docker needed for cross-compilation itself
     - Must install ``gcc-arm-linux-gnueabihf`` and ``libc6-dev-armhf-cross``
       on every developer machine and CI runner. Version drift risk.
   * - ``cargo-zigbuild``
     - Uses Zig as linker, no host toolchain needed
     - Less battle-tested for embedded Rust targets; adds Zig as a dependency

**Rationale**

``cross`` is the community standard for Rust cross-compilation and is
recommended in the official Rust cross-compilation guide. It wraps ``cargo``
transparently and handles the sysroot, linker, and environment variables
automatically. Developers already need Docker for the CI pipeline, so no
new tooling dependency is introduced.

**Target triple**: ``arm-unknown-linux-gnueabihf``
(ARMv7 hard-float ABI; compatible with Pi Zero W running Raspberry Pi OS).

**Usage**

.. code-block:: bash

   # Install once
   cargo install cross --git https://github.com/cross-rs/cross

   # Build any service
   cross build --target arm-unknown-linux-gnueabihf --release -p api-server

**Consequences**

- Developer setup docs must include ``cargo install cross``
- GitHub Actions component pipelines install ``cross`` at the start of each
  run (cached via ``actions/cache`` on ``~/.cargo/bin``)
- The ``cross`` Docker image for ``arm-unknown-linux-gnueabihf`` is pulled
  once and layer-cached by the runner

.. _dec-009:

DEC-009 — RAUC Bundle Signing Strategy
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_009
   :status: resolved
   :date: 2026-06-21
   :tags: ci, rauc, security, ota

**Decision**: **Throwaway dev certs** for CI/development builds.
**Hardware security key (YubiKey/HSM)** for production release signing.

.. note::

   DEC-023 replaces throwaway certs for tagged releases. Throwaway certs remain
   in use for builds on ``main`` and PRs.

**Context**

RAUC requires every bundle to be signed with a certificate chain. A
keyring (CA certificate) embedded in the device image verifies the
signature at install time. Two signing contexts require separate strategies:
CI builds (frequent, automated, untrusted outputs) and production releases
(infrequent, must be trusted by devices in the field).

This decision resolves the open issue KDS-8oi (Define release signing
strategy for RAUC bundles).

**CI / Development Signing — Throwaway Certs**

Every CI run generates a fresh throwaway keypair inside the ARM container
using ``openssl``. The certs exist only for the duration of the
``docker run`` invocation and are never written to disk on the host or
committed to the repository.

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - **A — Throwaway certs per-run** *(chosen)*
     - No secrets to manage; no keys ever stored anywhere; zero setup for
       new contributors; the CI keypair is worthless the moment the
       container exits
     - ~2–3 seconds added per run for RSA key generation; bundles from
       different runs cannot be verified against a shared keyring
   * - B — Committed dev keypair
     - Consistent keyring across runs; slightly faster
     - Private key in the repository (even if marked "dev only") sets a
       bad precedent; key rotation requires a new commit

**Rationale**: The CI bundle is a functional test artifact, not a
deliverable. No device in the field will ever install it. There is no
value in maintaining a persistent keypair for throwaway artifacts, and
the security properties of Option A are strictly better.

**Production Release Signing — Hardware Security Key**

Production bundles are signed using a hardware security key (YubiKey or
HSM) via RAUC's PKCS#11 interface. The private key is generated on the
hardware device and never exported.

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - A — GitHub Actions secret
     - Simple; works immediately; no extra hardware
     - Private key exists as a secret value that can be read by anyone
       with admin access to the repository; key rotation requires manual
       secret update
   * - **B — Hardware security key (YubiKey/HSM)** *(chosen)*
     - Private key never leaves the hardware device; strongest security
       posture; key cannot be exported or copied; signing requires physical
       possession of the device
     - Requires hardware purchase; signing step cannot run in automated CI
       (offline step); slightly more complex workflow
   * - C — Offline signing
     - Key stays off-network; flexible tooling
     - Key still exists as a file; discipline required to keep it secure

**Rationale**: The signing key is the root of trust for all OTA updates
delivered to devices in the field. A compromised signing key would allow
arbitrary firmware to be installed on every device. GitHub Actions secrets
are convenient but represent a key-as-value that can be read by privileged
users and is stored outside the operator's physical control. A hardware
key ensures the private key never exists outside the hardware boundary.

**Production Signing Workflow**

.. code-block:: text

   1. Release pipeline builds and tests the RAUC bundle (CI)
   2. CI uploads unsigned bundle as a GHA artifact
   3. Release manager downloads bundle to a local machine with the YubiKey
   4. Signs bundle:
        rauc bundle \
          --cert=release.cert.pem \
          --key="pkcs11:..." \
          /tmp/bundle-staging output/image-v1.0.0.raucb
   5. Distributes signed bundle via OTA update server

*Note*: RAUC supports PKCS#11 URLs directly in ``--key`` so the YubiKey
is referenced by its PKCS#11 slot, not as a file.

**Device Keyring**

The CA certificate corresponding to the YubiKey's signing certificate is
embedded in the device image at ``/etc/rauc/keyring.pem`` (baked into the
Dockerfile for production images). Devices will only install bundles whose
signature chain validates against this keyring.

For CI dev builds, the throwaway CA cert is used as the keyring — devices
in the field will never accept these bundles because their keyring contains
only the production CA certificate.

**Consequences**

- ``ci/create-bundle.sh`` generates throwaway certs on every run — no
  secrets required in CI for dev builds
- The production CA certificate must be provisioned into the release
  Dockerfile at build time (separate from the dev Dockerfile used in CI)
- A YubiKey (or equivalent HSM) must be procured before cutting the first
  production release
- The offline signing step is documented in the release runbook
- KDS-8oi (Define release signing strategy for RAUC bundles) is resolved
  by this decision

.. _dec-010:

DEC-010 — Full VM Boot Test (Solution 2)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_010
   :status: resolved
   :date: 2026-06-21
   :tags: ci, testing, qemu, rauc

**Decision**: Use ``qemu-system-arm`` with the **official pre-built RPi
kernel** (downloaded from ``raspberrypi/firmware``) to boot the Pi OS
rootfs. Test passes when ``rauc status`` returns valid slot information
after all four platform services reach ``active`` state. Runs **nightly on
main** and is **required on release tags**.

**Context**

Solution 1 (QEMU user-mode + Docker) validates that binaries install and
start, but does not validate the bootloader, kernel, systemd init sequence,
or RAUC slot-switching logic. Solution 2 adds a full VM boot test using
``qemu-system-arm`` to catch regressions in the init path and RAUC
integration before a release is cut.

**Kernel Source**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - **A — Official RPi kernel** *(chosen)*
     - Pre-built, zero build time, always matches the Raspberry Pi OS
       release being tested, maintained by the RPi Foundation
     - Tied to RPi Foundation's release cadence; no control over kernel
       config
   * - B — Custom kernel build
     - Full config control; can include/exclude modules
     - 20–40 min kernel build step; significant maintenance burden

**Rationale**: Custom kernel builds are overkill for boot validation of
userspace services. The official kernel is what will run on real devices.
Deviating from it in CI would reduce the value of the test.

**Source**: ``https://github.com/raspberrypi/firmware`` — the
``boot/kernel.img`` (32-bit ARMv6) and ``boot/*.dtb`` files are downloaded
at test time and cached between CI runs.

**Test Scope**

.. list-table::
   :header-rows: 1
   :widths: 10 20 70

   * - Level
     - Check
     - Rationale
   * - A
     - Kernel reaches userspace
     - Proves kernel + dtb + rootfs combination is bootable
   * - B
     - ``multi-user.target`` reached
     - Proves systemd init sequence completes
   * - C
     - All 4 services ``active``
     - Proves service binaries start and stay running under systemd
   * - **D**
     - **``rauc status`` returns slot info** *(chosen)*
     - Proves RAUC can read the slot layout from ``system.conf`` and
       communicate with the bootloader backend — the core OTA integration
       requirement

All four levels (A → D) are validated in sequence. D implies A, B, and C.

**Trigger**

.. list-table::
   :header-rows: 1
   :widths: 20 80

   * - Trigger
     - Rationale
   * - **Nightly on main** *(chosen)*
     - Catches regressions overnight without blocking PRs or extending
       the fast feedback loop. VM boot tests take 5–15 min — too slow for
       every PR.
   * - **Required on release tags** *(chosen)*
     - Guarantees every release candidate has passed the full boot test
       before the ``.raucb`` bundle is signed and distributed.

**QEMU Setup**

The test uses the ``raspi0`` machine type (BCM2835, ARM1176), which most
accurately represents the Pi Zero W hardware:

.. code-block:: bash

   # Download kernel and dtb (cached)
   wget https://github.com/raspberrypi/firmware/raw/master/boot/kernel.img
   wget https://github.com/raspberrypi/firmware/raw/master/boot/bcm2708-rpi-zero.dtb

   # Export rootfs from Docker image
   docker export $(docker create --platform linux/arm/v7 \
     kids-desktop-speaker:latest) | tar -C rootfs/ -xf -

   # Create ext4 rootfs image
   dd if=/dev/zero of=rootfs.img bs=1M count=1024
   mkfs.ext4 rootfs.img
   mount rootfs.img /mnt && cp -a rootfs/. /mnt && umount /mnt

   # Boot in QEMU
   qemu-system-arm \
     -M raspi0 \
     -kernel kernel.img \
     -dtb bcm2708-rpi-zero.dtb \
     -drive file=rootfs.img,format=raw \
     -append "root=/dev/mmcblk0p2 rw rootwait console=ttyAMA0" \
     -serial stdio \
     -no-reboot \
     -nographic

**Test Assertions** (run via QEMU serial console or guest agent):

.. code-block:: bash

   # Wait for multi-user.target
   systemctl is-active multi-user.target

   # Verify all platform services
   systemctl is-active api-server
   systemctl is-active audio-player
   systemctl is-active ota-updater
   systemctl is-active bluetooth-service

   # Verify RAUC slot integration (the key test)
   rauc status

**Consequences**

- ``qemu-system-arm`` must be installed on CI runners
  (``apt-get install qemu-system-arm`` on ``ubuntu-latest``)
- A nightly GitHub Actions workflow (``nightly-vm-test.yml``) is added
- The release pipeline (``release.yml``) gains a ``vm-boot-test`` job
  that runs after ``build-image`` and before bundle upload
- Kernel and dtb files are cached via ``actions/cache`` keyed on the
  firmware commit SHA to avoid re-downloading on every run
- The test does not validate hardware peripherals (no GPIO, I2S, Wi-Fi,
  Bluetooth) — hardware validation still requires physical Pi Zero W
- If the ``raspi0`` QEMU machine type proves incompatible with the official
  RPi kernel, fall back to ``versatilepb`` with a minimal kernel config
  (deferred investigation)

.. _dec-011:

DEC-011 — Release Pipeline Artifact Promotion Pattern
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_011
   :status: resolved
   :date: 2026-06-21
   :tags: ci, rauc, github-actions, release

**Decision**: Use a **two-phase artifact promotion pattern** in the release
pipeline: ``build-image`` produces a short-lived staging bundle
(``rauc-bundle-staging``); the bundle is promoted to a permanent release
artifact (``rauc-bundle-{tag}``) only after the ``vm-boot-test`` job passes.
The ``vm-boot-test`` job's **initial scope is integration baseline** — verify
all four platform services reach ``active`` state using the Docker rootfs
image; RAUC slot validation is a separate future concern.

**Context**

The release pipeline (``release.yml``) originally uploaded the ``.raucb``
bundle directly as ``rauc-bundle-{tag}`` from the ``build-image`` job, then
a downstream ``upload-bundle`` job attempted to upload under the same name —
causing a GHA artifact name collision. The reviewed design in
``ci-workflows.rst`` already specified the correct two-phase pattern but the
implementation diverged.

The design raises three related questions: how long the staging artifact
should be retained, whether to use a static or run-scoped name for the
staging slot, and what the ``vm-boot-test`` job actually needs to download
from the staging artifact.

**Promotion Pattern**

.. code-block:: text

   build-image job
     └── make bundle → ci/output/image.raucb
     └── upload: rauc-bundle-staging (7-day retention) ─────────────┐
                                                                     │
   vm-boot-test job  (uses Docker rootfs image, not the bundle)      │
     └── export rootfs from Docker image                             │
     └── boot in qemu-system-arm                                     │
     └── assert: all 4 services active (integration baseline)        │
         ─── PASS ──────────────────────────────────────────────────►│
                                                                     │
   upload-bundle job (gated on vm-boot-test)                         │
     └── download: rauc-bundle-staging ◄─────────────────────────────┘
     └── upload: rauc-bundle-{tag} (90-day retention)

Only bundles whose corresponding rootfs passes the integration baseline
test are promoted to a permanent release artifact.

**Staging Artifact — Name and Retention**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - ``rauc-bundle-staging`` (static) *(chosen)*
     - Simple. One slot per repository. No artifact accumulation.
     - Two simultaneous release builds (unlikely) could overwrite each other.
   * - ``rauc-bundle-staging-${{ github.run_id }}`` (unique)
     - No collision risk between concurrent runs.
     - Requires download step to know the ``run_id`` of the upstream job.
       Adds complexity for a scenario that will not occur in practice.

**Rationale**: Concurrent release tag pushes will not happen in normal
workflow. The simpler static name is preferred.

Staging retention is **7 days**. The artifact only needs to outlive a single
pipeline run (~15–30 min) but a 7-day window allows debugging failed
``vm-boot-test`` runs by inspecting the exact bundle that was produced.

Promoted release artifacts (``rauc-bundle-{tag}``) are retained for
**90 days** — long enough to cover the gap until the next release and to
support incident investigation.

**vm-boot-test Scope and Bundle Access**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - **A — Rootfs boot (integration baseline)** *(chosen for v1)*
     - Validates OS image, systemd init, and all four platform services
       with the same rootfs the Docker pipeline produces. Fast to implement.
     - Does not exercise the RAUC install path or A/B slot switching.
   * - B — RAUC install via bundle
     - Tests the full OTA path end-to-end: install bundle, reboot, verify
       new slot active.
     - Requires a base Pi OS image to install into. Significantly more
       complex; may warrant a dedicated design decision and separate workflow.

**Rationale**: The primary value of the vm-boot-test in v1 is an
**integration baseline**: confirm all artifacts produced by the pipeline
work together when booted. RAUC slot validation (Option B) adds value but
the right approach for that concern (dedicated integration environment,
hardware-in-the-loop, or a separate QEMU workflow) is an open question
that deserves its own decision when the time comes.

The ``vm-boot-test`` job therefore uses the **Docker image** exported as a
rootfs (``docker export``) — not the ``.raucb`` bundle. The bundle is what
is being promoted; it should not be consumed by the test that gates its
promotion.

*Note*: DEC-010 specified ``rauc status`` as the terminal test assertion.
This decision refines that scope: the initial implementation targets
integration baseline (all 4 services ``active``). RAUC slot queryability
is a subsequent enhancement, tracked separately.

**Initial Test Assertions** (v1 integration baseline):

.. code-block:: bash

   # Boot via qemu-system-arm (see DEC-010 for QEMU setup)
   # Assert via serial console / guest agent:

   systemctl is-active multi-user.target
   systemctl is-active api-server
   systemctl is-active audio-player
   systemctl is-active ota-updater
   systemctl is-active bluetooth-service

**Consequences**

- ``release.yml`` is updated to:

  - rename the ``build-image`` bundle upload to ``rauc-bundle-staging``
    with ``retention-days: 7``
  - add ``uses: actions/download-artifact@v4`` in the ``upload-bundle``
    job to fetch ``rauc-bundle-staging``
  - retain the promoted artifact as ``rauc-bundle-${{ github.ref_name }}``
    with ``retention-days: 90``

- ``vm-boot-test`` does **not** download the ``rauc-bundle-staging``
  artifact — it uses the Docker image directly (``docker export``)

- RAUC slot validation (``rauc status`` assertion) is deferred to a future
  decision; the ``vm-boot-test`` placeholder in ``release.yml`` need only
  document this scope boundary

- When RAUC slot validation is later implemented, it may warrant its own
  workflow rather than extending ``vm-boot-test`` — the two concerns
  (integration baseline and OTA path validation) have different triggers
  and failure modes

Onboarding & Controls
---------------------

.. _dec-012:

DEC-012 — Wi-Fi Onboarding & First-Boot Flow
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_012
   :status: resolved
   :date: 2026-06-21
   :tags: onboarding, wifi, networking, ux

**Decision**: Use **AP mode + captive portal** as the primary Wi-Fi
provisioning mechanism. A **USB config file** serves as a dev/recovery
fallback. The device re-enters AP mode automatically after **5 consecutive
connection failures**. User feedback during onboarding uses **pre-recorded
audio cues** (replaced by Piper TTS when available per DEC-015). The AP
hotspot is **open** — no password required.

**Context**

A new device has no Wi-Fi credentials. Every platform feature (AirPlay,
Spotify Connect, OTA updates, web UI) requires network access. Since the
device has no physical buttons (DEC-014), onboarding cannot rely on a
hardware reset button. Since no companion app is being built, BLE
provisioning is ruled out. The solution must work with a phone browser only.

**Provisioning Mechanism**

.. list-table::
   :header-rows: 1
   :widths: 10 30 35 25

   * - Option
     - Approach
     - Pros
     - Cons
   * - **A — AP mode + captive portal** *(primary, chosen)*
     - Device creates a Wi-Fi hotspot. User connects their phone and is
       redirected to a setup page to enter home Wi-Fi credentials.
     - No app required. Works on any phone with a browser. Standard
       pattern users recognise from other IoT devices.
     - User must switch their phone's Wi-Fi network during setup. Setup
       page loses internet access on the phone while connected to the
       device hotspot.
   * - B — BLE provisioning
     - Companion app pushes credentials over BLE.
     - Seamless UX; phone stays on home Wi-Fi.
     - Requires building and maintaining a mobile app. Out of scope.
   * - **C — USB config file** *(dev/recovery tool)*
     - User writes ``wifi.txt`` (SSID + password) to a USB stick and
       plugs it in on boot.
     - No network needed at all. Useful for headless dev setup and
       factory reset recovery.
     - Requires USB stick and physical access. Not a consumer UX.

**First-Boot Flow**

.. code-block:: text

   Power on
     │
     ▼
   Check saved Wi-Fi credentials
     ├── None found ──────────────────────────────────────────┐
     └── Found → attempt connection                            │
             │                                                 │
             ├── Success → play "Ready" audio cue             │
             │             start all services                  │
             └── Failure (×5) ──────────────────────────────► │
                                                               ▼
                                                    Enter AP mode
                                                    Broadcast: KidsSpeaker-XXXX
                                                    Play audio cue:
                                                    "Connect to KidsSpeaker-XXXX
                                                     Wi-Fi to set up your speaker"
                                                               │
                                                    User connects phone to hotspot
                                                    Captive portal → setup page
                                                    User enters SSID + password
                                                               │
                                                    Device saves credentials
                                                    Play audio cue: "Connecting..."
                                                    Exit AP mode
                                                               │
                                                    Attempt connection
                                                    ├── Success → play "Ready"
                                                    └── Failure ×5 → back to AP mode

**Re-provisioning Trigger**

Since there are no physical buttons, re-provisioning is triggered
automatically: **5 consecutive connection timeouts** cause the device to
clear its saved credentials and re-enter AP mode. The user does not need
to do anything beyond moving the device to a new location — the device
self-heals into setup mode if it cannot reach its last-known network.

An explicit factory reset command is also available via the web UI
(accessible while on the current network) for deliberate re-provisioning.

**USB Config File Format** (dev/recovery)

.. code-block:: text

   # /boot/wifi.txt (on USB stick)
   SSID=MyHomeNetwork
   PASSWORD=mysecretpassword

The API Server checks for a USB-mounted ``wifi.txt`` on boot before
entering AP mode. If found, credentials are applied and the file is
deleted from the USB stick.

**Security**

The AP hotspot is open (no WPA2 password). The captive portal setup page
has no PIN or authentication. Physical proximity to the device is treated
as sufficient trust for onboarding. The window of exposure is short
(device is in AP mode only until credentials are entered or a timeout
occurs).

**Audio Cues**

Audio feedback during onboarding relies on pre-recorded audio clips
packaged with the firmware:

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Trigger
     - Audio cue
   * - Entering AP mode
     - "Connect to KidsSpeaker-[ID] Wi-Fi to set up your speaker"
   * - Credentials received, connecting
     - "Got it, connecting now..."
   * - Connected successfully
     - "Ready"
   * - Connection failed, retrying
     - "Having trouble connecting, trying again..."
   * - Back to AP mode after failures
     - "Could not connect. Connect to KidsSpeaker-[ID] to try again"

When Piper TTS is available (DEC-015), these clips are generated
dynamically at runtime instead of being pre-recorded.

**Consequences**

- A new ``onboarding`` module lives in the API Server (or a dedicated
  ``onboarding-service``) and owns: AP mode management (``hostapd`` +
  ``dnsmasq``), the captive portal web page, credential persistence,
  and the connection-failure counter
- The device hostname is ``kidsspeaker-XXXX`` (last 4 hex digits of MAC
  address) so multiple devices on the same network are distinguishable
- Pre-recorded audio clips (WAV/OGG) are shipped in the firmware image
  under ``/usr/share/kids-desktop-speaker/audio/onboarding/``
- The BT controller (DEC-014) must be paired *after* Wi-Fi onboarding
  completes — the web UI guides the user through BT pairing as step 2
  of the setup flow
- ``wpa_supplicant`` manages Wi-Fi credentials persistently in
  ``/var/lib/kids-desktop-speaker/wifi.conf`` on the data partition
  (survives OTA updates)
- The API Server exposes ``/api/network/reset`` (authenticated) to
  trigger deliberate re-provisioning from the web UI

.. _dec-013:

DEC-013 — Spotify Connect via librespot
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_013
   :status: resolved
   :date: 2026-06-21
   :tags: audio, spotify, librespot

**Decision**: Use **librespot** (Rust) as the Spotify Connect receiver,
managed as a subprocess of the Audio Player service — the same pattern
used for shairport-sync (AirPlay, DEC-004).

**Context**

Spotify Connect is a hard requirement. The platform must appear as a
Spotify Connect target device on the local network, accepting playback
from the Spotify app on any phone, tablet, or computer on the same Wi-Fi
network.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - **librespot** *(chosen)*
     - Open-source Rust implementation. Active project (10k+ stars).
       Pure Rust — no C dependencies. Outputs to ALSA, PipeWire, or
       stdout (pipe). Well-documented. Used in production by Spotifyd
       and other projects.
     - Reverse-engineered (no official Spotify support). Spotify may
       break compatibility in future protocol updates. Requires Spotify
       Premium on the user's account.
   * - spotifyd
     - Thin daemon wrapper around librespot. Slightly easier to run
       as a standalone service.
     - Adds a layer with its own config format and lifecycle management.
       The Audio Player already manages subprocess lifetimes — wrapping
       librespot directly is simpler.
   * - Official Spotify Connect SDK
     - First-party support, guaranteed compatibility.
     - Closed-source, C library, requires a commercial licence and
       hardware certification programme. Not viable for this project.

**Rationale**

librespot is the de facto standard for open-source Spotify Connect
receivers. It is actively maintained, written in Rust (consistent with
the rest of the stack), and has a well-understood integration path via
ALSA. The Audio Player already manages shairport-sync as a child
process; adding librespot follows the same pattern with no new
architectural concepts.

**Integration**

The Audio Player starts and stops librespot as a child process, routing
its ALSA output through the loopback mixer (same channel as
shairport-sync). Only one audio source plays at a time; the Audio Player
arbitrates source priority.

.. code-block:: bash

   librespot \
     --name "KidsSpeaker" \
     --bitrate 320 \
     --backend alsa \
     --device kids-speaker-loopback \
     --disable-audio-cache

**Consequences**

- ``librespot`` is added as a system package (or compiled from source
  for ARMv6) and installed in the firmware image
- Spotify Premium is required on the user's account for Spotify Connect
- The Audio Player's subprocess manager handles librespot start/stop
  and restart-on-crash
- Device name ("KidsSpeaker") is configurable via the web UI and
  persisted on the data partition
- librespot protocol compatibility is a dependency risk; the project
  tracks the librespot changelog and pins to a tested version
- No offline Spotify playback — Spotify Connect requires an active
  internet connection and a Spotify client (phone/tablet) on the network

.. _dec-014:

DEC-014 — Physical Controls: External Bluetooth Controller
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_014
   :status: resolved
   :date: 2026-06-21
   :tags: bluetooth, controls, ux, hardware

**Decision**: The device has **no physical buttons or knobs**. All
playback control (volume up/down, skip, pause/play, source switching) is
handled by an **external Bluetooth AVRCP controller** paired to the
device.

**Context**

A speaker needs a way for the user to control playback without picking
up a phone. The alternatives are physical controls (buttons, rotary
encoder) mounted on the enclosure, or an external wireless controller.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 20 40 40

   * - Option
     - Pros
     - Cons
   * - Physical buttons on enclosure
     - No pairing required. Always available even if BT fails.
     - GPIO wiring complexity. Buttons must be waterproofed and
       mounted. Adds mechanical design scope to the enclosure.
       Button debounce and interrupt handling in firmware.
   * - **External BT AVRCP controller** *(chosen)*
     - Zero GPIO wiring. Enclosure is simpler (no cutouts for buttons).
       Leverages the BT stack already required for the platform.
       Controller is replaceable/upgradeable without hardware changes.
       Works from a distance.
     - Controller must be paired during setup. Controller can run out
       of battery or be lost. No fallback if controller is unavailable.

**Rationale**

The Bluetooth Service is already a core component. AVRCP playback
control is already planned (DEC-006). Delegating all physical control to
an external BT device removes GPIO complexity from both the firmware and
the enclosure design. The target user (children) is familiar with
game-style controllers and remotes.

**Bluetooth Service Requirements** (added by this decision)

- Maintain a **persistent AVRCP connection** to the paired controller
- **Auto-reconnect** when the controller wakes from sleep or comes back
  into range (without user intervention)
- Handle the following AVRCP commands: play, pause, stop, next track,
  previous track, volume up, volume down
- Source switching (AirPlay → Spotify → Radio) via a long-press or
  button combination defined during setup
- Pair up to **two controllers** (primary + backup)

**Pairing Flow**

Controller pairing is step 2 of the onboarding flow (after Wi-Fi is
configured). The web UI presents a pairing screen; the user puts their
controller into pairing mode and the Bluetooth Service completes the
bond. Pairing data is stored on the data partition and survives OTA
updates.

**Consequences**

- No GPIO pins are used for user controls in v1
- The enclosure has no button cutouts — simpler mechanical design
- The web UI / API Server must expose a "pair controller" endpoint
  used during onboarding
- If the controller is lost or discharged, the web UI (phone browser)
  is the fallback control surface
- The Bluetooth Service reconnection logic must be robust —
  controllers sleep aggressively to save battery
- Controller button mapping (e.g. which button triggers source switch)
  is configurable and stored on the data partition

.. _dec-015:

DEC-015 — Piper TTS: Deferred, Resource-Conditional
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_015
   :status: resolved
   :date: 2026-06-21
   :tags: audio, tts, piper, resource-management

**Decision**: **Defer Piper TTS** to a later phase. Implementation
proceeds only after all other platform services are stable and a
profiling run confirms sufficient CPU and RAM headroom on the Pi Zero W.
If the headroom is insufficient, a lighter fallback is chosen or TTS is
dropped entirely.

**Context**

Voice feedback enhances UX (status announcements, error messages,
onboarding prompts). Piper TTS is a high-quality, offline neural TTS
engine with a Rust-friendly interface. However, the Pi Zero W is severely
resource-constrained: 512 MB RAM, single-core ARM1176 at 1 GHz. Running
Piper inference concurrently with shairport-sync, librespot, MPD,
CamillaDSP, and four Rust services risks exhausting available memory or
causing audio dropouts.

In the short term, pre-recorded audio clips cover onboarding prompts
(DEC-012). TTS adds value for dynamic messages (track announcements,
error details) but is not on the critical path for v1.

**Gate Condition**

Piper TTS implementation is gated on a profiling milestone:

.. code-block:: text

   All 6 platform services stable + profiling run shows:
     - Idle RAM available ≥ 80 MB  (Piper model ~50–70 MB)
     - CPU headroom ≥ 30% at idle  (Piper inference spike ~60% for ~1s)

If either threshold is not met, proceed to fallback evaluation.

**Fallback Options**

.. list-table::
   :header-rows: 1
   :widths: 20 20 30 30

   * - Option
     - Quality
     - RAM footprint
     - Notes
   * - **Piper TTS** *(target)*
     - Natural, neural
     - ~50–70 MB (model)
     - Requires profiling gate to pass
   * - espeak-ng
     - Robotic but intelligible
     - < 5 MB
     - C library, available in Debian, very fast on ARM
   * - festival
     - Better than espeak-ng
     - ~20 MB
     - C daemon, Debian package available
   * - No TTS (pre-recorded clips only)
     - N/A
     - 0 MB runtime
     - Acceptable if messages are limited and predictable

**Rationale**

Shipping a working speaker with pre-recorded audio clips is better than
delaying the project while tuning Piper on constrained hardware. The
decision is explicitly revisited after profiling — it is not permanently
deferred.

**Consequences**

- Onboarding audio (DEC-012) ships with pre-recorded WAV/OGG clips
  packaged in the firmware; the API to play them is the same one that
  will later invoke Piper
- The Audio Player exposes a ``/tts`` internal endpoint that initially
  plays a pre-recorded clip by name, later routes through Piper (or
  fallback engine) — callers are decoupled from the TTS implementation
- A profiling milestone is added to the project backlog: measure idle
  and loaded RAM/CPU after all services are integrated
- ``piper-tts-rs`` (KDS-m6q) and ``piper-tts`` performance research
  (KDS-s3s) remain open until the profiling gate is evaluated
- The KDS-s3s beads issue ("Research: Piper TTS performance on Pi Zero
  W") is the blocker for this decision's gate condition

Bluetooth
---------

.. _dec-006:

DEC-006 — A2DP Audio Input
~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_006
   :status: resolved
   :date: 2026-05-25
   :tags: bluetooth, a2dp

**Decision**: **AVRCP only for v1.** A2DP audio input is deferred.

**Context**

The Bluetooth Service can support both AVRCP (remote control) and A2DP
(audio input from a phone). The Pi Zero W's BCM43430 chip supports both
profiles simultaneously.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 15 25 35 25

   * - Option
     - Approach
     - Pros
     - Cons
   * - A
     - AVRCP only
     - **CHOSEN.** Simpler, lower CPU/memory overhead
     - No BT audio from phones
   * - B
     - AVRCP + A2DP
     - Full BT speaker capability
     - PulseAudio/PipeWire dependency, CPU overhead, source switching complexity

**Rationale**

AirPlay and Spotify Connect cover the primary streaming use cases. A2DP
adds significant complexity and CPU load on the already-constrained Pi Zero
W. The loopback channel (ch4-5) is reserved for future A2DP support.

**Consequences**

- No A2DP sink profile is registered in v1
- The ``loopback_bt`` ALSA channel is unused in v1
- A2DP can be added later as a feature toggle without architecture changes
- No PulseAudio/PipeWire dependency in v1

Audio Playback
--------------

.. _dec-016:

DEC-016 — Audio Source Arbitration & librespot Autostart
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_016
   :status: resolved
   :date: 2026-06-21
   :tags: audio, spotify, airplay, arbitration

**Decision**: **Last-started source wins.** When a new audio source
becomes active, the Audio Player pauses all other sources. librespot
(Spotify Connect) starts automatically when the network is up and
advertises via Zeroconf — no stored Spotify credentials are required on
the device.

**Context**

Three audio sources can become active simultaneously: AirPlay
(shairport-sync), Spotify Connect (librespot), and Internet Radio /
Podcasts (MPD). A policy is needed for what happens when two are active
at the same time. Additionally, librespot must be discoverable on the
network as soon as Wi-Fi is available.

**Source Arbitration Policy**

Last-started wins. The Audio Player monitors all source subprocess
states. When a new source signals playback start, the Audio Player sends
pause/stop to all other active sources before unmuting the new one.

.. code-block:: text

   User plays Spotify Connect → librespot signals play
     Audio Player pauses shairport-sync (if active)
     Audio Player pauses MPD (if active)
     Audio Player unmutes librespot output channel

   User casts AirPlay → shairport-sync signals play
     Audio Player pauses librespot (if active)
     Audio Player pauses MPD (if active)
     Audio Player unmutes shairport-sync output channel

This is the simplest policy with the most intuitive UX: whatever the
user most recently chose to play is what plays.

**librespot Autostart & Discovery**

librespot runs in **Zeroconf discovery mode** (``--zeroconf-port``). No
Spotify credentials are stored on the device. Authentication is handled
entirely by the Spotify client app — the user opens Spotify on their
phone, sees "KidsSpeaker" as a Connect target, and taps it. The
Spotify app negotiates the session directly.

librespot is started by the Audio Player as soon as a working network
interface is detected (not just at boot), so the device is immediately
visible in Spotify after Wi-Fi onboarding completes.

The device name displayed in Spotify ("KidsSpeaker" by default) is
configurable via the web UI and stored on the data partition.

**Consequences**

- The Audio Player subscribes to network-up events (via
  ``systemd-networkd`` D-Bus signals or polling ``/sys/class/net``)
  and starts librespot only after a routable interface is available
- librespot restart-on-crash is handled by the Audio Player subprocess
  manager (same as shairport-sync)
- Each audio source subprocess exposes a play/pause signal that the
  Audio Player listens to (via stdout, D-Bus, or a Unix socket — TBD
  per service)
- The ALSA loopback mixer has one channel per source; the Audio Player
  controls per-channel mute to implement arbitration without stopping
  source processes unnecessarily
- Source state is exposed on ``/api/audio/source`` (current active
  source + status of each) for the web UI

.. _dec-020:

DEC-020 — Volume Limits
~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_020
   :status: resolved
   :date: 2026-06-21
   :tags: audio, volume, safety, parental-controls

**Decision**: Enforce a **configurable maximum volume cap** applied at
the CamillaDSP / ALSA mixer level. Default maximum is **75% of hardware
output range**. Parents can raise or lower the cap via the web UI.

**Context**

This is a kids speaker. Prolonged exposure to audio above 85 dB can
cause hearing damage in children. A software volume cap provides a
reasonable safeguard, acknowledging that the physical output level also
depends on the amplifier gain and speaker sensitivity.

**Implementation**

The volume cap is enforced in two places:

1. **CamillaDSP pipeline**: a gain stage limits the output level to the
   configured maximum. CamillaDSP applies this before the ALSA output
   stage so it catches all sources routed through the loopback mixer.
2. **ALSA master volume**: the ALSA master is additionally capped at the
   same percentage as a belt-and-suspenders measure.

The cap is stored in ``/var/lib/kids-desktop-speaker/config.json`` on
the data partition and applied at Audio Player startup and on any
config change.

**Consequences**

- Default cap: 75% — audible and clear without being dangerously loud
- Cap is adjustable 0–100% via ``/api/audio/volume-limit`` (web UI)
- AVRCP volume-up commands from the BT controller respect the cap and
  cannot exceed it
- The web UI displays the current cap and current volume with a clear
  visual indicator when the cap is active

.. _dec-021:

DEC-021 — Alarm & Sleep Timer
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_021
   :status: resolved
   :date: 2026-06-21
   :tags: audio, alarm, timer, ux

**Decision**: Include **alarm** (play audio at a scheduled time) and
**sleep timer** (stop playback after N minutes) as v1 features, managed
by the API Server and triggered via the web UI.

**Context**

Alarms and sleep timers are table-stakes features for a bedside children's
speaker. A sleep timer helps parents ensure the device stops playing at
bedtime. An alarm wakes children in the morning with a sound or a radio
station rather than a harsh buzzer.

**Design**

*Sleep timer*: The user sets a countdown (e.g. 30 min) via the web UI.
The API Server schedules a stop command to the Audio Player at expiry.
The BT controller can also trigger a pre-configured timer duration via a
button combination.

*Alarm*: The user configures one or more alarms (time + days of week +
audio source: clip, MPD station, or TTS phrase) via the web UI. The API
Server manages a scheduler (in-process, no external daemon needed) that
fires the alarm by commanding the Audio Player to play the configured
source. Alarms require NTP (DEC-019) to ensure correct device time.

**Consequences**

- Alarms and timers are stored on the data partition and survive OTA
  updates and reboots
- The API Server includes a lightweight in-process scheduler (e.g.
  ``tokio`` task with a sleep loop or a cron-style library)
- NTP (DEC-019) is a hard dependency for alarms — the web UI warns if
  time is not synced
- The web UI exposes alarm management: create / edit / delete alarms,
  set sleep timer, cancel active timer
- ``/api/timer`` and ``/api/alarms`` endpoints on the API Server

Build Pipeline
--------------

.. _dec-022:

DEC-022 — Cross-Compilation Pipeline for ARM Services
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_022
   :status: resolved
   :date: 2026-06-21
   :tags: build, cross-compilation, arm, rust, ci

**Decision**: Use the **``cross`` tool** to cross-compile all four Rust
services (``api-server``, ``audio-player``, ``ota-updater``,
``bluetooth-service``) for the Raspberry Pi Zero W target
(``armv7-unknown-linux-gnueabihf``). All crates must use **rustls** instead
of OpenSSL for TLS — OpenSSL cannot be reliably cross-compiled.

**Context**

The Pi Zero W runs ARMv6 with hard-float ABI.

.. warning::

   The original text here claimed ARMv7 binaries run on ARMv6 hardware through
   a compatibility layer. That is false: ARMv7 code crashes with an illegal
   instruction. DEC-024 replaces the target and base image.

Binaries must be built on x86_64 developer and CI machines
and injected into the Docker image used to populate the QEMU VM rootfs.

**Options Considered**

1. **``cross`` (chosen)** — Docker-based cross-compilation driver.
   Provides a correct ARM sysroot inside a container with no local
   toolchain required. Single command:
   ``cross build --release --target armv7-unknown-linux-gnueabihf``.

2. **``cargo`` + local ARM toolchain** (``arm-linux-gnueabihf-gcc``) —
   requires installing and maintaining a cross-compiler on every developer
   machine and CI runner. Fragile across distro versions.

3. **Build inside ARM Docker container**
   (``docker run --platform linux/arm/v7 rust:...``) — architecturally
   correct but ~10× slower than native cross-compile via QEMU user-mode
   emulation of the Rust compiler itself.

4. **GitHub Actions native ARM runners** — not available on standard
   hosted runners; requires a physical Pi or a paid cloud ARM instance.

**TLS constraint**

``reqwest`` (used by ``ota-updater`` for bundle downloads) defaults to
``native-tls`` (OpenSSL), which requires a C cross-toolchain and OpenSSL
headers for the target. To avoid this, ``ota-updater`` disables default
features and opts into ``rustls-tls``:

.. code-block:: toml

   reqwest = { version = "0.12", default-features = false, features = ["stream", "rustls-tls"] }

All future crates that need TLS must follow the same pattern.

**Consequences**

- ``cross`` and Docker must be present on developer machines and CI runners
- The Dockerfile ``COPY`` path
  ``target/armv7-unknown-linux-gnueabihf/release/<binary>`` must be
  populated before ``docker buildx build`` runs; the Makefile
  ``vm-image`` target depends on a ``cross-build`` prerequisite
- OpenSSL is banned as a transitive dependency in any crate — use rustls
- The ``armv7-unknown-linux-gnueabihf`` target must be installed:
  ``rustup target add armv7-unknown-linux-gnueabihf``

System
------

.. _dec-017:

DEC-017 — zRAM Memory Expansion
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_017
   :status: resolved
   :date: 2026-06-21
   :tags: system, memory, performance, zram

**Decision**: Enable **zRAM** with a compressed swap device sized at
**50% of physical RAM (256 MB)** to extend effective memory headroom
on the Pi Zero W.

**Context**

The Pi Zero W has 512 MB physical RAM. Running shairport-sync,
librespot, MPD, CamillaDSP, four Rust services, systemd, and avahi
simultaneously leaves limited headroom, particularly for Piper TTS
(DEC-015). zRAM creates a compressed in-memory swap device — no SD
card I/O, no wear, and a typical 2:1–3:1 compression ratio effectively
adds 128–192 MB of usable memory at the cost of CPU for compression.
Librespot is confirmed working on Pi Zero W (tested); zRAM further
reduces pressure during peak usage.

**Configuration**

.. code-block:: bash

   # /etc/systemd/system/zram.service (or via zram-tools package)
   zramctl /dev/zram0 --algorithm lz4 --size 256M
   mkswap /dev/zram0
   swapon /dev/zram0 --priority 100

``lz4`` is chosen over ``zstd`` for lower CPU overhead on the
single-core ARM1176. Priority 100 ensures zRAM is used before any
SD card swap (which is not configured).

**Consequences**

- zRAM is configured as a systemd service that runs at boot, before
  any platform services start
- No SD card swap is configured — zRAM is the only swap device
- The Dockerfile and firmware image include ``zram-tools`` or an
  equivalent systemd unit
- At peak load, up to ~50% of physical RAM can be swapped compressed
  into zRAM, effectively extending the memory envelope for Piper TTS
  evaluation (DEC-015)
- CPU overhead is low with lz4 on typical audio service memory patterns
  (mostly heap allocations, compresses well)

.. _dec-018:

DEC-018 — mDNS via avahi-daemon
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_018
   :status: resolved
   :date: 2026-06-21
   :tags: system, networking, mdns, discovery

**Decision**: Run **avahi-daemon** as a system service to handle mDNS
(``_tcp.local``) advertisement for the web UI and device hostname.
shairport-sync and librespot handle their own mDNS/Zeroconf internally.

**Context**

Several platform features require mDNS for discoverability:

- **Web UI**: the device must be reachable at ``kidsspeaker-XXXX.local``
  without knowing its IP address
- **AirPlay**: shairport-sync advertises ``_raop._tcp`` itself via its
  own mDNS stack — avahi is not required but can be used
- **Spotify Connect**: librespot handles Zeroconf internally
- **Device hostname**: avahi resolves ``kidsspeaker-XXXX.local`` on the
  LAN so users can type it in a browser

**Consequences**

- ``avahi-daemon`` is added to the Dockerfile and enabled as a systemd
  service
- The device hostname is set to ``kidsspeaker-XXXX`` (last 4 hex digits
  of Wi-Fi MAC) at first boot by the onboarding module
- ``/etc/avahi/services/http.service`` advertises the web UI on port 80
- avahi's D-Bus interface is available for other services if needed
- ``avahi-daemon`` RAM footprint is ~2–3 MB — negligible

.. _dec-019:

DEC-019 — NTP Time Synchronisation
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_019
   :status: resolved
   :date: 2026-06-21
   :tags: system, networking, ntp, time

**Decision**: Use **systemd-timesyncd** for NTP time synchronisation.
No additional NTP daemon is needed — timesyncd ships with systemd and is
already present in the firmware image.

**Context**

Correct system time is required for: alarms (DEC-021), OTA update
scheduling, log timestamps, and TLS certificate validation (used by
librespot for Spotify's HTTPS API). The Pi Zero W has no hardware RTC,
so time must be acquired from the network on every boot.

**Configuration**

.. code-block:: ini

   # /etc/systemd/timesyncd.conf
   [Time]
   NTP=pool.ntp.org
   FallbackNTP=time.cloudflare.com

systemd-timesyncd saves the last-known time to
``/var/lib/systemd/timesync/clock`` (on the data partition) and applies
it immediately on next boot as a monotonic best-guess before NTP sync
completes. This prevents large time jumps during the NTP sync window.

**Consequences**

- No additional packages needed — timesyncd is part of systemd
- The data partition mount must be available before timesyncd starts
  (``RequiresMountsFor=/var/lib/systemd/timesync`` in the unit)
- The API Server exposes ``/api/system/time`` showing current time and
  NTP sync status; the web UI displays a warning if time is not synced
- Alarm scheduling (DEC-021) blocks on NTP sync for the first alarm
  after boot; subsequent alarms use the saved clock as fallback

DEC-023 — Stable Development CA for Published Bundles
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_023
   :status: resolved
   :date: 2026-11-01
   :tags: ci, rauc, security, ota

**Decision**: Tagged releases are signed with a **stable development identity**.
Its CA certificate is committed as ``ci/rauc/keyring.pem`` and installed in the
image. The signing key and certificate are held in the GitHub secrets
``RAUC_SIGNING_KEY`` and ``RAUC_SIGNING_CERT``. Partly supersedes DEC-009.

**Context**

DEC-009 generates a new certificate on every CI run, so no device can verify a
published bundle and ``rauc install`` cannot succeed. The OTA loop needs a trust
anchor that stays the same across releases.

**Consequences**

- ``ci/scripts/gen-signing-ca.sh`` creates the identity and destroys the CA
  private key, so rotation means generating a new CA.
- Release builds fail rather than fall back to throwaway keys, and verify each
  bundle against the keyring.
- Builds on ``main`` and PRs cannot read the secrets and keep throwaway certs.
  No device trusts those bundles.
- Anyone able to read the repository secrets could sign an update trusted by
  devices using this CA. Production signing still moves to a hardware key
  (DEC-009), and devices shipped with the development CA need a planned
  rotation first.

DEC-024 — ARMv6 Raspbian Base and Emulated CPU Check
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_024
   :status: resolved
   :date: 2026-11-01
   :tags: build, arm, ci, hardware

**Decision**: Build the image from a **Raspbian bookworm rootfs** created with
``debootstrap`` from ``archive.raspbian.org``, compile every service for
``arm-unknown-linux-gnueabihf``, and run the image build and tests under QEMU
pinned to **ARM1176**, the Pi Zero W's CPU. Supersedes the ARMv7 target in
DEC-022.

**Context**

The image was based on ``arm32v7/debian:bookworm-slim``. Debian's ``armhf``
port requires ARMv7, so its ``/bin/true`` crashes with an illegal instruction on
ARM1176. Emulating ``linux/arm/v7`` and a Cortex-A15 VM could not detect this.

**Options Considered**

.. list-table::
   :header-rows: 1
   :widths: 30 70

   * - Option
     - Assessment
   * - **Raspbian via debootstrap** *(chosen)*
     - Hard-float ARMv6 with the same packages, built from the project's own
       archive with a pinned signing key
   * - ``balenalib/rpi-raspbian``
     - Ready-made, but third-party and last built in 2023
   * - Debian ``armel``
     - Runs on ARMv6, but soft-float is a poor fit for DSP on this CPU
   * - Pi Zero 2 W
     - Removes the problem, but changes the target hardware

**Consequences**

- ``make test-armv6`` runs the image's programs on ARM1176, with a control
  proving that ARMv7 code crashes.
- ``ota-updater`` carries an ARMv7/NEON ELF tag from ``ring``. It completes a
  real HTTPS request on ARM1176, which fits ``ring`` selecting code at runtime.
- The full-system VM still uses an ARMv7 kernel on a Cortex-A15. It tests boot
  behaviour, not the CPU.
- ``make hw-test`` runs the same checks on a real Pi Zero W. QEMU has already
  hidden one bug: launching a missing program looked successful under emulation
  but fails on the device.
- Emulation is not hardware. A device test is still required, and the Raspberry
  Pi kernel and bootloader are still outstanding.
- ``librespot`` and ``camilladsp`` are not packaged by Raspbian and must be built
  for ARMv6.

DEC-025 — Pi Zero W Boot Chain and Device Settings
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_025
   :status: resolved
   :date: 2026-11-01
   :tags: boot, ota, rauc, hardware, security

**Decision**: Boot with Raspberry Pi firmware, then U-Boot running an A/B script,
then the kernel stored **inside the root slot**. The SD card has four partitions:
boot (FAT), root A, root B and data.

**Context**

RAUC needs a bootloader that can switch slots and count failed attempts. Keeping
the kernel in the root slot means a kernel update is atomic and rolls back with the
rest of the system.

**Details**

- The image ships no ``uboot.env``. A valid file replaces all of U-Boot's built-in
  defaults, including ``bootcmd``, leaving it waiting at a prompt. The boot script
  creates the file on first boot.
- ``ci/scripts/clean-rootfs.py`` removes what ``docker export`` leaves behind.
  ``/.dockerenv`` makes systemd skip services meant only for real systems, such as
  clock sync, and Docker blanks ``/etc/resolv.conf``.
- Each device creates its own machine ID on first boot, and a ``disable *``
  preset stops that first boot from switching services on. DHCP identifies the
  device by MAC address, so its IP address survives a slot switch.
- A failed health check reboots, and the kernel reboots after a panic, so a bad
  update always uses up its boot tries.
- The CI VM checks over SSH that the system reaches ``running`` with every service
  active. Its disk is built with ``mke2fs -d`` in a container so files keep their
  real owners, and ``rauc-mark-good`` only runs when booted into a RAUC slot.
- Bluetooth starts when its adapter appears, via a udev rule, because on the Pi
  the adapter shows up after ``bluetooth.service`` has already been skipped.
- Open problem: a kernel that hangs before Linux arms the watchdog leaves the
  device stuck until power-cycled, as seen on real hardware. Arming the watchdog
  from the boot script made healthy boots fail on the Pi as well, so it was
  removed. KDS-fjl tracks the follow-up.
- U-Boot is 2024.01 from Raspbian trixie, pinned by checksum, because that is the
  version tested on a Pi Zero W.
- ``ci/boot/boot.cmd`` gives each slot three boot tries. ``rauc-mark-good`` restores
  them only after the API answers ``/health``.
- The hardware watchdog resets a hung boot, so rollback needs no manual power cycle.
- Firmware, U-Boot and the device tree sit on the boot partition, which RAUC does
  not update. Changing them requires reflashing.
- Wi-Fi details and SSH keys are files on the boot partition, added after flashing,
  so they never enter the image or repository.

**Security**

- The root account is locked, replacing the previous known password.
- SSH accepts keys only. Without a key on the boot partition, nobody can log in.
- Host keys are created per device and kept on the data partition.
- The serial console in the VM still logs in automatically. This must be reviewed
  before shipping devices.

DEC-026 — Release License Information: SBOM, Inventory and Source
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_026
   :status: resolved
   :date: 2026-11-06
   :tags: licensing, ci, release

**Decision**: Every release publishes, next to its bundle:

- ``sbom.spdx.json``: an SPDX software bill of materials from Syft, listing each
  Debian package in the image and each Rust crate, with versions and licenses.
- ``licenses.tar.gz``: the license inventory, with each package's source version
  and copyright file.
- ``sources.tar``: the Debian source packages for the GPL and LGPL software in the
  image, at the exact versions shipped.

``make -C ci legal-info`` produces all three, using existing tooling: Syft for the
SBOM, and the archives' own source packages for the source.

**Context**

The project's own code is Apache-2.0 and public. The image also redistributes
third-party GPL and LGPL binaries from the Raspbian and Raspberry Pi archives.
Those archives are rolling: versions shipped in an image disappeared from them
within days. The source is therefore collected in the same build that produces
the binaries, and the release fails if any of it can't be fetched.

**Open questions**

- Devices sold with the image preinstalled may also need a written offer in their
  documentation.
- GPLv3's installation-information requirement for consumer products, which a
  reflashable SD card probably satisfies.

These should be reviewed by a lawyer before devices are sold.

DEC-027 — IPC: Newline-Delimited JSON over Unix Sockets
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_027
   :status: resolved
   :date: 2026-11-06
   :tags: ipc, architecture

**Decision**: Services exchange newline-delimited JSON over Unix sockets, one
request and one response per connection, as specified in the IPC protocol page.
This replaces the length-prefixed framing first proposed in KDS-31u.

**Context**

Three services already used newline-delimited JSON. JSON escapes newlines inside
strings, so a newline reliably ends a message, and a length prefix adds nothing
for small messages. The text form can be tested by hand with ``socat`` on a
device. Socket names follow the deployed services, not the earlier design.

**Consequences**

- A shared ``kds-ipc`` crate implements framing, error responses, size limits,
  timeouts and socket setup, so services can't drift apart.
- Pushed events will need a subscription command with a long-lived connection.

DEC-028 — Portable A/B Boot: Own State File and Partition UUIDs
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_028
   :status: resolved
   :date: 2026-11-06
   :tags: boot, ota, rauc, ci

**Decision**: The boot script keeps the A/B state (``BOOT_ORDER``,
``BOOT_A_LEFT``, ``BOOT_B_LEFT``) in its own file, ``kds-ab.env`` on the boot
partition, which ``fw_setenv`` and RAUC also use. Partitions are found by
partition UUID, from a fixed disk ID (``4b445300``), in the boot script, ``fstab``
and RAUC's slot configuration. Refines DEC-025.

**Context**

Automating the A/B test in CI (KDS-in5) needs the same boot script to run in QEMU.
U-Boot's QEMU build keeps its environment in emulated flash, which Linux can't
easily write, while the Pi's U-Boot keeps it in ``uboot.env`` on the boot
partition. Device names also differ: ``/dev/mmcblk0`` on the Pi, ``/dev/vda`` in
the VM.

**Consequences**

- One boot script and one RAUC configuration serve the Pi and the VM. The VM
  differs only in its kernel and U-Boot build.
- U-Boot never loads ``kds-ab.env`` as its whole environment, so the state can't
  wipe ``bootcmd``, the failure seen when ``uboot.env`` was shipped.
- Devices flashed before this change keep their old boot script, because RAUC
  doesn't update the boot partition. KDS-kk5 tracks updating it.
- Every device shares the same disk ID. That's harmless, since only one SD card
  is in a device.

DEC-029 — Boot Partition Updates and Network-Aware Health Check
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_029
   :status: resolved
   :date: 2026-11-08
   :tags: ota, boot, rauc, networking

**Decision 1 — boot partition updates (KDS-kk5).** Each release carries its boot
files in ``/usr/lib/kds-boot/`` in the rootfs. The bundle's post-install hook runs
``update-boot.sh`` from the newly written slot, which copies changed files to the
boot partition: each is written beside the original and renamed into place,
firmware and U-Boot first, the boot script last. It refuses a boot script without
a valid U-Boot image header, and fails the install, so the new slot is never
activated. ``kds-boot.version`` records which release's boot files are installed.

**Context.** The boot partition is shared by both slots, so a broken boot script
can't be rolled back by switching slots. The boot script is therefore kept
generic and stable, tested in the QEMU A/B test before every release, and only
replaced when its content changes. Devices flashed before v0.1.0-rc.4 use the
older boot design and must be reflashed.

**Decision 2 — network-aware health check (KDS-j1o).** ``rauc-mark-good`` runs
``kds-health-check.sh``: the API must answer, and when Wi-Fi is configured the
device must get an IPv4 address within two minutes. Otherwise, a configured
network that is visible in a scan means the update broke the connection, which
fails the check and leads to a rollback. A network that isn't visible is treated
as the router being off, which passes, so an outage doesn't cause reboot loops.

DEC-030 — Text-to-Speech Engine: espeak-ng
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_030
   :status: resolved
   :date: 2026-11-09
   :tags: tts, audio

.. note::

   DEC-031 proposed cloud TTS, then was deferred on 2026-11-09. This decision
   stands, extended to Danish (the default) and English, which espeak-ng speaks.

**Decision**: The speaker uses **espeak-ng** with its English voice for text-to-
speech, on the device. Its quality is accepted as adequate rather than good.

**Context**

Piper, the original choice, can't run on the Pi Zero W (no ARMv6 build). Of the
engines that do run, a listening test on the real speaker found espeak-ng's English
voice the best, ahead of flite's 16 kHz voices. espeak-ng is also the fastest:
about 0.7 s for 6 s of speech (see Hardware Measurements), and the TTS cache
(TTS_REQ_001) avoids repeating even that.

**Consequences**

- Text-to-speech works offline, with no other machine or service.
- espeak-ng outputs 22.05 kHz, which the audio HAT rejects directly; the default
  ALSA output resamples to 48 kHz.
- Better voices are possible later: espeak-ng voice variants and speed or pitch
  settings, or Piper on another machine at home with the speaker caching its
  audio. The ``Synthesizer`` interface allows either without changing the cache.

DEC-031 — Cloud Text-to-Speech: ElevenLabs and Google Cloud TTS
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_031
   :status: deferred
   :date: 2026-11-09
   :tags: tts, cloud, i18n, privacy

.. note::

   Deferred on 2026-11-09: more trouble than it's worth for now. The speaker uses
   espeak-ng on the device (DEC-030) in Danish and English. This design is kept
   for later; the cache and the ``Synthesizer`` interface already support it.

**Decision**: Synthesise speech with cloud services, ElevenLabs and Google
Cloud Text-to-Speech, tried in a configured order, and keep the disk cache so the
same announcement is synthesised once. Supersedes DEC-030.

**Context**

The speaker must speak several languages. The on-device engines that run on the
Pi Zero W sound poor even in English and worse in other languages, and neural
engines need at least a Pi 4. Both providers handle many languages with natural
voices. The cache (TTS_REQ_001) keeps cost and latency down, and lets cached
announcements play offline.

**Consequences**

- Announcement text leaves the home, so cloud TTS is opt-in (TTS_REQ_006).
- Uncached announcements need internet access and a provider account.
- Providers charge per character; the cache avoids paying twice for the same text.
- Provider credentials live on the data partition (TTS_REQ_005).

**Resolved (2026-11-09)**

- espeak-ng stays as the last resort, after both providers, so an announcement
  is never silent when the cloud is unreachable.
- Changing credentials requires an admin token (DEC-032).
- The Web UI and API use HTTPS with a self-signed certificate (DEC-032).
- Languages: Danish (``da-DK``, the default) and English (``en-GB``). Other
  languages are switched on in the config.
- No character cap. When a provider's free quota runs out, announcements fall
  through to the next provider and finally to espeak-ng on the device. Adding
  credentials for a paid account lifts the limit.

- Google authentication: an API key, restricted in the Google Cloud console to
  the Text-to-Speech API, sent in the ``x-goog-api-key`` header so it never
  appears in a URL. Google's Text-to-Speech authentication page doesn't mention
  API keys, but independent integrations use them with this API. If a project
  rejects keys, a service-account option will be added; the settings page's Test
  button shows at once whether a key works.
- The opt-in wording: "Announcements will be sent to [provider] to be turned into
  speech. They leave your home network. Messages already spoken are reused
  without being sent again."

DEC-032 — Web Security: Self-Signed HTTPS and Admin Tokens
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_032
   :status: resolved
   :date: 2026-11-09
   :tags: security, api, web

**Decision 1 — HTTPS with a self-signed certificate.** The Web UI and API are
served over HTTPS. Each device creates its own certificate on first boot and keeps
it on the data partition, so it survives updates and is never shared between
devices. Browsers warn on the first visit; the parent accepts the certificate once.
A local certificate authority was rejected: every phone and computer would have to
trust it first.

**Decision 2 — admin tokens.** API tokens gain an ``admin`` flag. Changing
credentials and other sensitive settings requires an admin token. Other tokens,
such as one issued to Home Assistant, can control playback and read state but
can't change credentials. The first token, created during setup, is an admin
token.

**Context.** Cloud TTS credentials are entered in the Web UI (TTS_REQ_005). Over
plain HTTP anyone on the home network could read them, and with equal tokens any
integration could replace them.

DEC-033 — Announcements Pause the Source; One Shared Output
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_033
   :status: resolved
   :date: 2026-11-10
   :tags: audio, tts, dsp

**Decision 1: an announcement pauses the source instead of ducking it.**
The Audio Player hosts the announcer (TTS_REQ_002). For each announcement it
synthesises the speech first (espeak-ng through the TTS cache), then pauses the
running sources with SIGSTOP, plays an "in" chime, the speech and an "out"
chime, and resumes the sources with SIGCONT. The sources are always resumed,
even if playback fails. Announcements wait in a FIFO queue of at most 10.

**Decision 2: one shared ALSA output.** ``/etc/asound.conf`` defines
``hat_mix``, a dmix on the HAT at 48 kHz, and ``speaker``, a plug in front of
it that converts any rate. Every source and the announcer write to
``speaker``. A paused source keeps its PCM open, so without dmix the chime
could not open the HAT.

**Decision 3: the DSP pipeline drops to 2 channels.** Only one source plays at a
time (AUD_REQ_001) and announcements no longer play over music, so the 6-channel
loopback, per-source faders and TTS ducking in the CamillaDSP design are not
needed. When CamillaDSP is added, it processes one stereo stream between
``speaker`` and the HAT.

**Context.** Measured on the Pi Zero W (2026-11-10): an AirPlay session from a
PipeWire sender survived shairport-sync being stopped for 6 seconds and resumed
playing at 15 % CPU; dmix played two streams at different rates at once.
AirPlay is a live stream, so after an announcement it resumes at the live
position rather than where it stopped.

**Consequences.** No CamillaDSP is needed for announcements, and the CPU budget
is one source plus the chimes and speech. Rate conversion for every stream
happens in the ``speaker`` plug.

DEC-034 — Build librespot from Source in CI
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_034
   :status: resolved
   :date: 2026-11-10
   :tags: audio, spotify, ci

**Decision**: Spotify Connect (SW_REQ_001) uses librespot 0.8.0, built by CI
from its crates.io release for ``arm-unknown-linux-gnueabihf`` and installed as
``/usr/bin/librespot``. The Audio Player starts and supervises it like
shairport-sync.

**Context**: librespot publishes no binaries, and raspotify's armhf package
states that it does not support ARMv6 (Pi 1 and Pi Zero v1.x).

**How it is built** (``make -C ci librespot``, ``ci/scripts/build-librespot.sh``):

- The crate is downloaded from crates.io and checked against a pinned SHA-256,
  then built with ``cross`` and ``--locked``, so the dependency versions are the
  ones librespot released with.
- Features: ``alsa-backend`` (plays to the ``speaker`` device, DEC-033),
  ``rustls-tls-native-roots`` (no OpenSSL to cross-compile; the image's CA
  certificates), ``with-avahi`` (Zeroconf through the avahi-daemon already
  running, instead of a second mDNS responder on port 5353).
- The cross image (``ci/cross-alsa/Dockerfile``, shared with CamillaDSP) adds the ALSA headers; the Pi's
  own ARMv6 libasound is used at run time.
- CI caches the output, keyed on the build recipe; a cache hit skips the build.
- The build writes the licence of every crate compiled in; the release's licence
  inventory includes it as ``librespot-crates.tsv``. librespot is MIT; its
  dependencies are permissive (MIT, Apache-2.0, ISC, BSD, Zlib, Unicode) plus a
  few MPL-2.0 crates, whose source is the published crate. There is no GPL code.

**Runtime**: librespot advertises the hostname (as AirPlay does), outputs S16 at
the speaker's native format, keeps credentials in ``/data/librespot`` so the
speaker stays linked across updates, and caches no audio, to spare the SD card.

**Consequences**: Upgrading librespot means changing the version and SHA-256 in
the build script. Like ring in the OTA Updater, the binary carries an ARMv7 ELF
tag from ring's assembly, which chooses instructions at run time; the ARM1176
emulation test and real playback on the Pi are what prove it runs.

DEC-035 — Bluetooth Audio with BlueALSA
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_035
   :status: resolved
   :date: 2026-11-10
   :tags: audio, bluetooth

**Decision**: Bluetooth audio from a phone or tablet (A2DP sink) uses BlueALSA
(Debian ``bluez-alsa-utils`` 4.0.0): ``bluealsa`` registers the A2DP sink with
BlueZ and ``bluealsa-aplay`` plays the stream into the ``speaker`` device
(DEC-033). PulseAudio and PipeWire were not considered: a sound server is too
heavy for the Pi Zero W and would duplicate the ALSA setup.

**Measured on the Pi Zero W (2026-11-10)**, an iPad playing over SBC:
bluetoothd, bluealsa and bluealsa-aplay together use 15 % CPU and about 15 MB.

**Findings from the test**

- The adapter must report the loudspeaker device class
  (``Class = 0x240414`` in ``/etc/bluetooth/main.conf``), or iOS won't offer it
  as a speaker.
- ``bluealsa`` must run with ``-p a2dp-sink`` only; the packaged service also
  offers ``a2dp-source``.
- The pairing agent must trust and authorise the device during pairing. When
  authorisation of AVRCP (remote control) was refused because the device was
  not yet trusted, iOS dropped the audio link after 10 seconds.
- ``bluealsa-aplay`` must be a source managed by the Audio Player, so that an
  announcement pauses it like AirPlay and Spotify (TTS_REQ_002); unmanaged, the
  announcement plays over the music.

**Implementation (2026-11-10, KDS-8c7)**

- The Bluetooth Service registers its own BlueZ agent (``org.bluez.Agent1``,
  capability ``NoInputNoOutput``) over D-Bus with the pure-Rust ``zbus`` crate,
  instead of ``bluer``, which needs libdbus cross-compiled. The agent accepts
  and trusts devices only while a pairing window is open (BT_REQ_002), and
  authorises services for trusted devices at any time.
- The AVRCP monitor runs ``bluetoothctl --agent=off``. Its default agent, in a
  session nobody types into, was what refused AVRCP during the test.
- ``/data/bluetooth`` is bind-mounted over ``/var/lib/bluetooth`` for bluetoothd
  (a systemd drop-in), so pairings survive updates (BT_REQ_003).
- The Audio Player starts ``bluealsa-aplay --pcm=speaker`` as a managed source;
  the package's ``bluealsa-aplay.service`` is disabled.

DEC-036 — CamillaDSP Behind an ALSA Loopback
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_036
   :status: resolved
   :date: 2026-11-10
   :tags: audio, dsp, licensing

**Decision 1: where CamillaDSP sits.** Every source and the announcer write to
``speaker``, a plug in front of ``hat_mix`` (a dmix at 48 kHz, DEC-033). The
dmix now writes into the playback side of an ALSA loopback (``snd-aloop``, one
substream). CamillaDSP 4.1.3 captures the other side as one stereo stream,
processes it and plays it on the HAT (``hw:IQaudIOCODEC,0``, S32_LE). The
loopback runs on its own clock, so CamillaDSP adjusts the loopback's rate to the
HAT's (``enable_rate_adjust``). A ``camilladsp`` ALSA plugin was not needed.

**Decision 2: a limiter now, EQ with tuning.** The pipeline is a limiter at
-1 dBFS on both channels, so no source or volume setting can clip the output.
The speaker's bass EQ (DSP_REQ_001) comes with DSP tuning, measured on the
speaker.

**Update (2026-11-12, KDS-9jt.6): mono, headroom, room to breathe.** The
speaker has one driver and the amplifier runs at full gain, so the pipeline is
now: left plus right (-6 dB each) to one channel, a headroom cut (-10 dB, at
least the voicing EQ's largest boost), the voicing EQ (to come), the limiter,
then the same signal to both HAT channels. The codec's own mono mix is off; it
would add the two identical channels again after the limiter. Against
Spotify's pauses as a track starts: the shared dmix buffer is 340 ms (was
170), CamillaDSP's target level four chunks (85 ms, was 43) and it runs with
real-time scheduling. Tuning is live with ``make -C ci dsp-tune`` (CamillaGUI
on a laptop through an SSH tunnel; nothing is saved on the speaker).

**Decision 3: built from source.** CamillaDSP is GPL-3.0. CI builds it from the
pinned v4.1.3 release archive (SHA-256 checked) with our own ``Cargo.lock``
(the tag ships none), cross-compiled like librespot (DEC-034) with the shared
ALSA cross image. Each release's ``sources.tar`` contains that archive and lock
file, the exact source of the binary; ``licenses.tar.gz`` lists its crates.

**Buffering**: CamillaDSP's defaults (a target of one 1024-frame chunk,
corrected every 10 s) let the HAT run dry every 30-50 s on the single-core Pi
Zero: a 10-minute radio soak had 15 underruns. A target of 2048 frames (43 ms,
about 20 ms more latency) corrected every 3 s had none over 15 minutes.

**Measured on the Pi Zero W (2026-11-10)**: 8 % CPU and 6.5 MB. A -6 dBFS tone
passes at -6.00 dB; a full-scale tone leaves at -1.00 dBFS with no clipped
samples; the loopback's rate settles within 0.01 % of the HAT's. Over AirPlay,
the sender's volume changes the level on the Pi (100 %: -6.0 dB, 50 %: -34.9 dB),
a steep curve at low volume that tuning should revisit.

**Consequences**: CamillaDSP must run for anything to be heard; it starts when
the HAT appears (udev) and keeps retrying until the HAT is there. When the last
stream stops, the loopback's capture stalls and CamillaDSP pauses; it logs one
playback underrun each time, into silence, which is expected. Its websocket listens on
localhost only. ``make -C ci hw-dsp-test`` checks levels, clipping, timing and,
optionally, the sender's volume over AirPlay.

DEC-037 — Recorded Voice Messages
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_037
   :status: resolved
   :date: 2026-11-10
   :tags: audio, messages, api

**Context**: espeak-ng sounds robotic (DEC-030), so parents can record short
voice messages instead (MSG_REQ_001-003). Decided with the user: a new message
plays at once; named messages are kept, unnamed ones expire 90 days after they
were last played.

**Decision 1: the browser sends WAV.** The Web UI records with MediaRecorder,
decodes with Web Audio and uploads 16-bit PCM WAV (24 kHz mono is about 1.4 MB
for 30 s), so the Pi needs no Opus or AAC decoder. The API Server accepts PCM
WAV of 8-48 kHz, mono or stereo, 16-bit, at most 30 s (a little slack for
headers) and at most 6 MB.

**Decision 2: the API Server owns the messages; the Audio Player plays them.**
IPC messages are limited to 64 KiB, so recordings don't travel over IPC. The
API Server stores each message in ``/data/messages`` as ``<id>.wav`` plus
``<id>.json`` (recorded time, name, duration, last played) and expires unnamed
ones. To play one it sends ``play_message {id}`` to the Audio Player, which
reads ``/data/messages/<id>.wav`` (ids are checked, so no other file can be
named) and queues it with the announcements: pause, in chime, the recording,
out chime, resume.

**Decision 3: recording needs HTTPS, the API doesn't require it.** Browsers
only allow the microphone on secure pages, so the Web UI is served over HTTPS
(WEB_REQ_001). A message is not a credential, so the API accepts it on either
listener.

DEC-038 — API Tokens and a Spoken Setup Code
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_038
   :status: resolved
   :date: 2026-11-11
   :tags: security, api, onboarding

**Decision 1: every request needs a token** (decided with the user), except
``/health`` (the update health check uses it and it reveals only "ok" and the
version), the two setup endpoints, and requests from the device itself over
loopback (anything on the Pi already runs as root; on-device scripts and the
hardware tests use this). Tokens are 256-bit random strings, shown once and
stored as SHA-256 hashes in ``/data/api/tokens.json`` with a name, an admin
flag, the creation time and when they were last used. Managing tokens (and,
later, credentials) needs an admin token over HTTPS.

**Decision 2: the first admin token comes from a spoken code** (decided with
the user). The speaker has no screen or buttons, and the SD card should not be
needed. So the speaker reads a 6-digit code aloud, in Danish, through the
announcement queue (DEC-033): only someone in the room hears it. The digits are
read slowly, one at a time (espeak-ng at 140 words a minute with 600 ms between
words, about 1.5 s a digit; ``"slow": true`` on the ``announce`` command), as
the normal speed was too fast to write down. Until an admin
token exists the speaker reads it after every start-up and repeats it, 8 s after
each reading ends, until setup is completed. ``POST /api/setup {code, name}``
exchanges the code for an admin token. Later, ``POST /api/setup/start`` asks
for a new code to recover a lost admin token (once a minute at most, to stop
announcement spam). A code allows 5 attempts and is compared in constant time.

**Rejected**: a code on the boot partition (needs the SD card), the first caller
after boot becoming admin (anyone on the network could claim the speaker), a
button (there is none).

DEC-039 — One Source at a Time: Source Events and Stopping the Others
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_039
   :status: resolved
   :date: 2026-11-11
   :tags: audio, bluetooth

**Context**: every receiver writes to the shared output (DEC-033), so nothing
stopped two senders from playing mixed (AUD_REQ_001). The arbitrator existed but
was never told what played.

**Decision 1: each source reports when it plays.** The Audio Player accepts
``source_event {source, state}`` over IPC (``active`` or ``inactive``):

- AirPlay: shairport-sync's ``run_this_before_play_begins`` and
  ``run_this_after_play_ends`` run ``audio-player notify airplay active|inactive``.
- Spotify: librespot's ``--onevent`` runs ``kds-spotify-event``, which passes
  ``$PLAYER_EVENT`` to ``audio-player notify spotify`` (``playing`` is active;
  ``paused``, ``stopped`` and ``session_disconnected`` are inactive).
- Bluetooth: the Bluetooth Service watches BlueZ's ``MediaTransport1.State``
  (``active`` while the phone streams) and reports it.
- Radio: the Audio Player starts its stream player itself (DEC-048).

**Decision 2: the newest wins, and the others are stopped the way each allows.**
Radio: the stream player stops. AirPlay and Spotify: the receiver is restarted, which ends
the session (neither offers a "stop" a receiver can send). Bluetooth: the
Bluetooth Service sends AVRCP Pause to the phone through BlueZ's
``MediaPlayer1`` (``avrcp {action}`` over IPC), so the phone shows it paused.

A sender that reconnects by itself keeps the "newest wins" rule: PipeWire's
AirPlay sink reconnects to the restarted receiver while it still has audio, and
then takes over again. An iPhone shows the AirPlay session as ended instead.

**Decision 3: announcements pause a Bluetooth phone over AVRCP** (KDS-9jt.2) and
send Play afterwards, so it continues where it was. If the phone offers no
AVRCP player, the Audio Player falls back to stopping ``bluealsa-aplay`` with
SIGSTOP as before.

DEC-040 — Device Volume and the Volume Limit in CamillaDSP
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_040
   :status: resolved
   :date: 2026-11-11
   :tags: audio, dsp, parental

**Decision**: the device volume (VOL_REQ_001) is CamillaDSP's main volume, which
every source passes through (DEC-036), so one setting covers AirPlay, Spotify,
Bluetooth, radio, podcasts and announcements alike. The Audio Player owns it:
it keeps the volume and the limit in ``/data/audio/volume.json`` and sets
CamillaDSP over its localhost websocket (``SetVolume``, ``SetMute``) at start-up
and on every change. CamillaDSP's statefile (``/data/camilladsp/statefile.yml``)
restores the level if CamillaDSP restarts on its own.

**Default**: the volume starts at the limit, 75 % (-15 dB). This is
provisional: the speaker is a raw driver without its cabinet, so the default,
the limit and the curve are set when it is in the cabinet, together with the
DSP tuning (KDS-9jt.6).

**Superseded in part by DEC-050 (2026-11-13)**: the volume is now the HAT
amplifier's analog gain over 63 dB, with CamillaDSP at 0 dB.

**Curve**: 0 % mutes; 1-100 % maps linearly onto -60 to 0 dB (50 % is -30 dB),
an even curve in loudness. The limit caps the percentage: asking for more than
the limit gives the limit, and lowering the limit lowers a higher volume.

**Context**: the volume limit was a number held in the API Server's memory and
enforced nowhere. Each sender's own volume still applies before this one, so a
phone at half volume and the device at 75 % play at both.

DEC-041 — Sleep Timer and Alarms: Fades, Fallback and Local Time
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_041
   :status: resolved
   :date: 2026-11-11
   :tags: audio, alarm, timer

Implements DEC-021 (SLP_REQ_001, ALM_REQ_001). As DEC-021 says, the API Server
schedules both and drives the Audio Player over IPC.

- **Sleep timer**: ``POST /api/timer {minutes}`` (1-240), ``GET`` and ``DELETE``.
  At the end the API Server steps the device volume (DEC-040) down to 0 over
  30 s, asks the Audio Player to ``stop_all`` (radio and podcasts stop, AirPlay
  and Spotify sessions end, a Bluetooth phone pauses, as in DEC-039), then puts
  the volume back so the next listen isn't silent.
- **Alarms**: ``/api/alarms`` (list, add, change, remove), kept in
  ``/data/api/alarms.json``. The scheduler checks every 15 s whether an alarm's
  time fell since the last check, so a busy moment doesn't skip one. It never
  catches up more than 2 minutes, so the jump when the clock is first set after
  boot (the Pi has no RTC, DEC-019) doesn't fire a morning's alarms at once.
  When an alarm goes off: volume to half the alarm's volume, play the
  station, then rise to the alarm's volume over a minute. If the station fails
  (no internet), the speaker says "Godmorgen" with the chimes.
- **Local time**: the image's time zone is Europe/Copenhagen; alarm times are
  local time, so they follow summer time.

DEC-042 — A Bluetooth Remote Through Linux Input Events
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_042
   :status: resolved
   :date: 2026-11-11
   :tags: bluetooth, controls

**Decision**: when a Bluetooth remote sends AVRCP buttons, BlueZ turns them into
key presses on a Linux input device (``/dev/input/eventN`` with bus type
Bluetooth). The Bluetooth Service watches for such devices, reads their key
events (``KEY_PLAYPAUSE``, ``KEY_NEXTSONG``, ``KEY_PREVIOUSSONG``,
``KEY_VOLUMEUP``, ``KEY_VOLUMEDOWN`` and the CD variants) and sends the Audio
Player ``remote {action}`` over IPC, which acts on it (RMT_REQ_001). Held
volume keys repeat.

**Context**: the first design expected ``bluetoothctl`` to print ``Key:`` lines;
it prints property changes, not key presses. That monitor stays in place until
a real remote confirms the input path, then goes.

**Status**: unit-tested with recorded input events; not yet tried with a real
remote.

DEC-043 — Discovery over mDNS and Home Assistant through the REST API
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_043
   :status: resolved
   :date: 2026-11-11
   :tags: networking, integration

**Decision**: avahi, which already advertises AirPlay and Spotify, also
advertises ``_kidsspeaker._tcp`` (TXT: ``path=/api``, ``auth=bearer``,
``api=1``) and ``_https._tcp`` on port 443 (DSC_REQ_001), from a static service
file. Home Assistant uses its ``rest`` and ``rest_command`` integrations with a
token created for it (AUTH_REQ_002); the API reference documents a working
configuration. A dedicated Home Assistant integration is not needed for v1.

**Context**: Home Assistant's ``rest`` integration only switches certificate
checks on or off, so with the speaker's self-signed certificate it needs
``verify_ssl: false``: still encrypted, but without proof of the speaker's
identity, which is acceptable on the home network.

DEC-044 — Wi-Fi Onboarding: Never Forget the Network, Always Come Back
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_044
   :status: resolved
   :date: 2026-11-11
   :tags: onboarding, wifi, networking

Refines DEC-012 (NET_ONB_001/002).

**Decision 1: saved Wi-Fi is never deleted.** DEC-012 cleared the credentials
after 5 failed connections; a router down for a few minutes would then leave
the speaker waiting for setup. Instead, after 5 failed checks (30 s apart, after
a 2-minute grace at boot) the speaker opens its setup access point for 10
minutes and then tries the saved network again, alternating until one works.
With no saved network it opens the access point at once and keeps it open.

**Decision 2: systemd bounds the access point.** The access point is a systemd
service (``kds-setup-ap.service``: hostapd, dnsmasq, the address 192.168.4.1)
with ``RuntimeMaxSec`` of 10 minutes when a network is saved, and an
``ExecStopPost`` that restores normal Wi-Fi. Even if the onboarding program
dies, the speaker returns to its network.

**Decision 3: the setup page writes the boot partition.** The captive portal
(``http://192.168.4.1``, every DNS name answered with that address so phones
open it) saves the network to ``/boot/firmware/kds/wifi.conf``, the file
``setup-sd.sh`` writes and ``kds-device-config`` applies at boot (DEC-025). It
survives updates and there is one source of truth.

**Decision 4: the speaker says what to do.** Entering setup mode, it announces
(in Danish) "Forbind til netværket KidsSpeaker-xxxx for at sætte højttaleren
op". The access point is open, as DEC-012 decided: only Wi-Fi credentials can be
set through it, and they are only useful to someone in range.

DEC-045 — The Web UI: Preact, TypeScript and Storybook
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_045
   :status: resolved
   :date: 2026-11-11
   :tags: web, ui

Decided with the user (wave 5, KDS-9pb review).

**Decision 1: Preact and TypeScript, built in CI.** The Web UI is a small
Preact app written in TypeScript and built with Vite in CI into static files.
The API Server (Rust, unchanged) serves them at ``/`` over HTTPS; nothing is
built or rendered on the Pi. This replaces the Leptos plan in the solution
design, because the user wants a Storybook to validate the UI, and Storybook
needs a JavaScript UI.

**Decision 2: Storybook for validation.** Every screen and component has
stories for its states (empty, playing, errors, setup, a child's device),
rendered with mocked API data, so the UI can be checked in a browser without a
speaker. CI builds the Storybook with every change; component tests run with
Vitest.

**Decision 3: from the design review.** Four tabs (Afspiller, Radio, Beskeder,
Indstillinger). A child's device (a non-admin token) sees only the Player and
Radio. Sharing access shows the token once with a Copy button and a QR code
linking ``https://<hostname>.local/#token=...``; the URL fragment never reaches
the server. Danish by default, English as an option (LOC_REQ_001).

DEC-046 — Live Updates: Server-Sent Events from the API Server
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_046
   :status: resolved
   :date: 2026-11-12
   :tags: web, ui, api

**Context**: the Web UI polled each endpoint every 5 to 15 s, so a change
made elsewhere (AirPlay starting, a remote's volume button, another phone)
showed late, and the update reboot countdown (OTA_REQ_010) couldn't be shown
(UI_REQ_004, KDS-apx.7, KDS-u1v). The solution design planned a WebSocket at
``/ws`` with the token in the query string, which puts tokens in URLs and logs.

**Decision 1: Server-Sent Events at** ``GET /api/events``. One-way JSON events
over a normal HTTP response (``text/event-stream``), authenticated with the
usual ``Authorization: Bearer`` header by the same middleware as every other
route; any token may subscribe. The browser reads it with ``fetch`` and a
stream reader (``EventSource`` can't send headers), and reconnects with
back-off. Nothing needs to flow from the browser, so a WebSocket adds nothing,
and Home Assistant or ``curl -N`` can use the same stream.

**Decision 2: one event per topic, the same JSON as its endpoint.** Topics:
``source`` (``GET /api/audio/source``), ``volume`` (``/api/audio/volume``),
``timer`` (``/api/timer``), ``bluetooth`` (``/api/bluetooth/status``) and
``ota`` (``/api/ota/status``). A new subscriber first gets every topic, then
an event only when a topic changes. Counters that tick (``remaining_s``) don't
count as a change; the UI counts down itself.

**Decision 3: the API Server watches, the backends are unchanged.** An Event
Hub in the API Server asks the backends over IPC every 2 s, only while someone
subscribes, compares with what it sent last, and sends what changed. After a
successful change through the API (any non-GET ``/api`` request) it looks at
once, so the sender's own change arrives immediately. Push subscriptions in
every backend would be more code in four services for little gain: the IPC
requests are local and cheap, and nobody subscribes most of the time.

**Decision 4: the reboot countdown comes from the OTA Updater.** Its status
gains ``reboot_at`` (Unix seconds) while a reboot waits out its grace period,
and ``null`` otherwise. The Web UI shows a banner with the countdown on every
screen and a Reboot now button for a parent (OTA_REQ_007).

**Consequences**: lists that only change through the API (stations, messages,
alarms, tokens) are not pushed; a screen reloads them after its own changes.

DEC-047 — Update Bundle Size: Trim the Image, Check Space First
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_047
   :status: resolved
   :date: 2026-11-12
   :tags: ota, image

**Context** (KDS-sgt.8): the OTA Updater downloads the whole bundle to
``/data/ota/staging`` on the 1 GB data partition before RAUC installs it. A dev
bundle had reached 497 MB (rootfs 1,074 MB), close to RAUC's 512 MB
``max-bundle-download-size`` and to half the partition, which also holds voice
messages and settings.

**Decision 1: the image leaves out what a speaker never reads.** A last
Dockerfile layer removes the apt cache (203 MB of ``.deb`` files and indexes
nobody cleaned), man and info pages, documentation except each package's
``copyright`` file (the SBOM and license inventory read those), and
translations other than Danish and English. The smoke test checks it.

**Decision 2: dev builds carry line tables only.** Dev binaries go onto the Pi
too (``hw-install``); full debug info made them 344 MB together. Line tables
keep file and line numbers in backtraces; dependencies carry none. They are now
100 MB.

**Decision 3: the OTA Updater checks free space before downloading.** If the
bundle plus 32 MB doesn't fit, it doesn't start, reports "not enough space for
the update: it needs N MB, M MB free" in its status, and tries again at the
next check. A leftover bundle from an interrupted download is removed first.

**Result**: rootfs 1,074 → 550 MB, dev bundle 497 → 245 MB; release bundles,
with 10 MB of release binaries instead of 100 MB, are smaller still.

**Not now**: RAUC HTTP streaming would install from the URL without a local
copy, and the kernel has what it needs (``nbd``, dm-verity), but Raspbian's
RAUC 1.8 is built without streaming, so it would mean building and maintaining
RAUC ourselves. The largest remaining part was MPD's dependencies (FFmpeg with
video encoders, Samba, librsvg, codec2), removed with MPD in DEC-048.

DEC-048 — Radio and Podcasts in the Audio Player, without MPD
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_048
   :status: resolved
   :date: 2026-11-12
   :tags: audio, radio, podcast, image

Decided with the user (KDS-9jt.5). Supersedes DEC_005.

**Context**: Debian's ``mpd`` links every plugin's library, so it brought 144
packages (~140 MB installed: Samba, FFmpeg with video encoders, codec2,
librsvg, fonts, a soundfont, PipeWire) to play HTTP streams. A minimal MPD
built in CI would have dropped ``.m4a`` podcast episodes (no MP4 demuxer
without FFmpeg).

**Decision**: the Audio Player plays radio and podcasts itself
(``audio-player/src/stream/``): HTTP with ICY titles and ``Range`` seeks,
symphonia (pure Rust) decoding MP3, AAC including ``.m4a``, Ogg Vorbis, FLAC
and WAV, and ``aplay -D speaker`` as the output, as announcements use. The
rest of the Audio Player sees the same operations it used on MPD (play, seek,
pause, stop, status).

**Measured on the Pi Zero W** (DR P1, 128 kbit/s MP3): 9.3 % CPU decoding plus
4.3 % for aplay and 5 MB, against MPD's 12.3 % and 39 MB. Opus streams are not
supported (symphonia has no Opus decoder); none of the default stations use it.

**Consequences**: no MPD service, port 6600 or configuration; a live stream
that drops reconnects 3 times; the image loses MPD's packages. symphonia is
MPL-2.0 (file-level copyleft), now allowed in ``deny.toml``: it is used
unmodified and its source is the published crate, as for librespot's MPL-2.0
crates (DEC-034).

DEC-049 — One Volume: the Phone's Volume Sets the Speaker's
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_049
   :status: resolved
   :date: 2026-11-12
   :tags: audio, volume

Decided with the user (KDS-9jt.6, VOL_REQ_002).

**Context**: every source already plays through CamillaDSP and its device
volume (DEC-040), but AirPlay and Spotify also scaled their audio by the
phone's volume (librespot at 50 % by default), so music sat well below the
chimes and messages that interrupted it, and each source kept its own level.

**Decision**: senders play at full level and their volume becomes the device
volume:

- **AirPlay**: shairport-sync ignores the volume for the audio
  (``ignore_volume_control``) and runs
  ``audio-player notify airplay volume <dB>`` on each change (-30 to 0 dB,
  -144 for mute).
- **Spotify**: librespot's volume is fixed (``--volume-ctrl fixed``, 100
  steps); its ``volume_changed`` event (0-65535) goes through the event hook.
- **Bluetooth**: BlueALSA uses native volume (``--a2dp-volume``), so it doesn't
  scale the audio; the Bluetooth Service watches BlueZ's
  ``MediaTransport1.Volume`` (AVRCP, 0-127) and reports it.

The Audio Player's ``source_volume {source, value}`` maps the sender's whole
range onto 0 to the maximum volume (a phone all the way up plays at the limit,
and every step of its slider counts) and sets the device volume, which the
next source keeps. The Web UI's slider moves in 1 % steps.

**Not yet**: the speaker's volume isn't sent back to the phone, so a phone's
slider shows its own last value until it is moved.

DEC-050 — Volume on the Amplifier; Headroom for the Bass Boost
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. meta::
   :decision-id: DEC_050
   :status: resolved
   :date: 2026-11-13
   :tags: audio, volume, dsp

Decided with the user (KDS-9jt.6). Refines DEC-040.

**Decision 1: the device volume is the amplifier's gain.** The amplifier is
the DA7213 codec's line-out on the HAT; its ``Lineout Volume`` is an analog
gain after the DAC, 0-63 in 1 dB steps (48 is 0 dB). The volume maps onto its
whole range: 100 % is step 63 (``LINEOUT_FULL``, full output), each 1.6 % is
about one step, 0 % mutes CamillaDSP. CamillaDSP's volume stays at 0 dB, so
the DAC always gets the whole digital signal and its noise drops with the
volume. If the codec can't be set (no HAT, the VM), CamillaDSP's volume takes
over on the same 63 dB curve. If 1 dB steps turn out too coarse, CamillaDSP
adds the steps in between.

**Decision 2 (to implement with the voicing): how the bass boost avoids
clipping without costing loudness.**

*It is not a resolution problem.* CamillaDSP calculates in 64-bit floating
point, so nothing clips inside the pipeline, however large the boost; a signal
clips only where it is converted to the DAC's integers (S32_LE) and exceeds
full scale. A 32-bit output has far more resolution than the DA7213's DAC
(about 24 bits, ~100 dB). Cutting the digital level by 10 dB and making it up
with the analog gain after the DAC costs at most those 10 dB of the DAC's
signal-to-noise ratio (~90 dB left): measurable, not audible on this speaker.

*What limits loudness is the amplifier's voltage swing.* A +10 dB bass boost
on music that already peaks at full scale needs 10 dB more swing than there
is, whatever the DSP does. So the plan is:

1. **Headroom made up in analog**: the digital cut before the EQ equals its
   largest boost; with the volume on the amplifier (Decision 1) the analog gain
   makes it up, so the top of the volume range still reaches full output.
2. **Loudness-style bass**: full boost at low and middle volume, fading
   towards the top, so the loudest setting plays nearly flat at the
   amplifier's full output. CamillaDSP's Loudness filter follows a fader; the
   Audio Player sets that fader to mirror the volume (CamillaDSP's own volume
   stays at 0 dB).
3. **A high-pass below what the driver can play**, so no swing is spent on
   bass the driver can't reproduce.
4. **A compressor or soft limiter last**, so rare peaks are tamed instead of
   forcing more static headroom.

The amounts (boost, high-pass, where the boost fades, ``LINEOUT_FULL``) come
from voicing with the cabinet and a measurement microphone.

