#!/bin/bash
# test-gen-manifest.sh — test ci/scripts/gen-manifest.sh (CI_REQ_004)
# ===================================================================
# Builds a fixture bundle, runs gen-manifest.sh, and asserts the manifest
# schema and that sha256/size match the fixture. Exits non-zero on any
# mismatch so it can gate CI.

set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
GEN="$HERE/gen-manifest.sh"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

BUNDLE="$TMP/image.raucb"
VERSION="v1.2.3"
head -c 4096 /dev/urandom > "$BUNDLE"

EXPECT_SHA=$(sha256sum "$BUNDLE" | awk '{print $1}')
EXPECT_SIZE=$(stat -c%s "$BUNDLE")

OUT=$("$GEN" "$BUNDLE" "$VERSION")

fail=0
check() {
    if [ "$2" = "$3" ]; then
        echo "  [PASS] $1"
    else
        echo "  [FAIL] $1: got '$2' expected '$3'"; fail=1
    fi
}

if echo "$OUT" | jq -e . >/dev/null 2>&1; then
    echo "  [PASS] output is valid JSON"
else
    echo "  [FAIL] output is not valid JSON"; fail=1
fi

check "version" "$(echo "$OUT" | jq -r .version)" "$VERSION"
check "bundle"  "$(echo "$OUT" | jq -r .bundle)"  "image.raucb"
check "sha256"  "$(echo "$OUT" | jq -r .sha256)"  "$EXPECT_SHA"
check "size"    "$(echo "$OUT" | jq -r .size)"    "$EXPECT_SIZE"

for k in version bundle sha256 size released_at; do
    if [ "$(echo "$OUT" | jq "has(\"$k\")")" = "true" ]; then
        echo "  [PASS] key present: $k"
    else
        echo "  [FAIL] key missing: $k"; fail=1
    fi
done

if "$GEN" "$TMP/nope.raucb" "$VERSION" >/dev/null 2>&1; then
    echo "  [FAIL] expected non-zero exit on missing bundle"; fail=1
else
    echo "  [PASS] non-zero exit on missing bundle"
fi

if "$GEN" "$BUNDLE" >/dev/null 2>&1; then
    echo "  [FAIL] expected non-zero exit on missing version arg"; fail=1
else
    echo "  [PASS] non-zero exit on missing version arg"
fi

echo "========================================"
if [ "$fail" -ne 0 ]; then
    echo "gen-manifest tests FAILED"; exit 1
fi
echo "All gen-manifest tests passed."
