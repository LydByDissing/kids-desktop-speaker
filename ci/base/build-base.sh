#!/bin/bash
# build-base.sh — build the ARMv6 Raspbian root filesystem (DEC-024)
# ==================================================================
# Usage: build-base.sh <output-tar>
#
# Runs debootstrap in a privileged container, because it needs chroot and mknod.
# The second stage runs ARM code under QEMU pinned to the Pi Zero W's CPU, so
# anything that isn't ARMv6 fails here. Requires binfmt_misc for ARM.

set -euo pipefail

OUTPUT=$(realpath -m "$1")
HERE=$(cd "$(dirname "$0")" && pwd)
FINGERPRINT=A0DA38D0D76E8B5D638872819165938D90FDDD2E

mkdir -p "$(dirname "$OUTPUT")"
docker run --rm --privileged \
    -v "$HERE/raspbian.public.key:/key.asc:ro" \
    -v "$(dirname "$OUTPUT")":/out \
    -e FINGERPRINT="$FINGERPRINT" \
    -e OUT_NAME="$(basename "$OUTPUT")" \
    -e OUT_UID="$(id -u)" -e OUT_GID="$(id -g)" \
    debian:bookworm-slim bash -euc '
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends debootstrap gnupg ca-certificates >/dev/null
        actual=$(gpg --show-keys --with-colons /key.asc | awk -F: "/^fpr/{print \$10; exit}")
        [ "$actual" = "$FINGERPRINT" ] || { echo "error: unexpected Raspbian key: $actual" >&2; exit 1; }
        gpg --dearmor < /key.asc > /tmp/raspbian.gpg

        export QEMU_CPU=arm1176
        MIRROR=http://archive.raspbian.org/raspbian
        debootstrap --arch=armhf --variant=minbase --keyring=/tmp/raspbian.gpg \
            bookworm /rootfs "$MIRROR"

        echo "deb $MIRROR bookworm main contrib non-free rpi" > /rootfs/etc/apt/sources.list
        install -m 644 /tmp/raspbian.gpg /rootfs/etc/apt/trusted.gpg.d/raspbian.gpg
        rm -rf /rootfs/var/cache/apt/archives/*.deb /rootfs/var/lib/apt/lists/*

        tar --numeric-owner -C /rootfs -cf "/out/$OUT_NAME" .
        chown "$OUT_UID:$OUT_GID" "/out/$OUT_NAME"
    '
echo "==> Base rootfs: $OUTPUT ($(du -sh "$OUTPUT" | cut -f1))"
