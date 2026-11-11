#!/bin/bash
# hw-web-test.sh — the Web UI on a real Pi (WEB_REQ_001, DEC-045)
# ==============================================================
# Usage: PI_HOST=root@host hw-web-test.sh
#
# Checks that api-server serves the Web UI over HTTPS with its security
# headers, that the app's own paths get the page while unknown /api paths stay
# a JSON 404, and that the page needs no token while the API does. If a
# headless Chromium is available (KDS_CHROME, or Playwright's), it also opens
# the page with an admin token and a child's token, checks the tabs each one
# sees, and saves screenshots in $KDS_WEB_SHOTS (default: a temporary folder).

set -uo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
source "$REPO/ci/scripts/hw-lib.sh"
remote true || { echo "error: cannot reach $PI_HOST" >&2; exit 2; }

host=$(remote cat /proc/sys/kernel/hostname)
base="https://$host.local"
"${SCP[@]}" "$PI_HOST:/data/tls/cert.pem" "$WORK/device.pem"
fetch() { curl -s --cacert "$WORK/device.pem" "$@"; }
header() { fetch -o /dev/null -D - "$base$1" | tr -d '\r' | sed -n "s/^$2: //Ip"; }

admin_id=; child_id=
cleanup() {
    [ -n "$admin_id" ] && api_revoke "$admin_id"
    [ -n "$child_id" ] && api_revoke "$child_id"
    rm -rf "$WORK"
}
trap cleanup EXIT

log "=== The page"
check "the page is installed" "$(remote 'test -s /usr/share/kds/web/index.html && echo yes')" yes
check "/ answers 200 without a token" "$(fetch -o /dev/null -w '%{http_code}' "$base/")" 200
check "it is the app" "$(fetch "$base/" | grep -c 'id="app"')" 1
check "only the speaker's own scripts may run" "$(header / content-security-policy | grep -c "script-src 'self';")" 1
check "no framing" "$(header / content-security-policy | grep -c "frame-ancestors 'none'")" 1
check "nosniff" "$(header / x-content-type-options)" nosniff
check "no referrer" "$(header / referrer-policy)" no-referrer
script=$(fetch "$base/" | sed -n 's/.*src="\(\/assets\/[^"]*\.js\)".*/\1/p' | head -1)
check "its script loads" "$(fetch -o /dev/null -w '%{http_code}' "$base$script")" 200
check "as JavaScript" "$(header "$script" content-type | grep -c javascript)" 1

log "=== Paths"
check "the app's own paths get the page" "$(fetch -o /dev/null -w '%{http_code}' "$base/settings/access")" 200
check "unknown /api paths are a JSON 404" "$(fetch -w ' %{http_code}' "$base/api/nothing-here")" '{"error":"not found"} 404'
check "the API still needs a token" "$(fetch -o /dev/null -w '%{http_code}' "$base/api/audio/source")" 401
check "health tells the app the speaker is set up" \
    "$(fetch "$base/health" | python3 -c 'import json,sys; print(json.load(sys.stdin)["set_up"])')" True

chrome=${KDS_CHROME:-$(ls -d "$HOME"/.cache/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-linux64/chrome-headless-shell 2>/dev/null | tail -1)}
if [ -x "$chrome" ]; then
    log "=== In a browser"
    shots=${KDS_WEB_SHOTS:-$WORK/shots}; mkdir -p "$shots"
    read -r admin_id admin_token < <(api_token "hw-web-test admin" admin)
    read -r child_id child_token < <(api_token "hw-web-test child")
    # The certificate is self-signed for the device; Chromium is told to accept it.
    browse() { # token what -> the rendered page
        "$chrome" --headless --no-sandbox --ignore-certificate-errors --user-data-dir="$WORK/profile-$2" \
            --window-size=420,860 --virtual-time-budget=8000 "$3" "$base/#token=$1" 2>/dev/null
    }
    dom=$(browse "$admin_token" admin --dump-dom)
    check "an admin sees four tabs" "$(grep -o 'class="tab[ "]' <<<"$dom" | wc -l)" 4
    check "the player shows the volume" "$(grep -c 'type="range"' <<<"$dom")" 1
    dom=$(browse "$child_token" child --dump-dom)
    check "a child sees two tabs" "$(grep -o 'class="tab[ "]' <<<"$dom" | wc -l)" 2
    browse "$admin_token" admin2 --screenshot="$shots/admin.png" >/dev/null
    browse "$child_token" child2 --screenshot="$shots/child.png" >/dev/null
    check "screenshots saved" "$(ls "$shots"/admin.png "$shots"/child.png 2>/dev/null | wc -l | tr -d ' ')" 2
    [ -n "${KDS_WEB_SHOTS:-}" ] && log "  screenshots in $shots"
else
    log "=== In a browser: skipped (no headless Chromium; set KDS_CHROME)"
fi

if [ "$fail" = 0 ]; then log "All Web UI tests passed."; else log "Some Web UI tests FAILED."; fi
exit "$fail"
