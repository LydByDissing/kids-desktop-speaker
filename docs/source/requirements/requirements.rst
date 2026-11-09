Requirements
============

This section documents all project requirements for the Kids Desktop Speaker,
organized by category. Requirements are managed with `sphinx-needs`_ for full
traceability.

.. _sphinx-needs: https://sphinx-needs.readthedocs.io/

Overview
--------
Small form factor streaming speaker that plays back content from Spotify, AirPlay,
various podcast sources, and internet radio. Primary controls are through native
apps on smart devices (phones and iPads). A simple web interface is also available
to control podcast and internet radio playback. Bluetooth is used as a control
interface for volume and playback.

Hardware
--------

.. req:: HW_REQ_001
   :id: HW_REQ_001
   :status: open
   :tags: hardware

   The system shall be built on a Raspberry Pi Zero W (version 1) as the base
   computing platform.

.. req:: HW_REQ_002
   :id: HW_REQ_002
   :status: open
   :tags: hardware

   The system shall use the IQaudIO Pi-Codec Zero HAT for audio amplification
   and output.

.. req:: HW_REQ_003
   :id: HW_REQ_003
   :status: open
   :tags: hardware

   The system shall support Bluetooth connectivity for control interfaces.

.. req:: HW_REQ_004
   :id: HW_REQ_004
   :status: open
   :tags: hardware

   An external Bluetooth AVRCP controller shall be the primary physical control
   surface; the device has no onboard buttons (ref DEC-014).

Software — Audio Streaming
--------------------------

.. req:: AUD_REQ_001
   :id: AUD_REQ_001
   :status: open
   :tags: software, audio

   Only one audio source shall be active at a time. The CPU budget is therefore
   the DSP chain plus the most demanding single source, not all sources at once
   (see Hardware Measurements). A TTS announcement interrupts the active source
   (TTS_REQ_002) rather than playing over it.

.. req:: SW_REQ_001
   :id: SW_REQ_001
   :status: open
   :tags: software, streaming

   The system shall support Spotify Connect for streaming music from Spotify.

.. req:: SW_REQ_002
   :id: SW_REQ_002
   :status: open
   :tags: software, streaming

   The system shall support AirPlay for streaming audio from Apple devices.

.. req:: SW_REQ_003
   :id: SW_REQ_003
   :status: open
   :tags: software, streaming

   The system shall support playback from podcast sources (RSS-based).

.. req:: SW_REQ_004
   :id: SW_REQ_004
   :status: open
   :tags: software, streaming

   The system shall support internet radio streaming (e.g., Icecast, Shoutcast).

Software — DSP
--------------

.. req:: DSP_REQ_001
   :id: DSP_REQ_001
   :status: open
   :tags: software, dsp

   The system shall integrate CamillaDSP as the DSP engine to enhance sound
   quality, particularly in the bass region.

   Reference: https://github.com/HEnquist/camilladsp

Software — User Interface
-------------------------

.. req:: UI_REQ_001
   :id: UI_REQ_001
   :status: open
   :tags: software, ui

   The system shall provide a web interface for controlling playback from any
   browser.

.. req:: UI_REQ_002
   :id: UI_REQ_002
   :status: open
   :tags: software, ui

   The web interface shall support volume control, playback control (play/pause/
   skip), and source selection.

.. req:: UI_REQ_003
   :id: UI_REQ_003
   :status: deferred
   :tags: software, ui

   The system shall provide native mobile apps for phones and iPads.

   Note: native mobile apps — explicitly ruled out, DEC-012 builds no app.

Software — Features
-------------------

.. req:: FEAT_REQ_001
   :id: FEAT_REQ_001
   :status: deferred
   :tags: software, feature

   The system shall support sending custom text messages that are read aloud
   using a text-to-speech engine.

   Note: deferred per DEC-015. Work on the TTS audio cache (TTS_REQ_001) started
   on 2026-11-09 at the product owner's request; the engine choice is KDS-o2d.

