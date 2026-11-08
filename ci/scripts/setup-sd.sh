#!/bin/bash
# setup-sd.sh — add Wi-Fi and SSH access to a freshly flashed speaker SD card
# ===========================================================================
# Usage: setup-sd.sh [--boot DIR] [--key FILE] [--ssid NAME] [--country CC]
#
# Run on your computer after flashing kds-sdcard.img.xz, with the card inserted.
# Writes kds/authorized_keys and kds/wifi.conf to the card's KDS-BOOT partition,
# which the speaker applies at every boot (DEC-025). Asks for anything not given.
# The Wi-Fi password can also come from KDS_WIFI_PSK. Nothing is written to the
# image or the repository.

set -euo pipefail

BOOT="" KEY="" SSID="" COUNTRY=""
while [ "$#" -gt 0 ]; do
    case "$1" in
        --boot) BOOT="$2"; shift 2 ;;
        --key) KEY="$2"; shift 2 ;;
        --ssid) SSID="$2"; shift 2 ;;
        --country) COUNTRY="$2"; shift 2 ;;
        -h|--help) sed -n 2,11p "$0"; exit 0 ;;
        *) echo "error: unknown option $1" >&2; exit 2 ;;
    esac
done

die() { echo "error: $*" >&2; exit 1; }

# Find the card's boot partition, unless given.
if [ -z "$BOOT" ]; then
    for dir in "/media/$USER/KDS-BOOT" "/run/media/$USER/KDS-BOOT" /media/KDS-BOOT; do
        [ -d "$dir" ] && BOOT="$dir" && break
    done
    [ -n "$BOOT" ] || die "KDS-BOOT not found. Insert the card, or give --boot DIR."
fi
# Only ever write to our boot partition: it has U-Boot and the A/B boot script.
[ -f "$BOOT/boot.scr" ] && [ -f "$BOOT/u-boot.bin" ] \
    || die "$BOOT doesn't look like a speaker boot partition (no boot.scr or u-boot.bin)"

# SSH key: the first public key found, unless given.
if [ -z "$KEY" ]; then
    for k in "$HOME/.ssh/id_ed25519.pub" "$HOME/.ssh/id_ecdsa.pub" "$HOME/.ssh/id_rsa.pub"; do
        [ -f "$k" ] && KEY="$k" && break
    done
    [ -n "$KEY" ] || die "no SSH public key found. Create one with: ssh-keygen -t ed25519"
fi
[ -f "$KEY" ] || die "key file not found: $KEY"
case "$KEY" in *.pub) ;; *) die "$KEY is not a public key (.pub). Never copy a private key." ;; esac

# Wi-Fi details.
[ -n "$SSID" ] || read -r -p "Wi-Fi network name: " SSID
[ -n "$SSID" ] || die "no network name given"
PSK="${KDS_WIFI_PSK:-}"
if [ -z "$PSK" ]; then
    read -r -s -p "Wi-Fi password: " PSK; echo
fi
[ "${#PSK}" -ge 8 ] && [ "${#PSK}" -le 63 ] || die "Wi-Fi passwords are 8 to 63 characters"
[ -n "$COUNTRY" ] || read -r -p "Country code [DK]: " COUNTRY
COUNTRY=$(echo "${COUNTRY:-DK}" | tr '[:lower:]' '[:upper:]')
[[ "$COUNTRY" =~ ^[A-Z]{2}$ ]] || die "country must be a two-letter code, such as DK"
case "$SSID$PSK" in *'"'*|*\\*) [ -n "$(command -v wpa_passphrase)" ] \
    || die "quotes or backslashes in the network name or password need wpa_passphrase (package wpasupplicant)" ;; esac

# A hashed key keeps the plain password off the card, when wpa_passphrase exists.
if command -v wpa_passphrase >/dev/null; then
    NETWORK=$(wpa_passphrase "$SSID" "$PSK" | grep -v '^\s*#psk=')
else
    NETWORK=$(printf 'network={\n\tssid="%s"\n\tpsk="%s"\n}' "$SSID" "$PSK")
fi

mkdir -p "$BOOT/kds"
cp "$KEY" "$BOOT/kds/authorized_keys"
{
    echo "ctrl_interface=DIR=/run/wpa_supplicant"
    echo "country=$COUNTRY"
    echo "$NETWORK"
} > "$BOOT/kds/wifi.conf"
sync

echo "Card ready:"
echo "  SSH key:  $(cut -d' ' -f1,3 "$KEY") -> kds/authorized_keys"
echo "  Wi-Fi:    $SSID ($COUNTRY)$(command -v wpa_passphrase >/dev/null && echo ', password stored hashed')"
echo "Eject the card, boot the speaker, then: ssh root@kidsspeaker-XXXX.local"
