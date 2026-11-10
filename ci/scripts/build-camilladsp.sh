#!/bin/bash
# build-camilladsp.sh — cross-compile CamillaDSP for the Pi Zero W (ARMv6, DEC-036)
# ================================================================================
# Usage: build-camilladsp.sh
#
# CamillaDSP is GPL-3.0, so we build it from source and publish that source with
# each release. This downloads the pinned release tag from GitHub, checks it
# against the pinned SHA-256, adds our Cargo.lock (the tag ships none; ours pins
# every dependency) and builds with cross for arm-unknown-linux-gnueabihf.
# Output in ci/camilladsp/out: the binary, the exact source (upstream archive
# plus Cargo.lock) for the release's source bundle, and the crate licences.
# A stamp makes a rebuild a no-op, so CI can cache the output directory.

set -euo pipefail

VERSION=4.1.3
SHA256=7e2fdca1e9c73885426ac889d27682c76d49ea1ea52c5f5bfe5356d7fdfa52df
TARGET=arm-unknown-linux-gnueabihf

REPO=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$REPO/ci/camilladsp/out"
LOCK="$REPO/ci/camilladsp/Cargo.lock"
STAMP="camilladsp $VERSION $SHA256 $(sha256sum "$LOCK" | cut -d' ' -f1) $TARGET"

if [ -x "$OUT/camilladsp" ] && [ "$(cat "$OUT/stamp" 2>/dev/null)" = "$STAMP" ]; then
    echo "==> CamillaDSP $VERSION already built: $OUT/camilladsp"
    exit 0
fi

SRC="$OUT/src"
rm -rf "$SRC" "$OUT"/camilladsp* "$OUT/stamp" "$OUT/crates.tsv"
mkdir -p "$SRC"
ARCHIVE="$OUT/camilladsp-$VERSION.tar.gz"
echo "==> Downloading CamillaDSP $VERSION..."
curl -sSfL --retry 3 -o "$ARCHIVE" \
    "https://github.com/HEnquist/camilladsp/archive/refs/tags/v$VERSION.tar.gz"
echo "$SHA256  $ARCHIVE" | sha256sum -c --quiet - \
    || { echo "error: CamillaDSP $VERSION does not match the pinned SHA-256" >&2; exit 1; }
tar -xzf "$ARCHIVE" -C "$SRC"
cp "$LOCK" "$SRC/camilladsp-$VERSION/Cargo.lock"

echo "==> Building the cross image..."
docker build -q -t kds-cross-alsa:0.2.5 "$REPO/ci/cross-alsa" >/dev/null

echo "==> Cross-compiling CamillaDSP $VERSION for $TARGET..."
cd "$SRC/camilladsp-$VERSION"
CROSS_CONFIG="$REPO/ci/cross-alsa/Cross.toml" cross build --release --locked --target "$TARGET"
cp "target/$TARGET/release/camilladsp" "$OUT/camilladsp"
# GPL-3.0 for the program; some files are MPL-2.0.
cat LICENSE_GLPv3.txt LICENSE_MPL2.0.txt > "$OUT/LICENSE"
{
    printf "crate\tversion\tlicense\n"
    cargo tree --locked --target "$TARGET" -e normal --prefix none --format $'{p}\t{l}' \
        | sed -E 's/ \(\*\)$//; s/^([^ ]+) v([^ \t]+)[^\t]*\t/\1\t\2\t/' | sort -u
} > "$OUT/crates.tsv"
# The corresponding source for the release (GPL-3.0): upstream archive + lockfile.
cp "$LOCK" "$OUT/camilladsp-$VERSION.Cargo.lock"
echo "$STAMP" > "$OUT/stamp"
rm -rf "$SRC"
echo "==> Built $OUT/camilladsp ($(du -h "$OUT/camilladsp" | cut -f1))"