.. req:: TTS_REQ_001
   :id: TTS_REQ_001
   :status: open
   :tags: software, tts, performance

   Synthesised speech shall be cached on the data partition, keyed by the text,
   language, provider, model, voice and audio format, and reused instead of
   synthesised again. An entry shall be
   removed 90 days after it was last used. Synthesis takes about 2 seconds on the
   Pi Zero W, and the same messages repeat often.

.. req:: TTS_REQ_002
   :id: TTS_REQ_002
   :status: open
   :tags: software, tts, audio

   A TTS announcement shall pause the active source, play a short "in" chime,
   speak the announcement, play a short "out" chime, then resume the source where
   it was paused. Nothing else plays during the announcement.

.. req:: TTS_REQ_003
   :id: TTS_REQ_003
   :status: open
   :tags: software, tts, i18n

   Each announcement shall carry a language (a BCP 47 tag such as ``da-DK`` or
   ``en-GB``) and be spoken in that language, with a voice configured per language
   (ref DEC-031).

.. req:: TTS_REQ_004
   :id: TTS_REQ_004
   :status: open
   :tags: software, tts, cloud

   Speech shall be synthesised by a cloud service: ElevenLabs and Google Cloud
   Text-to-Speech, tried in a configured order. When a provider fails or is
   unreachable, the next one shall be tried. A cached announcement shall play
   without contacting any provider.

.. req:: TTS_REQ_005
   :id: TTS_REQ_005
   :status: open
   :tags: software, tts, security

   Provider credentials shall be stored on the data partition, readable only by
   the TTS Engine, and entered by a parent in the Web UI (Config → Text-to-speech)
   or through the API. They shall be write-only: the UI and API show whether a
   provider is configured and the last four characters of its key, never the key.
   They shall never be part of the image, the repository or logs. Credentials
   shall only be accepted over HTTPS, so they don't cross the network in clear,
   and only from an admin token (DEC-032).

.. req:: TTS_REQ_006
   :id: TTS_REQ_006
   :status: open
   :tags: software, tts, privacy

   Announcement text is sent to the chosen provider, outside the home. Cloud TTS
   shall be off until a parent enables it, after being told which provider
   receives the text.

.. req:: FEAT_REQ_002
   :id: FEAT_REQ_002
   :status: deferred
   :tags: software, feature

   The system shall support multi-room audio — synchronizing playback across
   multiple speakers on the same network.

   Note: too complex for Pi Zero W v1, no ADR.

.. req:: FEAT_REQ_003
   :id: FEAT_REQ_003
   :status: open
   :tags: software, feature

   The system shall support customizable EQ settings to tailor sound to user
   preferences.

.. req:: FEAT_REQ_004
   :id: FEAT_REQ_004
   :status: deferred
   :tags: software, feature

   The system shall support integration with voice assistants (e.g., Alexa,
   Google Assistant) for voice control.

   Note: out of scope v1, no ADR.

.. req:: FEAT_REQ_005
   :id: FEAT_REQ_005
   :status: open
   :tags: software, feature

   The system shall support automation platforms (e.g., Home Assistant, IFTTT)
   for smart home integration.

.. req:: FEAT_REQ_006
   :id: FEAT_REQ_006
   :status: open
   :tags: software, feature, parental

   The system shall provide parental controls including volume limiting, content
   filtering, and usage time limits.

.. req:: FEAT_REQ_007
   :id: FEAT_REQ_007
   :status: open
   :tags: software, feature

   The system shall enforce a configurable maximum volume limit, defaulting to
   75% of hardware output range (ref DEC-020).

.. req:: FEAT_REQ_008
   :id: FEAT_REQ_008
   :status: open
   :tags: software, feature

   The system shall support scheduled alarms that play audio at a configured
   time and day of week (ref DEC-021).

