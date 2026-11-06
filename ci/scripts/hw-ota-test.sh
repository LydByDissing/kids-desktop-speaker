#!/bin/bash
# hw-ota-test.sh — prove the OTA updater end to end on a real Pi Zero W (KDS-3zt)
# ==============================================================================
# Usage: PI_HOST=root@kidsspeaker-XXXX.local hw-ota-test.sh
#
# Serves a stand-in for the GitHub Releases API from this machine, reached by the
# Pi through a reverse SSH tunnel, so no firewall changes are needed. The updater
# on the Pi then finds the release, download it, verify it and install it. A
# tampered manifest must be rejected first. The updater is pointed at this server
# with a runtime drop-in that disappears on reboot. Bundles use a throwaway key
# trusted only by the device under test, so reflash it afterwards.
#
# The Pi first gets the build under test installed, so the checks exercise this
# build's updater rather than whatever the device was running.

set -uo pipefail
: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK="$REPO/ci/output/otatest"
PORT=8765
VERSION=v0.5.0-test
source "$REPO/ci/scripts/hw-lib.sh"

SERVER_PID=
TUNNEL_PID=
cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
    [ -n "$TUNNEL_PID" ] && kill "$TUNNEL_PID" 2>/dev/null
    hw_cleanup
}
trap cleanup EXIT

serve() { # sha256-to-advertise
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null && wait "$SERVER_PID" 2>/dev/null
    python3 - "$WORK/good/image.raucb" "$1" "$HOST_IP" "$PORT" "$VERSION" <<'PY' 2>>"$WORK/server.log" &
import http.server, json, os, sys
bundle, sha, host, port, version = sys.argv[1:6]
base = f"http://{host}:{port}"
release = {"tag_name": version, "assets": [
    {"name": "manifest.json", "browser_download_url": f"{base}/manifest.json"},
    {"name": "image.raucb", "browser_download_url": f"{base}/image.raucb"}]}
manifest = {"version": version, "bundle": "image.raucb", "sha256": sha, "size": os.path.getsize(bundle)}
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/repos/test/kds/releases/latest":
            body, kind = json.dumps(release).encode(), "application/json"
        elif self.path == "/manifest.json":
            body, kind = json.dumps(manifest).encode(), "application/json"
        elif self.path == "/image.raucb":
            self.send_response(200)
            self.send_header("Content-Length", str(os.path.getsize(bundle)))
            self.end_headers()
            with open(bundle, "rb") as f:
                while chunk := f.read(1 << 20):
                    self.wfile.write(chunk)
            return
        else:
            self.send_error(404); return
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *a):
        sys.stderr.write("server: " + (a[0] % a[1:]) + "\n")
http.server.ThreadingHTTPServer(("127.0.0.1", int(port)), Handler).serve_forever()
PY
    SERVER_PID=$!
    sleep 1
}

point_updater_here() {
    remote "mkdir -p /run/systemd/system/ota-updater.service.d && printf '[Service]\nEnvironment=OTA_API_BASE=http://$HOST_IP:$PORT OTA_REPO_OWNER=test OTA_REPO_NAME=kds OTA_CHECK_INTERVAL_SECS=20\n' > /run/systemd/system/ota-updater.service.d/test.conf && systemctl daemon-reload && systemctl restart ota-updater"
}

ota_log() { remote "journalctl -u ota-updater -b -o cat --no-pager --since '-$1 s'"; }

remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }
# The Pi reaches the server as its own localhost, through the tunnel.
HOST_IP=127.0.0.1
cleanup; mkdir -p "$WORK"
log "=== Setup"

make_test_identity || exit 1
make_bundle base v0.4.0-test none || exit 1
make_bundle good "$VERSION" none || exit 1
trust_test_identity
REAL_SHA=$(sha256sum "$WORK/good/image.raucb" | cut -d' ' -f1)

log "=== Installing the build under test"
"${SCP[@]}" "$WORK/base/image.raucb" "$PI_HOST:/data/ota/base.raucb" || exit 1
remote 'rauc install /data/ota/base.raucb >/tmp/install.log 2>&1; rc=$?; rm -f /data/ota/base.raucb; exit $rc' \
    || { log "install failed"; remote tail -5 /tmp/install.log; exit 1; }
BASE_SLOT=$([ "$(slot)" = A ] && echo B || echo A)
id=$(boot_id); reboot_pi
wait_for_reboot "$id" 420 || { log "Pi did not come back"; exit 1; }
wait_settled "$BASE_SLOT" 300 || { log "did not settle on slot $BASE_SLOT"; exit 1; }
check "runs the build under test" "$(remote cat /etc/kds-version)" v0.4.0-test
# The test identity has to be trusted by the newly booted slot too.
trust_test_identity
# Open the tunnel only now: a reboot drops it.
ssh -N -o BatchMode=yes -o ExitOnForwardFailure=yes -o StrictHostKeyChecking=accept-new \
    -o ServerAliveInterval=15 -R "$PORT:127.0.0.1:$PORT" "$PI_HOST" &
TUNNEL_PID=$!
sleep 3
START=$(slot); START_VERSION=$(remote cat /etc/kds-version)
summary

log "=== Test 1: tampered manifest is rejected"
serve "$(echo tampered | sha256sum | cut -d' ' -f1)"
point_updater_here
for _ in $(seq 1 40); do ota_log 600 | grep -q 'checksum mismatch' && break; sleep 15; done
check "updater reports a checksum mismatch" "$(ota_log 600 | grep -c 'checksum mismatch' | sed 's/[1-9][0-9]*/yes/')" yes
check "nothing was installed" "$(remote "rauc status 2>/dev/null | grep -c '$VERSION'")" 0
check "partial download was removed" "$(remote 'ls /data/ota/staging/ 2>/dev/null | wc -l')" 0
# Checks run every 20 s here. A bad release must not be downloaded again (KDS-6pb).
sleep 90
check "bad release is not downloaded again" "$(grep -c 'GET /image.raucb' "$WORK/server.log")" 1
check "updater says it is skipping the bad release" "$(ota_log 120 | grep -c 'failed verification earlier' | sed 's/[1-9][0-9]*/yes/')" yes

log "=== Test 2: valid release is installed"
serve "$REAL_SHA"
for _ in $(seq 1 60); do ota_log 900 | grep -q "installed $VERSION" && break; sleep 15; done
ota_log 900 | grep -E 'ota:' | tail -4 | cut -c1-140
check "updater installed the release" "$(ota_log 900 | grep -c "installed $VERSION" | sed 's/[1-9][0-9]*/yes/')" yes
check "running version is unchanged until reboot" "$(remote cat /etc/kds-version)" "$START_VERSION"
id=$(boot_id); reboot_pi
wait_for_reboot "$id" 420 || log "  Pi did not come back"
OTHER=$([ "$START" = A ] && echo B || echo A)
wait_settled "$OTHER" 300 || log "  did not settle on slot $OTHER"
summary
check "booted the updated slot" "$(slot)" "$OTHER"
check "runs the released version" "$(remote cat /etc/kds-version)" "$VERSION"
check "services are healthy" "$(remote systemctl is-system-running)" running

if [ "$fail" -ne 0 ]; then log "OTA tests FAILED"; exit 1; fi
log "All OTA tests passed."
