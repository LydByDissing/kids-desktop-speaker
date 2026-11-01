#!/bin/bash
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Build image if not present
if [ ! -f "$SCRIPT_DIR/rootfs.img" ] || [ ! -f "$SCRIPT_DIR/vmlinuz" ] || [ "${FORCE_REBUILD:-}" = "1" ]; then
    "$SCRIPT_DIR/build-image.sh"
fi

echo "==> Starting QEMU ARM VM (headless)..."
echo "    Logs: /tmp/kds-vm.log"

# Construct QEMU args, same as run-vm.sh but headless
AUDIO_ARGS=()
if [ "${KDS_AUDIO:-0}" = "1" ]; then
    AUDIO_ARGS=(
        -audiodev pipewire,id=audio0
        -device virtio-sound-pci,audiodev=audio0
    )
fi

# Start QEMU in background. Redirect stdin from /dev/null so -nographic
# does not consume the terminal and QEMU does not exit on EOF.
qemu-system-arm \
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
    -display none \
    -serial file:/tmp/kds-vm.log \
    < /dev/null &

QEMU_PID=$!
echo "==> QEMU PID: $QEMU_PID"

# Trap to kill QEMU on exit
trap "kill $QEMU_PID 2>/dev/null || true; wait $QEMU_PID 2>/dev/null || true" EXIT

echo "==> Waiting for API to be ready..."
# Poll localhost:8080/health every 3s; 30 attempts by default (slower CI runners set more)
MAX_ATTEMPTS="${KDS_VM_BOOT_ATTEMPTS:-30}"
ATTEMPT=0
while [ $ATTEMPT -lt $MAX_ATTEMPTS ]; do
    if curl -sf http://localhost:8080/health >/dev/null 2>&1; then
        echo "==> API is ready!"
        break
    fi
    ATTEMPT=$((ATTEMPT + 1))
    if [ $ATTEMPT -eq $MAX_ATTEMPTS ]; then
        echo "FAIL: API did not become ready after $MAX_ATTEMPTS attempts"
        exit 1
    fi
    sleep 3
done

echo "==> Running smoke tests..."
SMOKE_EXIT_CODE=0
"$SCRIPT_DIR/dev-smoke.sh" || SMOKE_EXIT_CODE=$?

echo "==> Cleaning up QEMU..."
kill $QEMU_PID 2>/dev/null || true
wait $QEMU_PID 2>/dev/null || true

exit $SMOKE_EXIT_CODE