.. req:: FEAT_REQ_009
   :id: FEAT_REQ_009
   :status: open
   :tags: software, feature

   The system shall support a sleep timer that stops playback after a configured
   duration (ref DEC-021).

Software — Reliability & Performance
-------------------------------------

.. req:: REL_REQ_001
   :id: REL_REQ_001
   :status: open
   :tags: software, reliability

   The system shall run continuously without crashes or significant performance
   degradation.

.. req:: PRF_REQ_001
   :id: PRF_REQ_001
   :status: open
   :tags: software, performance

   The system shall ensure low latency for audio playback and control commands.

Software — Networking
---------------------

.. req:: NET_REQ_001
   :id: NET_REQ_001
   :status: open
   :tags: software, networking

   The system shall support Wi-Fi for network connectivity.

.. req:: NET_REQ_002
   :id: NET_REQ_002
   :status: open
   :tags: software, networking

   The system shall support over-the-air (OTA) updates for the entire system
   (OS, software, configs) using RAUC with A/B partitions.

.. req:: OTA_REQ_001
   :id: OTA_REQ_001
   :status: open
   :tags: software, ota

   The system shall provide atomic OTA updates: updates are written to an
   inactive partition and activated only after successful installation.

.. req:: OTA_REQ_002
   :id: OTA_REQ_002
   :status: open
   :tags: software, ota

   The system shall automatically roll back to the previous system version
   if a new update fails to boot.

.. req:: OTA_REQ_003
   :id: OTA_REQ_003
   :status: open
   :tags: software, ota, data

   User data (radio stations, podcast feeds, alarms, API tokens, Bluetooth
   pairings, voice models) shall persist across OTA updates and rollbacks.

.. req:: OTA_REQ_004
   :id: OTA_REQ_004
   :status: open
   :tags: software, ota

   The OTA Updater shall periodically query the project's GitHub Releases for
   the latest release and, when the release manifest's version is newer than
   the running system version, download the RAUC bundle and verify its
   SHA-256 against the manifest before invoking ``rauc install``. A bundle
   whose checksum does not match the manifest shall not be installed (consumer
   side of CI_REQ_004).

.. req:: OTA_REQ_005
   :id: OTA_REQ_005
   :status: open
   :tags: software, ota

   The running system version shall be recorded at image build time in
   ``/etc/kds-version`` (derived from the release tag). The OTA Updater shall
   read this file as the current version for update comparison.

.. req:: OTA_REQ_006
   :id: OTA_REQ_006
   :status: open
   :tags: software, ota

   After installing an update, the OTA Updater shall reboot the device to
   activate it only inside a configurable maintenance window (default
   03:00–05:00 local time), only while no audio is playing, and only after a
   grace period of 5 minutes during which activity is checked again.

.. req:: OTA_REQ_007
   :id: OTA_REQ_007
   :status: open
   :tags: software, ota, api

   The API Server shall let a user check for an update immediately and reboot
   immediately, the latter skipping the maintenance window.

.. req:: OTA_REQ_008
   :id: OTA_REQ_008
   :status: open
   :tags: software, ota, boot

   An update shall also update the boot partition (firmware, U-Boot, boot script,
   device tree, ``config.txt``) when its files differ, copying only changed files,
   replacing each atomically, and writing the boot script last. An invalid boot
   script shall abort the install before the new slot is activated (ref DEC-029).

.. req:: OTA_REQ_009
   :id: OTA_REQ_009
   :status: open
   :tags: software, ota, networking, reliability

   A slot shall only be marked good once the API answers and, when Wi-Fi is
   configured, the device has an IPv4 address. A slot whose Wi-Fi interface or
   supplicant is missing, or which can see its configured network but not join
   it, shall fail the check. A configured network that isn't visible shall not
   fail it, so a router outage doesn't cause reboot loops (ref DEC-029).

