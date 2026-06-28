#!/bin/bash
# gen-manifest.sh — emit a release manifest.json for a RAUC bundle
# ================================================================
# Usage: gen-manifest.sh <bundle-path> <version>
#
# Prints the manifest JSON to stdout. The OTA Updater consumes this to decide
# whether an update is newer, then verifies the downloaded bundle against the
# sha256 before handing it to `rauc install` (CI_REQ_004; see the ota-updater
# "GitHub Releases Integration" design and ci-workflows Layer 4).
#
# Fields: version, bundle (basename), sha256, size (bytes), released_at (UTC).

set -euo pipefail

if [ "$#" -ne 2 ]; then
    echo "usage: $0 <bundle-path> <version>" >&2
    exit 2
fi

BUNDLE="$1"
VERSION="$2"

if [ ! -f "$BUNDLE" ]; then
    echo "error: bundle not found: $BUNDLE" >&2
    exit 1
fi

SHA=$(sha256sum "$BUNDLE" | awk '{print $1}')
SIZE=$(stat -c%s "$BUNDLE")
NAME=$(basename "$BUNDLE")
RELEASED_AT=$(date -u +%Y-%m-%dT%H:%M:%SZ)

cat <<EOF
{
  "version": "$VERSION",
  "bundle": "$NAME",
  "sha256": "$SHA",
  "size": $SIZE,
  "released_at": "$RELEASED_AT"
}
EOF
