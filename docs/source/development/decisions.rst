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