.. req:: CI_REQ_001
   :id: CI_REQ_001
   :status: open
   :tags: infrastructure, ci

   The project shall provide a CI pipeline that builds the system image on
   x86_64 hardware using QEMU user-mode emulation for ARM cross-architecture
   support.

.. req:: CI_REQ_002
   :id: CI_REQ_002
   :status: open
   :tags: infrastructure, ci

   The CI pipeline shall produce a RAUC bundle (``.raucb``) as a build
   artifact for each release.

.. req:: CI_REQ_003
   :id: CI_REQ_003
   :status: open
   :tags: infrastructure, ci

   The CI pipeline shall run smoke tests on the built image to verify
   services start correctly and RAUC can query slot status.

.. req:: CI_REQ_004
   :id: CI_REQ_004
   :status: open
   :tags: infrastructure, ci, ota

   On a version tag (``v*.*.*``), the CI pipeline shall publish the RAUC
   bundle as a GitHub Release asset together with a version manifest that
   declares the release version and the bundle's SHA-256 checksum, so the
   OTA Updater can discover, verify, and download the update (ref DEC-001,
   OTA_REQ_001, and the ota-updater ``GitHub Releases Integration`` design).

.. req:: CI_REQ_005
   :id: CI_REQ_005
   :status: open
   :tags: infrastructure, ci, ota, security

   Bundles published on a version tag shall be signed with a stable identity
   that verifies against the keyring in the device image. The release shall
   fail if that identity is unavailable or the bundle does not verify. Tags
   containing a pre-release suffix shall be published as GitHub prereleases
   (ref DEC-023).

.. req:: CI_REQ_006
   :id: CI_REQ_006
   :status: open
   :tags: infrastructure, ci, hardware

   The system image shall run on the Pi Zero W's ARMv6 CPU. The CI pipeline shall
   build and test the image under emulation of that CPU, and shall fail if any
   tested program crashes on it (ref DEC-024).

.. req:: CI_REQ_007
   :id: CI_REQ_007
   :status: open
   :tags: infrastructure, ci, hardware

   The CI pipeline shall produce a flashable SD card image for the Pi Zero W with
   A/B root slots. The image shall contain no shared credentials: no known passwords,
   SSH keys, host keys or Wi-Fi details (ref DEC-025).

.. req:: CI_REQ_008
   :id: CI_REQ_008
   :status: open
   :tags: infrastructure, ci, licensing

   Each release shall publish a license inventory listing every third-party
   package and Rust crate it distributes, with versions, source versions and
   licenses, plus each package's copyright file (ref KDS-khs).

.. req:: NET_REQ_003
   :id: NET_REQ_003
   :status: open
   :tags: software, networking

   The system shall provide basic functionality (e.g., Bluetooth playback) when
   not connected to the internet.

.. req:: NET_ONB_001
   :id: NET_ONB_001
   :status: open
   :tags: software, networking, onboarding

   The system shall provide a Wi-Fi onboarding flow via AP mode and captive
   portal for first-time setup (ref DEC-012).

.. req:: NET_ONB_002
   :id: NET_ONB_002
   :status: open
   :tags: software, networking, onboarding

   The system shall automatically re-enter AP mode onboarding after 5 consecutive
   Wi-Fi connection failures (ref DEC-012).

Software — Security
-------------------

.. req:: SEC_REQ_001
   :id: SEC_REQ_001
   :status: open
   :tags: software, security

   The system shall secure communication between the speaker and control devices
   (encryption for Bluetooth and network communication).

Software — Accessibility
------------------------

.. req:: ACC_REQ_001
   :id: ACC_REQ_001
   :status: open
   :tags: software, accessibility

   The system shall support accessibility features including screen reader
   compatibility and voice feedback.

.. req:: LOC_REQ_001
   :id: LOC_REQ_001
   :status: open
   :tags: software, localization

   The system shall support multiple languages in the user interface and voice
   feedback.

Software — Internal Communication
---------------------------------

