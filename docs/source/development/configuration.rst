Configuration
=============

The Kids Desktop Speaker is configured via a YAML file and environment variables.

Configuration File
------------------

The main configuration file is ``config.yaml`` in the project root:

.. code-block:: yaml

   # config.yaml — Kids Desktop Speaker

   # Device settings
   device:
     name: "Kids Speaker"
     hostname: "kids-speaker"

   # Audio settings
   audio:
     output_device: "hw:0,0"
     sample_rate: 44100
     buffer_size: 1024
     default_volume: 50
     max_volume: 80  # Parental control: max volume limit

   # DSP settings
   dsp:
     enabled: true
     config_path: "/etc/camilladsp/config.yml"
     # CamillaDSP configuration for bass enhancement
     bass_boost: 6  # dB

   # Web server
   server:
     host: "0.0.0.0"
     port: 8000
     cors_origins:
       - "http://localhost:3000"

   # Bluetooth
   bluetooth:
     enabled: true
     discoverable: true
     pin: "0000"

   # TTS
   tts:
     engine: "piper"
     model: "en_US-lessac-medium"
     speed: 1.0

   # Parental controls
   parental:
     enabled: true
     max_volume: 70
     bedtime_hour: 20  # 8 PM — auto-mute
     time_limit_minutes: 120

   # Logging
   logging:
     level: "INFO"
     file: "/var/log/kids-speaker/app.log"

Environment Variables
---------------------

.. list-table::
   :header-rows: 1
   :widths: 35 65

   * - Variable
     - Description
   * - ``KIDS_SPEAKER_CONFIG``
     - Path to configuration file (default: ``config.yaml``)
   * - ``KIDS_SPEAKER_LOG_LEVEL``
     - Override log level (e.g., ``DEBUG``)
   * - ``KIDS_SPEAKER_PORT``
     - Override server port
   * - ``SPOTIFY_USERNAME``
     - Spotify account username
   * - ``SPOTIFY_PASSWORD``
     - Spotify account password
   * - ``SPOTIFY_CLIENT_ID``
     - Spotify API client ID
   * - ``SPOTIFY_CLIENT_SECRET``
     - Spotify API client secret

CamillaDSP Configuration
------------------------

The DSP pipeline is configured separately in CamillaDSP's own YAML format:

.. code-block:: yaml

   # /etc/camilladsp/config.yml
   devices:
     samplerate: 44100
     chunksize: 1024
     capture:
       type: Alsa
       device: "hw:0,0"
       channels: 2
     playback:
       type: Alsa
       device: "hw:0,0"
       channels: 2

   filters:
     bass_boost:
       type: Biquad
       parameters:
         type: Lowshelf
         freq: 100
         gain: 6.0
         q: 0.7

   pipeline:
     - type: Filter
       channel: 0
       names: [bass_boost]
     - type: Filter
       channel: 1
       names: [bass_boost]
