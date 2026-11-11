Wi-Fi Setup
===========

Gets a speaker onto the home Wi-Fi without a screen or an app (NET_ONB_001/002,
DEC-012, DEC-044). A small Rust service, ``kds-wifi-setup``.

- **When**: with no saved network it opens the setup access point at once and
  keeps it open. With a saved network that fails 5 checks in a row (30 s apart,
  after 2 minutes' grace at boot) it opens it for 10 minutes, then tries the
  network again, alternating until one works. Saved networks are never deleted.
- **The access point**: ``KidsSpeaker-xxxx``, open, channel 6, address
  192.168.4.1. A transient systemd unit (``kds-setup-ap``, started with
  ``systemd-run``) runs hostapd and dnsmasq; its ``RuntimeMaxSec`` and
  ``ExecStopPost`` bring normal Wi-Fi back even if the service dies. A runtime
  networkd file gives wlan0 the fixed address while it runs.
- **The portal**: every DNS name points at 192.168.4.1 and every path shows the
  setup page, so phones open it by themselves. It lists the networks seen before
  the access point opened. Saving writes ``/boot/firmware/kds/wifi.conf`` (as
  ``setup-sd.sh`` does) with the network name hex-encoded and only the WPA key,
  never the password.
- **Feedback**: the speaker announces "Forbind til netværket KidsSpeaker-xxxx
  for at sætte højttaleren op".

C4 Components
-------------

.. plantuml::
   :caption: Components — Wi-Fi Setup

   @startuml
   !include ../../c4/C4_Component.puml
   LAYOUT_TOP_DOWN()

   Container_Boundary(ws, "Wi-Fi Setup") {
       Component(mon, "Monitor", "monitor.rs", "Checks every 30 s: saved network? connected? Decides when to open the access point")
       Component(portal, "Captive portal", "portal.rs (axum)", "Setup page on 192.168.4.1:80")
       Component(wpa, "WPA config", "wpa.rs", "Hex SSID, PBKDF2 key; validation")
   }
   System_Ext(ap, "kds-setup-ap", "systemd-run: hostapd, dnsmasq, networkd file")
   System_Ext(boot, "Boot partition", "kds/wifi.conf")
   Container_Ext(audio, "Audio Player", "audio.sock")
   Rel(mon, ap, "start, stop")
   Rel(mon, audio, "announce the setup network")
   Rel(portal, wpa, "build the config")
   Rel(portal, boot, "save the network")
   @enduml
