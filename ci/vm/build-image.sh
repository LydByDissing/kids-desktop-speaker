#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT="$SCRIPT_DIR/rootfs.img"
KERNEL_OUT="$SCRIPT_DIR/vmlinuz"
INITRD_OUT="$SCRIPT_DIR/initrd.img"

# Step 1: Get kernel + initrd from a temporary ARM container
# We install linux-image-armmp-lpae inside arm32v7/debian:bookworm-slim
# and copy the kernel/initrd out to the host.
if [ ! -f "$KERNEL_OUT" ]; then
    echo "==> Extracting ARM kernel..."
    docker run --rm --platform linux/arm/v7 \
        -v "$SCRIPT_DIR:/out" \
        arm32v7/debian:bookworm-slim \
        bash -c '
            apt-get update -qq && apt-get install -y -q --no-install-recommends linux-image-armmp-lpae
            cp /boot/vmlinuz-*-armmp-lpae /out/vmlinuz
            cp /boot/initrd.img-*-armmp-lpae /out/initrd.img
        '
fi

# Step 2: Export our Docker image rootfs to a raw ext4 disk image
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

    # Create ext4 image (1.5GB)
    truncate -s 1536M "$OUTPUT"
    mkfs.ext4 -q -L rootfs "$OUTPUT"

    # Populate (requires root or unshare)
    MNTDIR=$(mktemp -d)
    if command -v unshare &>/dev/null && unshare --mount true 2>/dev/null; then
        unshare --mount bash -c "
            mount -o loop '$OUTPUT' '$MNTDIR'
            tar -xf '$TMPTAR' -C '$MNTDIR' --numeric-owner 2>/dev/null || true
            # Ensure api-server binary is present and executable
            # (CI installs stub scripts; in dev we build native and overlay)
            umount '$MNTDIR'
        "
    else
        echo "ERROR: need unshare+mount or run as root to populate ext4" >&2
        rm -f "$TMPTAR" "$OUTPUT"
        rmdir "$MNTDIR"
        exit 1
    fi
    rm -f "$TMPTAR"
    rmdir "$MNTDIR"
    echo "==> rootfs.img ready"
fi
