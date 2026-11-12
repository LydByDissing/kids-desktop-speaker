Data Model — Kids Desktop Speaker
===================================

.. meta::
   :status: draft
   :version: 0.1.0
   :author: architecture-session-2026-05-25

This document describes the data model for the Kids Desktop Speaker.
All persistent data is stored on the **data partition** (``/data``) as
config files. No database is used — the data model is expressed as
JSON and YAML files that survive OTA updates and rollbacks.

Design Goals
------------

1. **Config files only** — No SQLite, no database server. Plain JSON/YAML
   files that are easy to inspect, back up, and version control.
2. **Survives OTA** — All data is on the separate data partition, never
   touched by RAUC.
3. **Human-readable** — Files can be edited manually (for development/debug)
   or through the Web UI.
4. **Atomic writes** — Config changes use write-to-temp + rename to prevent
   corruption on power loss.

Data Partition Layout
---------------------

.. code-block:: text

   /data/
   ├── config/                  # Application configuration
   │   ├── alarms.json          # Alarm definitions
   │   ├── radio-stations.json  # Internet radio station URLs
   │   ├── podcast-feeds.json   # Podcast RSS feed URLs
   │   ├── dsp-profiles.json    # Custom DSP profile metadata
   │   └── system.json          # System settings (maintenance window, etc.)
   ├── auth/
   │   └── tokens.json          # API token hashes
   ├── spotify/
   │   └── credentials.json     # Spotify Connect credentials (librespot)
   ├── radio/
   │   └── stations.json        # Radio stations (DEC-048)
   ├── podcasts/
   │   ├── feeds.json           # Podcast feeds
   │   └── positions.json       # Where each episode got to
   ├── piper/
   │   └── models/              # Piper TTS voice models (.onnx + .json)
   ├── ota/
   │   └── staging/             # Downloaded RAUC bundles (before install)
   └── camilladsp/
       └── profiles/            # Custom CamillaDSP YAML profiles

Config File Formats
-------------------

System Settings (``/data/config/system.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "maintenance_window": {
       "start": "03:00",
       "end": "05:00",
       "timezone": "Europe/Copenhagen"
     },
     "default_tts_voice": "en_US-amy-low",
     "volume_limit": {
       "enabled": false,
       "max_percent": 80
     },
     "active_dsp_profile": "flat"
   }

Alarms (``/data/config/alarms.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "alarms": [
       {
         "id": "alarm_001",
         "name": "Wake up",
         "enabled": true,
         "time": "07:00",
         "days": ["mon", "tue", "wed", "thu", "fri"],
         "source": "radio",
         "source_id": "radio_001",
         "volume_percent": 50,
         "fade_in_secs": 30
       }
     ]
   }

Radio Stations (``/data/config/radio-stations.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "stations": [
       {
         "id": "radio_001",
         "name": "DR P3",
         "url": "https://live-ssl.dr.dk/dr-mas-hd-p3",
         "genre": "Pop"
       }
     ]
   }

Podcast Feeds (``/data/config/podcast-feeds.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "feeds": [
       {
         "id": "podcast_001",
         "name": "Planet Money",
         "url": "https://feeds.npr.org/510289/podcast.xml",
         "auto_download": false
       }
     ]
   }

DSP Profiles (``/data/config/dsp-profiles.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "profiles": [
       {
         "id": "flat",
         "name": "Flat",
         "built_in": true,
         "file": "/etc/camilladsp/profiles/flat.yml"
       },
       {
         "id": "custom_001",
         "name": "My Custom EQ",
         "built_in": false,
         "file": "/data/camilladsp/profiles/custom_001.yml"
       }
     ]
   }

API Tokens (``/data/auth/tokens.json``)
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

.. code-block:: json

   {
     "tokens": [
       {
         "id": "tok_abc123",
         "name": "Home Assistant",
         "hash": "sha256:a1b2c3d4...",
         "created_at": "2026-05-25T12:00:00Z",
         "last_used_at": "2026-05-25T13:00:00Z",
         "enabled": true
       }
     ]
   }

Atomic Write Pattern
--------------------

All config file updates use atomic writes to prevent corruption:

1. Write to temporary file: ``/data/config/alarms.json.tmp``
2. ``fsync()`` the temp file
3. ``rename()`` temp file to final path (atomic on ext4)

This ensures that a power loss mid-write never leaves a corrupted config file.

.. note::

   The ``rename()`` syscall is atomic on ext4 (the data partition filesystem).
   Either the old file or the new file exists, never a partial write.

Service-to-Config Mapping
-------------------------

.. list-table::
   :header-rows: 1
   :widths: 25 40 35

   * - Service
     - Reads
     - Writes
   * - API Server
     - All config files
     - Alarms, radio, podcasts, tokens, system settings
   * - Audio Player
     - Radio stations, podcast feeds (via MPD)
     - (none directly — MPD manages its own state)
   * - TTS Engine
     - System settings (default voice)
     - (none)
   * - OTA Updater
     - System settings (maintenance window)
     - (none)
   * - CamillaDSP
     - DSP profiles
     - (none — profile switching via symlink + reload)
   * - Bluetooth Service
     - (none)
     - (none — BlueZ manages paired devices)
   * - MPD
     - Radio stations, podcast feeds
     - Playlists, playback state

Open Questions
--------------

.. todo::

   - **MPD database format**: MPD uses its own binary database format, not
     JSON. This is fine — MPD manages it. But it means the data partition
     needs enough space for MPD's database + playlists. Estimate: <10MB
     for typical use.

   - **Config file validation**: Should the API Server validate config file
     schema on write? For v1, basic JSON validation is sufficient. Schema
     validation can be added later.

   - **Backup/restore**: Should there be a way to export/import all config
     as a single file? Useful for device migration. Low priority for v1.
