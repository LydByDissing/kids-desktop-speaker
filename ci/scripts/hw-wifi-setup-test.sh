#!/bin/bash
# hw-wifi-setup-test.sh — the Wi-Fi setup access point on a real Pi (NET_ONB_001/002, DEC-044)
# ==========================================================================================
# Usage: PI_HOST=root@host hw-wifi-setup-test.sh
#
# Opens the setup access point for 90 s. The Pi leaves the network meanwhile
# (its only radio becomes the access point) and must come back by itself; then
# the test reads the logs: the access point started (hostapd AP-ENABLED, the
# portal's address), it closed when its time ran out, normal Wi-Fi returned,
# and nothing was left behind. Joining the access point with a phone and using
# the setup page is a manual check. The speaker is muted unless KDS_HW_AUDIBLE=1.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
trap speaker_unmute EXIT
speaker_mute

log "=== Before"
check "wifi-setup is running" "$(remote systemctl is-active wifi-setup)" active
check "no access point" "$(remote systemctl is-active kds-setup-ap)" inactive
since=$(remote date +%s)

log "=== Open the setup access point for 90 s"
ip=$(remote "ip -4 -o addr show wlan0" | awk '{print $4}' | cut -d/ -f1)
remote "echo 90 > /run/kds/wifi-setup-request"
gone=no
for _ in $(seq 120); do
    ping -c1 -W1 "$ip" >/dev/null 2>&1 || { gone=yes; break; }
    sleep 1
done
check "the Pi left the network (its radio is the access point)" "$gone" yes
log "  waiting for it to come back by itself..."
back=no
for _ in $(seq 72); do
    sleep 5
    remote true && { back=yes; break; }
done
check "the Pi came back to the network by itself" "$back" yes
[ "$back" = yes ] || { log "Some Wi-Fi setup tests FAILED (the Pi is not back; power-cycle it)."; exit 1; }
speaker_mute

log "=== What happened"
journal=$(remote "journalctl --since @$since -o cat --no-pager")
check "it opened KidsSpeaker-2df5" "$(echo "$journal" | grep -c 'opening the setup access point KidsSpeaker-2df5')" 1
check "hostapd enabled the access point" "$(echo "$journal" | grep -c 'AP-ENABLED' | awk '{print ($1>0)?"yes":"no"}')" yes
check "it announced how to set up" "$(remote "journalctl -u audio-player --since @$since -o cat --no-pager" | grep -c 'announced (Da' | awk '{print ($1>0)?"yes":"no"}')" yes
check "it closed when its time ran out" "$(echo "$journal" | grep -c 'the access point closed; trying the saved network')" 1
check "the access point is gone" "$(remote systemctl is-active kds-setup-ap)" inactive
check "and cleared, so it can open again" "$(remote "systemctl list-units --all --no-legend 'kds-setup-ap*'" | grep -c .)" 0
check "the portal's network file is gone" "$(remote 'test -e /run/systemd/network/10-kds-setup-ap.network && echo left || echo gone')" gone
check "wlan0 is back on DHCP" "$(remote "ip -4 addr show wlan0" | grep -c '192.168.4.1')" 0
check "wpa_supplicant runs again" "$(remote systemctl is-active wpa_supplicant@wlan0)" active

log "=== It opens again"
remote "echo 30 > /run/kds/wifi-setup-request"
gone=no
for _ in $(seq 120); do
    ping -c1 -W1 "$ip" >/dev/null 2>&1 || { gone=yes; break; }
    sleep 1
done
check "a second access point opens" "$gone" yes
for _ in $(seq 72); do sleep 5; remote true && break; done
check "and the Pi comes back again" "$(remote true && echo yes)" yes

if [ "$fail" = 0 ]; then log "All Wi-Fi setup tests passed."; else log "Some Wi-Fi setup tests FAILED."; fi
exit "$fail"
