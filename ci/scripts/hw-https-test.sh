#!/bin/bash
# hw-https-test.sh — HTTPS on a real Pi (WEB_REQ_001, DEC-032)
# ============================================================
# Usage: PI_HOST=root@host hw-https-test.sh
#
# Checks that the API answers over HTTPS with the device's own certificate,
# that the certificate names the device and stays the same across a restart,
# that its key is private and on /data, and that plain HTTP still answers for
# health checks. Connects from this host by the device's .local name, trusting
# only the certificate fetched from the device.

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
trap 'rm -rf "$WORK"' EXIT
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

host=$(remote cat /proc/sys/kernel/hostname)
fingerprint() { remote "openssl x509 -in /data/tls/cert.pem -noout -fingerprint -sha256"; }

log "=== Certificate"
check "the certificate is on /data" "$(remote 'test -s /data/tls/cert.pem && echo yes')" yes
check "the private key is readable by root only" "$(remote stat -c %a /data/tls/key.pem)" 600
check "it names the device ($host.local)" \
    "$(remote "openssl x509 -in /data/tls/cert.pem -noout -ext subjectAltName" | grep -c "DNS:$host.local")" 1
before=$(fingerprint)

log "=== HTTPS"
check "HTTPS answers on the device" "$(remote "curl -sk -o /dev/null -w '%{http_code}' https://localhost/health")" 200
"${SCP[@]}" "$PI_HOST:/data/tls/cert.pem" "$WORK/device.pem"
check "HTTPS answers from this host, verified by the device's certificate" \
    "$(curl -s --cacert "$WORK/device.pem" -o /dev/null -w '%{http_code}' "https://$host.local/health")" 200
check "plain HTTP still answers for health checks" "$(remote "curl -s -o /dev/null -w '%{http_code}' http://localhost:8080/health")" 200

log "=== The certificate survives a restart"
remote systemctl restart api-server
for _ in $(seq 20); do [ "$(remote "curl -sk -o /dev/null -w '%{http_code}' https://localhost/health")" = 200 ] && break; sleep 1; done
check "the same certificate after a restart" "$(fingerprint)" "$before"

if [ "$fail" = 0 ]; then log "All HTTPS tests passed."; else log "Some HTTPS tests FAILED."; fi
exit "$fail"
