System Context
==============

Overview
--------
The system context diagram provides a high-level view of the Kids Desktop Speaker platform and its interactions with external systems. This diagram helps to understand the boundaries of the system and how it interacts with the outside world.

External Systems
----------------

1. **User Devices**
   - Mobile devices (phones, tablets) for controlling the speaker via native apps.
   - Web browsers for controlling the speaker via a web interface.

2. **Bluetooth Devices**
   - Devices that connect to the speaker via Bluetooth for control (e.g., volume, playback).

3. **Cloud Services**
   - **Spotify**: For streaming music.
   - **Internet Radio Services**: For streaming radio.
   - **Podcast Services**: For streaming podcasts.

4. **Home Assistant**
   - For integrating the speaker into smart home setups.

5. **Wi-Fi Network**
   - For connectivity and communication between the speaker and other devices/services.

6. **Offline TTS Engine**
   - For converting text messages to speech for custom announcements.

7. **GitHub**
   - For hosting firmware updates and releases.

Diagram
-------

.. image:: ../images/system_context.png
   :alt: System Context Diagram
   :align: center

Description
-----------

The Kids Desktop Speaker platform interacts with various external systems to provide a seamless audio experience. The system is controlled by user devices through native apps or a web interface. Bluetooth devices can also control the speaker for basic functions like volume and playback.

The speaker streams audio from cloud services such as Spotify, internet radio, and podcast services. It integrates with Home Assistant for smart home automation and uses an offline TTS engine for custom announcements.

Firmware updates are hosted on GitHub and can be delivered to the speaker over the Wi-Fi network.

Internal Components
-------------------

- **CamillaDSP**: An internal container within the system for enhancing sound quality, particularly in the bass region.

Next Steps
----------

- Create a container diagram to detail the internal components of the system.
- Create component diagrams to detail the interactions within each container.
- Create code diagrams to detail the implementation of key components.