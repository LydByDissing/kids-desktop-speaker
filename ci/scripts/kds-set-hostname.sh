#!/bin/bash
set -euo pipefail
MAC=$(cat /sys/class/net/wlan0/address 2>/dev/null || cat /sys/class/net/eth0/address 2>/dev/null || echo "00:00:00:00:00:00")
SUFFIX=$(echo "$MAC" | tr -d ':' | tail -c 5)
HOSTNAME="kidsspeaker-${SUFFIX}"
echo "$HOSTNAME" > /etc/hostname
sed -i "s/127.0.1.1.*/127.0.1.1\t${HOSTNAME}/" /etc/hosts || echo "127.0.1.1\t${HOSTNAME}" >> /etc/hosts
hostnamectl set-hostname "$HOSTNAME" 2>/dev/null || true
