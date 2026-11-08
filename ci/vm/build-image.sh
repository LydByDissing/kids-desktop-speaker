#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
OUTPUT="$SCRIPT_DIR/rootfs.img"
KERNEL_OUT="$SCRIPT_DIR/vmlinuz"
INITRD_OUT="$SCRIPT_DIR/initrd.img"

# Step 1: Get kernel + initrd from a temporary ARM container
# We install linux-image-armmp-lpae inside arm32v7/debian:bookworm-slim. This
# ARMv7 kernel only boots the emulated VM (Cortex-A15); it never ships to a device.
# and copy the kernel/initrd out to the host.
if [ ! -f "$KERNEL_OUT" ] || [ ! -f "$SCRIPT_DIR/modules.tar" ]; then
    echo "==> Extracting ARM kernel..."
    docker run --rm --platform linux/arm/v7 \
        -v "$SCRIPT_DIR:/out" \
        arm32v7/debian:bookworm-slim \
        bash -c '
            apt-get update -qq && apt-get install -y -q --no-install-recommends zstd linux-image-armmp-lpae
            cp /boot/vmlinuz-*-armmp-lpae /out/vmlinuz
            cp /boot/initrd.img-*-armmp-lpae /out/initrd.img
            # Modules too: the A/B VM mounts the FAT boot partition, and this
            # kernel builds vfat as a module (ab-vm.sh).
            tar -C / -cf /out/modules.tar $(cd / && ls -d lib/modules/*-armmp-lpae)
            chmod a+r /out/vmlinuz /out/initrd.img /out/modules.tar
        '
fi

# Step 2: Export our Docker image rootfs to a raw ext4 disk image.
# mke2fs -d builds it from a directory without loop mounts or host root access.
# Device nodes under ./dev are excluded — the kernel populates /dev via devtmpfs on boot.
if [ ! -f "$OUTPUT" ] || [ "${FORCE_REBUILD:-}" = "1" ]; then
    # KDS_VM_IMAGE reuses an image that is already built (CI builds one earlier
    # in the same job). Unset, the image is built here.
    VM_IMAGE="${KDS_VM_IMAGE:-}"
    if [ -z "$VM_IMAGE" ]; then
        VM_IMAGE=kids-desktop-speaker:vm
        echo "==> Building Docker image..."
        docker buildx build --platform linux/arm/v6 \
            -f "$(dirname "$SCRIPT_DIR")/Dockerfile.pi-image" \
            -t "$VM_IMAGE" --load \
            "$(dirname "$SCRIPT_DIR")/.."
    else
        echo "==> Reusing Docker image: $VM_IMAGE"
    fi

    echo "==> Exporting rootfs..."
    CONTAINER=$(docker create --platform linux/arm/v6 "$VM_IMAGE")
    TMPTAR=$(mktemp /tmp/rootfs-XXXX.tar)
    docker export "$CONTAINER" > "$TMPTAR.raw"
    python3 "$(dirname "$SCRIPT_DIR")/scripts/clean-rootfs.py" "$TMPTAR.raw" "$TMPTAR"
    rm -f "$TMPTAR.raw"
    # Removal can fail briefly while the storage driver is busy. It isn't fatal.
    docker rm "$CONTAINER" >/dev/null 2>&1 || { sleep 3; docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; }

    # A throwaway key that lets ci-run.sh check services over SSH. VM only: it goes
    # in through the device settings path, which a real device reads from its boot
    # partition instead.
    [ -f "$SCRIPT_DIR/ci_key" ] || ssh-keygen -q -t ed25519 -N '' -C kds-vm-ci -f "$SCRIPT_DIR/ci_key"

    # Build the disk in a container as root, so files keep their real owners, as on
    # a device. Unpacking as a normal user would make everything owned by that user.
    echo "==> Creating rootfs image..."
    rm -f "$OUTPUT"
    docker run --rm -v "$TMPTAR:/rootfs.tar:ro" -v "$SCRIPT_DIR:/vm" \
        -e OUT_UID="$(id -u)" -e OUT_GID="$(id -g)" debian:bookworm-slim bash -euc '
            apt-get update -qq
            apt-get install -y -qq --no-install-recommends e2fsprogs >/dev/null
            mkdir /w
            tar -xf /rootfs.tar -C /w --numeric-owner --exclude="./dev/*" --exclude="./proc/*" --exclude="./sys/*"
            install -d -m 755 /w/boot/firmware/kds
            install -m 644 /vm/ci_key.pub /w/boot/firmware/kds/authorized_keys
            mke2fs -q -t ext4 -L vm-root -d /w /vm/rootfs.img 1536M
            chown "$OUT_UID:$OUT_GID" /vm/rootfs.img
        '
    rm -f "$TMPTAR"
    echo "==> rootfs.img ready"
fi