.. req:: IPC_REQ_001
   :id: IPC_REQ_001
   :status: open
   :tags: software, ipc

   Services shall communicate over Unix domain sockets in ``/run/kds`` using
   newline-delimited JSON, with one request and one response per connection
   (ref DEC-027).

.. req:: IPC_REQ_002
   :id: IPC_REQ_002
   :status: open
   :tags: software, ipc

   Every response shall contain ``ok``. A failed request shall receive
   ``ok: false`` and an ``error`` message, including for malformed, oversized
   or unknown requests, rather than a closed connection.

.. req:: IPC_REQ_003
   :id: IPC_REQ_003
   :status: open
   :tags: software, ipc, reliability

   A server shall create ``/run/kds`` if missing and replace a stale socket, so
   services work regardless of start order. Messages over 64 KiB shall be
   rejected.

.. req:: IPC_REQ_004
   :id: IPC_REQ_004
   :status: open
   :tags: software, ipc, api

   The API Server shall report live service state by querying the services'
   sockets, and shall answer ``503 Service Unavailable`` when a service can't be
   reached within its timeout.

Software — Diagnostics
----------------------

.. req:: LOG_REQ_001
   :id: LOG_REQ_001
   :status: open
   :tags: software, diagnostics

   The system shall provide comprehensive logging for troubleshooting and
   debugging.

Documentation
-------------

.. req:: DOC_REQ_001
   :id: DOC_REQ_001
   :status: open
   :tags: documentation

   The project shall provide clear and concise end-user documentation for setup
   and usage.

.. req:: DOC_REQ_002
   :id: DOC_REQ_002
   :status: open
   :tags: documentation

   The project shall provide developer documentation for extending or modifying
   the software.

.. req:: DOC_REQ_003
   :id: DOC_REQ_003
   :status: open
   :tags: documentation

   The project shall document all APIs exposed for third-party integration.

Compliance
----------

.. req:: CMP_REQ_001
   :id: CMP_REQ_001
   :status: open
   :tags: compliance

   The system shall comply with relevant regulations (e.g., FCC, CE, RoHS).

.. req:: CMP_REQ_002
   :id: CMP_REQ_002
   :status: open
   :tags: compliance

   The system shall obtain necessary certifications (e.g., Bluetooth SIG, Wi-Fi
   Alliance).

Testing
-------

.. req:: TST_REQ_001
   :id: TST_REQ_001
   :status: open
   :tags: testing

   The project shall include comprehensive unit tests for all software components.

.. req:: TST_REQ_002
   :id: TST_REQ_002
   :status: open
   :tags: testing

   The project shall include integration tests to verify all components work
   together.

.. req:: TST_REQ_003
   :id: TST_REQ_003
   :status: open
   :tags: testing

   The project shall include user acceptance testing with real users.

.. req:: TST_REQ_004
   :id: TST_REQ_004
   :status: open
   :tags: testing

   The project shall include stress testing for high-load scenarios.

.. req:: TST_REQ_005
   :id: TST_REQ_005
   :status: open
   :tags: testing

   The project shall include penetration testing to identify security
   vulnerabilities.

User Stories
------------

.. req:: UST_REQ_001
   :id: UST_REQ_001
   :status: open
   :tags: user_story

   As a user, I want to stream music from Spotify so that I can listen to my
   favorite songs.

.. req:: UST_REQ_002
   :id: UST_REQ_002
   :status: open
   :tags: user_story

   As a user, I want to use AirPlay to stream audio from my Apple devices.

.. req:: UST_REQ_003
   :id: UST_REQ_003
   :status: open
   :tags: user_story

   As a user, I want to listen to podcasts so that I can stay informed and
   entertained.

.. req:: UST_REQ_004
   :id: UST_REQ_004
   :status: open
   :tags: user_story

   As a user, I want to listen to internet radio so that I can discover new music
   and shows.

