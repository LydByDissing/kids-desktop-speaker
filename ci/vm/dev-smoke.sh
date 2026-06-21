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
if response=$(curl -sf "$BASE/api/system/time" 2>&1); then
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
if response=$(curl -sf "$BASE/api/audio/source" 2>&1); then
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
if response=$(curl -sf "$BASE/api/audio/volume-limit" 2>&1); then
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

# Test 5: POST /api/audio/volume-limit body '{"limit":50}' → expect HTTP 200
echo "==> Test 5: POST /api/audio/volume-limit"
if response=$(curl -sf -X POST "$BASE/api/audio/volume-limit" \
    -H "Content-Type: application/json" \
    -d '{"limit":50}' 2>&1); then
    echo "[PASS] audio/volume-limit POST"
    PASS=$((PASS + 1))
else
    echo "[FAIL] audio/volume-limit POST — $response"
    FAIL=$((FAIL + 1))
fi

echo ""
echo "Results: $PASS passed, $FAIL failed"

# Exit 0 only if all 5 pass
if [ $FAIL -eq 0 ] && [ $PASS -eq 5 ]; then
    echo "All smoke tests passed."
    exit 0
else
    exit 1
fi
