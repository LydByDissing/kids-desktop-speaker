#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT="$SCRIPT_DIR/rootfs.img"
KERNEL_OUT="$SCRIPT_DIR/vmlinuz"
INITRD_OUT="$SCRIPT_DIR/initrd.img"

if ! command -v genext2fs &>/dev/null; then
    echo "ERROR: genext2fs not found. Install with: sudo apt-get install genext2fs" >&2
    exit 1
fi

# Step 1: Get kernel + initrd from a temporary ARM container
# We install linux-image-armmp-lpae inside arm32v7/debian:bookworm-slim
# and copy the kernel/initrd out to the host.
if [ ! -f "$KERNEL_OUT" ]; then
    echo "==> Extracting ARM kernel..."
    docker run --rm --platform linux/arm/v7 \
        -v "$SCRIPT_DIR:/out" \
        arm32v7/debian:bookworm-slim \
        bash -c '
            apt-get update -qq && apt-get install -y -q --no-install-recommends zstd linux-image-armmp-lpae
            cp /boot/vmlinuz-*-armmp-lpae /out/vmlinuz
            cp /boot/initrd.img-*-armmp-lpae /out/initrd.img
        '
fi

# Step 2: Export our Docker image rootfs to a raw ext2 disk image.
# genext2fs creates the image from a directory without needing root or loop mounts.
# Device nodes under ./dev are excluded — the kernel populates /dev via devtmpfs on boot.
if [ ! -f "$OUTPUT" ] || [ "${FORCE_REBUILD:-}" = "1" ]; then
    echo "==> Building Docker image..."
    docker buildx build --platform linux/arm/v7 \
        -f "$(dirname "$SCRIPT_DIR")/Dockerfile.pi-image" \
        -t kids-desktop-speaker:vm --load \
        "$(dirname "$SCRIPT_DIR")/.."

    echo "==> Exporting rootfs..."
    CONTAINER=$(docker create --platform linux/arm/v7 kids-desktop-speaker:vm)
    TMPTAR=$(mktemp /tmp/rootfs-XXXX.tar)
    docker export "$CONTAINER" > "$TMPTAR"
    docker rm "$CONTAINER"

    echo "==> Creating rootfs image (genext2fs, no root needed)..."
    MNTDIR=$(mktemp -d)
    tar -xf "$TMPTAR" -C "$MNTDIR" --exclude='./dev/*' 2>/dev/null || true
    mkdir -p "$MNTDIR/dev"
    rm -f "$TMPTAR"

    # 1536MB = 1572864 × 1KB blocks; 200000 inodes; 0% reserved
    genext2fs -d "$MNTDIR" -b 1572864 -N 200000 -m 0 "$OUTPUT"
    rm -rf "$MNTDIR"
    echo "==> rootfs.img ready"
fi
