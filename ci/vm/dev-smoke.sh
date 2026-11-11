#!/bin/bash
set -euo pipefail
command -v jq >/dev/null 2>&1 || { echo "ERROR: jq not found"; exit 2; }
BASE="${KDS_BASE_URL:-http://localhost:8080}"
TIMEOUT="${KDS_TIMEOUT:-60}"

# Track pass/fail counts
PASS=0
FAIL=0

echo "==> Waiting for api-server at $BASE (timeout ${TIMEOUT}s)..."
deadline=$((SECONDS + TIMEOUT))
until curl -sf "$BASE/health" >/dev/null 2>&1; do
    if [ $SECONDS -ge $deadline ]; then
        echo "[FAIL] health — api-server did not respond within ${TIMEOUT}s"
        exit 1
    fi
    sleep 2
done

# Every API call except /health needs a token (AUTH_REQ_001). Requests from the
# VM itself need none, so the token is created inside the VM over SSH.
SCRIPT_DIR=$(cd "$(dirname "$0")" && pwd)
if [ -z "${KDS_TOKEN:-}" ] && [ -f "$SCRIPT_DIR/ci_key" ]; then
    echo "==> Creating an API token inside the VM..."
    deadline=$((SECONDS + ${KDS_TOKEN_TIMEOUT:-180}))
    until [ -n "${KDS_TOKEN:-}" ] || [ $SECONDS -ge $deadline ]; do
        KDS_TOKEN=$(timeout 20 ssh -i "$SCRIPT_DIR/ci_key" -p "${KDS_VM_SSH_PORT:-2222}" -o BatchMode=yes \
            -o ConnectTimeout=5 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR \
            root@localhost "curl -s -X POST -H 'content-type: application/json' -d '{\"name\": \"smoke test\", \"admin\": true}' \
            http://127.0.0.1:8080/api/tokens" 2>/dev/null | jq -r '.token // empty') || true
        [ -n "${KDS_TOKEN:-}" ] || sleep 5
    done
fi
[ -n "${KDS_TOKEN:-}" ] || { echo "[FAIL] no API token (set KDS_TOKEN or provide ci/vm/ci_key)"; exit 1; }
AUTH=(-H "Authorization: Bearer $KDS_TOKEN")

echo "==> Test: a request without a token is refused"
code=$(curl -s -o /dev/null -w '%{http_code}' "$BASE/api/system/time")
if [ "$code" = 401 ]; then
    echo "[PASS] no token: 401"
    PASS=$((PASS + 1))
else
    echo "[FAIL] no token: got $code, want 401"
    FAIL=$((FAIL + 1))
fi

# Test 1: GET /health → expect JSON with .status == "ok"
echo "==> Test 1: GET /health"
if response=$(curl -sf "$BASE/health" 2>&1); then
    if echo "$response" | jq -e '.status == "ok"' >/dev/null 2>&1; then
        echo "[PASS] health"
        PASS=$((PASS + 1))
    else
        echo "[FAIL] health — $(echo "$response" | jq .)"
        FAIL=$((FAIL + 1))
    fi
else
    echo "[FAIL] health — $response"
    FAIL=$((FAIL + 1))
fi

# Test 2: GET /api/system/time → expect JSON with .time field present
echo "==> Test 2: GET /api/system/time"
if response=$(curl -sf "${AUTH[@]}" "$BASE/api/system/time" 2>&1); then
    if echo "$response" | jq -e '.time' >/dev/null 2>&1; then
        echo "[PASS] system/time"
        PASS=$((PASS + 1))
    else
        echo "[FAIL] system/time — $(echo "$response" | jq .)"
        FAIL=$((FAIL + 1))
    fi
else
    echo "[FAIL] system/time — $response"
    FAIL=$((FAIL + 1))
fi

# Test 3: GET /api/audio/source → expect JSON with .active field present
echo "==> Test 3: GET /api/audio/source"
if response=$(curl -sf "${AUTH[@]}" "$BASE/api/audio/source" 2>&1); then
    if echo "$response" | jq -e 'has("active")' >/dev/null 2>&1; then
        echo "[PASS] audio/source"
        PASS=$((PASS + 1))
    else
        echo "[FAIL] audio/source — $(echo "$response" | jq .)"
        FAIL=$((FAIL + 1))
    fi
else
    echo "[FAIL] audio/source — $response"
    FAIL=$((FAIL + 1))
fi

# Test 4: GET /api/audio/volume-limit → expect JSON with .limit == 75
echo "==> Test 4: GET /api/audio/volume-limit"
if response=$(curl -sf "${AUTH[@]}" "$BASE/api/audio/volume-limit" 2>&1); then
    if echo "$response" | jq -e '.limit == 75' >/dev/null 2>&1; then
        echo "[PASS] audio/volume-limit"
        PASS=$((PASS + 1))
    else
        echo "[FAIL] audio/volume-limit — $(echo "$response" | jq .)"
        FAIL=$((FAIL + 1))
    fi
else
    echo "[FAIL] audio/volume-limit — $response"
    FAIL=$((FAIL + 1))
fi

# Test 5: the volume limit is a parental control: even an admin token may only
# change it over HTTPS (VOL_REQ_001), and the VM forwards plain HTTP only.
echo "==> Test 5: POST /api/audio/volume-limit over plain HTTP is refused"
code=$(curl -s -o /dev/null -w '%{http_code}' "${AUTH[@]}" -X POST "$BASE/api/audio/volume-limit" \
    -H "Content-Type: application/json" -d '{"limit":50}')
if [ "$code" = 403 ]; then
    echo "[PASS] audio/volume-limit POST over HTTP: 403"
    PASS=$((PASS + 1))
else
    echo "[FAIL] audio/volume-limit POST over HTTP: got $code, want 403"
    FAIL=$((FAIL + 1))
fi

# Test 5b: the device volume can be set with any token.
echo "==> Test 5b: POST /api/audio/volume"
if response=$(curl -sf "${AUTH[@]}" -X POST "$BASE/api/audio/volume" \
    -H "Content-Type: application/json" -d '{"volume":30}' 2>&1) \
    && echo "$response" | jq -e '.volume == 30' >/dev/null 2>&1; then
    echo "[PASS] audio/volume POST"
    PASS=$((PASS + 1))
else
    echo "[FAIL] audio/volume POST — $response"
    FAIL=$((FAIL + 1))
fi

echo ""
# Tests 6 and 7: live state from the backend services over IPC (IPC_REQ_004)
check_live() { # name path jq-filter
    echo "==> Test: GET $2"
    if response=$(curl -sf "${AUTH[@]}" "$BASE$2" 2>&1) && echo "$response" | jq -e "$3" >/dev/null 2>&1; then
        echo "[PASS] $1"
        PASS=$((PASS + 1))
    else
        echo "[FAIL] $1 — $response"
        FAIL=$((FAIL + 1))
    fi
}
check_live "ota/status from ota-updater" /api/ota/status 'has("state") and has("version")'
check_live "bluetooth/status from bluetooth-service" /api/bluetooth/status 'has("connected")'

echo ""
echo "Results: $PASS passed, $FAIL failed"

# Pass only if every test ran and none failed
if [ $FAIL -eq 0 ] && [ $PASS -gt 0 ]; then
    echo "All smoke tests passed."
    exit 0
else
    exit 1
fi
