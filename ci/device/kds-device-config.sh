#!/bin/bash
# kds-device-config.sh — apply per-device settings at boot (DEC-025)
# Settings live outside the root slots, so they survive A/B updates. Wi-Fi details
# and SSH keys come from the boot partition, never from the image.
set -u

BOOT=/boot/firmware/kds
DATA=/data

mkdir -p "$DATA/ota/staging" 2>/dev/null

if [ -f "$BOOT/wifi.conf" ]; then
    install -m 600 "$BOOT/wifi.conf" /etc/wpa_supplicant/wpa_supplicant-wlan0.conf
    rfkill unblock wifi 2>/dev/null
fi

if [ -f "$BOOT/authorized_keys" ]; then
    install -d -m 700 /root/.ssh
    install -m 600 "$BOOT/authorized_keys" /root/.ssh/authorized_keys
fi

# Host keys stay on /data so they are unique per device and survive updates.
if [ -d "$DATA" ] && mountpoint -q "$DATA"; then
    if ! ls "$DATA"/ssh/ssh_host_*_key >/dev/null 2>&1; then
        install -d -m 700 "$DATA/ssh"
        ssh-keygen -q -t ed25519 -N '' -f "$DATA/ssh/ssh_host_ed25519_key"
    fi
    cp -p "$DATA"/ssh/ssh_host_* /etc/ssh/
else
    ssh-keygen -A
fi
