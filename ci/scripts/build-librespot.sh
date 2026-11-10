#!/bin/bash
# build-librespot.sh — cross-compile librespot for the Pi Zero W (ARMv6, DEC-034)
# ==============================================================================
# Usage: build-librespot.sh
#
# librespot publishes no binaries, and raspotify's armhf package doesn't run on
# ARMv6. This downloads the pinned release from crates.io, checks it against the
# pinned SHA-256, and builds it with cross for arm-unknown-linux-gnueabihf.
# The result is ci/librespot/out/librespot. A stamp of the version and features
# makes a rebuild a no-op, so CI can cache the output directory.

set -euo pipefail

VERSION=0.8.0
SHA256=030c5e98cc06f20283b5948ae23bdac1de0dec58dee2c5ec4c8dafc7aaa796ab
# ALSA output to the "speaker" device; rustls with the system's CA roots (no
# OpenSSL to cross-compile); Zeroconf through the avahi-daemon already running.
FEATURES="alsa-backend rustls-tls-native-roots with-avahi"
TARGET=arm-unknown-linux-gnueabihf

REPO=$(cd "$(dirname "$0")/../.." && pwd)
OUT="$REPO/ci/librespot/out"
STAMP="librespot $VERSION $SHA256 $FEATURES $TARGET"

if [ -x "$OUT/librespot" ] && [ "$(cat "$OUT/stamp" 2>/dev/null)" = "$STAMP" ]; then
    echo "==> librespot $VERSION already built: $OUT/librespot"
    exit 0
fi

SRC="$OUT/src"
rm -rf "$SRC" "$OUT/librespot" "$OUT/stamp"
mkdir -p "$SRC"
echo "==> Downloading librespot $VERSION from crates.io..."
curl -sSfL --retry 3 -o "$SRC/librespot.crate" \
    "https://static.crates.io/crates/librespot/librespot-$VERSION.crate"
echo "$SHA256  $SRC/librespot.crate" | sha256sum -c --quiet - \
    || { echo "error: librespot $VERSION does not match the pinned SHA-256" >&2; exit 1; }
tar -xzf "$SRC/librespot.crate" -C "$SRC"

echo "==> Building the cross image..."
docker build -q -t kds-cross-alsa:0.2.5 "$REPO/ci/cross-alsa" >/dev/null

echo "==> Cross-compiling librespot $VERSION for $TARGET ($FEATURES)..."
cd "$SRC/librespot-$VERSION"
CROSS_CONFIG="$REPO/ci/cross-alsa/Cross.toml" cross build --release --locked \
    --target "$TARGET" --no-default-features --features "$FEATURES"

cp "target/$TARGET/release/librespot" "$OUT/librespot"
# Every crate compiled into the binary, with its licence, for the release's
# license inventory (ci/scripts/license-inventory.sh).
{
    printf "crate\tversion\tlicense\n"
    cargo tree --locked --target "$TARGET" --no-default-features --features "$FEATURES" \
        -e normal --prefix none --format $'{p}\t{l}' \
        | sed -E 's/ \(\*\)$//; s/^([^ ]+) v([^ \t]+)[^\t]*\t/\1\t\2\t/' | sort -u
} > "$OUT/crates.tsv"
# Keep the licence next to the binary for the release's license inventory.
cp LICENSE "$OUT/LICENSE"
echo "$STAMP" > "$OUT/stamp"
rm -rf "$SRC"
echo "==> Built $OUT/librespot ($(du -h "$OUT/librespot" | cut -f1))"
