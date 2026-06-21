#!/bin/bash
set -euo pipefail
BASE="${KDS_BASE_URL:-http://localhost:8080}"
TIMEOUT="${KDS_TIMEOUT:-30}"

echo "==> Waiting for api-server at $BASE (timeout ${TIMEOUT}s)..."
deadline=$((SECONDS + TIMEOUT))
until curl -sf "$BASE/health" >/dev/null 2>&1; do
    if [ $SECONDS -ge $deadline ]; then
        echo "FAIL: api-server did not respond within ${TIMEOUT}s"
        exit 1
    fi
    sleep 2
done

echo "==> /health"
curl -sf "$BASE/health" | tee /dev/stderr | grep -q '"status":"ok"'
echo "  PASS"

echo "==> /api/system/time"
curl -sf "$BASE/api/system/time" | tee /dev/stderr | grep -q '"time"'
echo "  PASS"

echo "==> /api/audio/volume-limit"
curl -sf "$BASE/api/audio/volume-limit" | tee /dev/stderr | grep -q '"limit"'
echo "  PASS"

echo "==> POST /api/audio/volume-limit"
curl -sf -X POST "$BASE/api/audio/volume-limit" \
    -H "Content-Type: application/json" \
    -d '{"limit":60}' | tee /dev/stderr | grep -q '"limit":60'
echo "  PASS"

echo ""
echo "All smoke tests passed."
