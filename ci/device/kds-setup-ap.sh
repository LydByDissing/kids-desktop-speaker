#!/bin/bash
# kds-setup-ap.sh — the Wi-Fi setup access point (NET_ONB_001, DEC-044)
# Usage: kds-setup-ap.sh run <ssid>   (as the ExecStart of a transient unit)
#        kds-setup-ap.sh stop         (its ExecStopPost: back to normal Wi-Fi)
# kds-wifi-setup starts it with systemd-run, bounded by RuntimeMaxSec, so the
# speaker returns to its network even if kds-wifi-setup dies.
set -u
NETWORK=/run/systemd/network/10-kds-setup-ap.network
RUN=/run/kds/setup-ap

case "${1:-}" in
run)
    ssid=${2:?ssid}
    mkdir -p "$RUN" /run/systemd/network
    systemctl stop wpa_supplicant@wlan0.service
    # networkd gives wlan0 the portal's address instead of running DHCP.
    printf '[Match]\nName=wlan0\n\n[Network]\nAddress=192.168.4.1/24\nDHCP=no\n' > "$NETWORK"
    networkctl reload
    networkctl reconfigure wlan0
    cat > "$RUN/hostapd.conf" <<CONF
interface=wlan0
driver=nl80211
ssid=$ssid
country_code=DK
hw_mode=g
channel=6
auth_algs=1
wpa=0
CONF
    # DHCP for phones, and every name answered with the portal, so phones
    # open the setup page by themselves.
    cat > "$RUN/dnsmasq.conf" <<CONF
interface=wlan0
bind-interfaces
listen-address=192.168.4.1
dhcp-range=192.168.4.10,192.168.4.60,10m
dhcp-option=option:router,192.168.4.1
address=/#/192.168.4.1
no-resolv
pid-file=$RUN/dnsmasq.pid
CONF
    dnsmasq --conf-file="$RUN/dnsmasq.conf"
    exec hostapd "$RUN/hostapd.conf"
    ;;
stop)
    [ -f "$RUN/dnsmasq.pid" ] && kill "$(cat "$RUN/dnsmasq.pid")" 2>/dev/null
    rm -f "$NETWORK"
    networkctl reload
    networkctl reconfigure wlan0
    systemctl start wpa_supplicant@wlan0.service
    rm -rf "$RUN"
    ;;
*)
    echo "usage: $0 run <ssid> | stop" >&2
    exit 2
    ;;
esac