.. req:: UST_REQ_005
   :id: UST_REQ_005
   :status: open
   :tags: user_story

   As a user, I want to control the speaker using my phone or iPad so that I can
   easily manage playback.

.. req:: UST_REQ_006
   :id: UST_REQ_006
   :status: open
   :tags: user_story

   As a user, I want to use a simple website to control the speaker so that I can
   manage playback from any device.

.. req:: UST_REQ_007
   :id: UST_REQ_007
   :status: open
   :tags: user_story

   As a user, I want to use Bluetooth to control volume and playback so that I
   can adjust settings without using my phone.

.. req:: UST_REQ_008
   :id: UST_REQ_008
   :status: open
   :tags: user_story

   As a parent, I want to set volume limits so that my child's hearing is
   protected.

.. req:: UST_REQ_009
   :id: UST_REQ_009
   :status: open
   :tags: user_story

   As a user, I want to send a text message to the speaker so that I can deliver
   a custom voice announcement.

.. req:: UST_REQ_010
   :id: UST_REQ_010
   :status: open
   :tags: user_story

   As a user, I want to sync multiple speakers so that I can have music
   throughout my home.

Future Enhancements
-------------------

- Sleep sounds / white noise modes
- Bedtime scheduling (auto-off at bedtime)
- Visual feedback via LED indicators

Note: Scheduled alarms (FEAT_REQ_008) and sleep timer (FEAT_REQ_009) are now v1
requirements (ref DEC-021).

System-Level Requirements
-------------------------

These requirements describe system-wide behaviors that span multiple containers.

.. req:: Auto Source Select
   :id: SYS_REQ_001
   :status: open
   :tags: system, audio

   When a new audio source starts, the previous source shall stop playback automatically.

.. req:: Fade to Mute on Source Switch
   :id: SYS_REQ_002
   :status: open
   :tags: system, audio

   On source switch, audio shall fade to mute (short fade out) before switching to the new source.

.. req:: Fade In on New Source Playback
   :id: SYS_REQ_003
   :status: open
   :tags: system, audio

   When a new source starts playback, audio shall fade from mute to the previous volume level.

.. req:: Local-Network API
   :id: SYS_REQ_004
   :status: open
   :tags: system, api

   The system shall expose a local-network API for external control (e.g., Home Assistant).

.. req:: API Authentication
   :id: SYS_REQ_005
   :status: open
   :tags: system, security

   The API shall require authentication and be accessible only on the local network.

.. req:: Auto-Discovery
   :id: SYS_REQ_006
   :status: open
   :tags: system, discovery

   The system shall support auto-discovery where possible (mDNS, SSDP, etc.).

.. req:: mDNS Service Advertisement
   :id: SYS_REQ_007
   :status: open
   :tags: system, networking

   The system shall advertise its presence on the local network via mDNS using
   avahi-daemon, resolvable at kidsspeaker-XXXX.local (ref DEC-018).

.. req:: NTP Time Synchronization
   :id: SYS_REQ_008
   :status: open
   :tags: system, networking

   The system shall synchronise system time via NTP using systemd-timesyncd
   (ref DEC-019).

.. req:: zRAM Swap Configuration
   :id: SYS_REQ_009
   :status: open
   :tags: system, performance

   The system shall use zRAM (256 MB, lz4) as the sole swap device to extend
   effective memory on the Pi Zero W (ref DEC-017).

Project Progress
----------------

.. needtable::
   :columns: id;title;status;type;tags
   :sort_by: id
   :style: table

Unfulfilled Requirements
~~~~~~~~~~~~~~~~~~~~~~~~

Requirements with no specifications linked yet.

.. needlist::
   :status: open
   :filter: fulfills is None
   :sort_by: id

Fulfilled Requirements
~~~~~~~~~~~~~~~~~~~~~~

Requirements that have at least one specification linked.

.. needlist::
   :status: open
   :filter: fulfills is not None
   :sort_by: id
