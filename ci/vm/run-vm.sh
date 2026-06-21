#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Build image if not present
if [ ! -f "$SCRIPT_DIR/rootfs.img" ] || [ ! -f "$SCRIPT_DIR/vmlinuz" ]; then
    "$SCRIPT_DIR/build-image.sh"
fi

echo "==> Starting QEMU ARM VM (Ctrl-A X to quit)..."
echo "    API available at http://localhost:8080 once boot completes (~30s)"

AUDIO_ARGS=()
if [ "${KDS_AUDIO:-1}" = "1" ]; then
    AUDIO_ARGS=(
        -audiodev pa,id=audio0
        -device virtio-sound-pci,audiodev=audio0
    )
fi

exec qemu-system-arm \
    -M virt \
    -cpu cortex-a15 \
    -m 512M \
    -kernel "$SCRIPT_DIR/vmlinuz" \
    -initrd "$SCRIPT_DIR/initrd.img" \
    -append "root=/dev/vda rw console=ttyAMA0,115200 loglevel=3 quiet" \
    -drive file="$SCRIPT_DIR/rootfs.img",format=raw,if=none,id=vda \
    -device virtio-blk-device,drive=vda \
    -netdev user,id=net0,hostfwd=tcp::8080-:8080 \
    -device virtio-net-device,netdev=net0 \
    "${AUDIO_ARGS[@]}" \
    -nographic \
    -serial mon:stdio
