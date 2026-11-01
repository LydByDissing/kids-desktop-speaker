#!/bin/bash
# test-create-bundle.sh — test ci/create-bundle.sh real-rootfs bundling (KDS-35k)
# ===============================================================================
# Drives create-bundle.sh inside the ARM image with a fixture rootfs tar and a
# version, then asserts via `rauc info` that the bundle carries the real rootfs
# tar and the passed version. Also checks the placeholder fallback. Requires the
# image to be built first (make build); exits non-zero on any mismatch.

set -euo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
IMAGE=kids-desktop-speaker:latest
PLATFORM=linux/arm/v6

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "error: $IMAGE not found — run 'make -C ci build' first" >&2
    exit 2
fi

TMP=$(mktemp -d)
TMP2=$(mktemp -d)
trap 'rm -rf "$TMP" "$TMP2"' EXIT

run_bundle() {
    docker run --rm --platform "$PLATFORM" \
        -v "$1:/output" \
        -v "$REPO/ci/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro" \
        ${2:+-e VERSION=$2} \
        --entrypoint /bin/bash "$IMAGE" /usr/local/bin/create-bundle.sh >/dev/null
}
info() {
    docker run --rm --platform "$PLATFORM" -v "$1:/output" \
        --entrypoint /bin/bash "$IMAGE" \
        -c "rauc info --no-verify /output/image.raucb 2>/dev/null"
}

fail=0
assert() {
    if echo "$2" | grep -qE "$3"; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi
}

mkdir -p "$TMP/rootdir"
echo marker > "$TMP/rootdir/etc-os-release"
tar -C "$TMP/rootdir" -cf "$TMP/rootfs.tar" .
VERSION=v9.9.9-test
run_bundle "$TMP" "$VERSION"
INFO=$(info "$TMP")
assert "version matches the tag"        "$INFO" "Version:.*$VERSION"
assert "real rootfs tar is bundled"     "$INFO" "Filename:.*rootfs\.tar"
assert "compatible is kids-desktop-speaker" "$INFO" "Compatible:.*kids-desktop-speaker"

run_bundle "$TMP2" ""
INFO2=$(info "$TMP2")
assert "fallback to placeholder when no rootfs.tar" "$INFO2" "Filename:.*rootfs\.img"
assert "fallback version defaults to 0.1.0-dev"     "$INFO2" "Version:.*0\.1\.0-dev"

echo "========================================"
if [ "$fail" -ne 0 ]; then
    echo "create-bundle tests FAILED"; exit 1
fi
echo "All create-bundle tests passed."
