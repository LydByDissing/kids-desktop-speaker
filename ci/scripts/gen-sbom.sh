#!/bin/bash
# gen-sbom.sh — produce an SPDX SBOM for a release (DEC-026)
# ===========================================================
# Usage: gen-sbom.sh <rootfs.tar> <output.spdx.json> [version]
#
# Lists every Debian package in the image, from its dpkg database and copyright
# files, and every Rust crate in Cargo.lock, using Syft.

set -euo pipefail

ROOTFS=$(realpath "$1")
OUTPUT=$(realpath -m "$2")
VERSION="${3:-dev}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
SYFT_IMAGE=anchore/syft:v1.54.1

SCAN=$(mktemp -d "$REPO/ci/output/sbom-XXXX")
trap 'rm -rf "$SCAN"' EXIT

# Only what Syft needs, not the whole rootfs: the package database, license files,
# and os-release, without which packages get no package URL (purl).
tar -xf "$ROOTFS" -C "$SCAN" --wildcards --no-same-owner \
    'var/lib/dpkg/status' 'usr/share/doc/*/copyright' 'etc/os-release' 'usr/lib/os-release'
mkdir -p "$SCAN/src"
cp "$REPO/Cargo.lock" "$REPO/Cargo.toml" "$SCAN/src/"

docker run --rm -v "$SCAN:/scan:ro" "$SYFT_IMAGE" scan dir:/scan \
    --source-name kids-desktop-speaker --source-version "$VERSION" \
    -o spdx-json -q > "$OUTPUT"
echo "==> SBOM: $OUTPUT ($(python3 -c 'import json,sys; print(len(json.load(open(sys.argv[1]))["packages"]))' "$OUTPUT") packages)"
