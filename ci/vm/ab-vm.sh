#!/bin/bash
# ab-vm.sh — run the A/B update and rollback test in QEMU (KDS-in5)
# ==================================================================
# Usage: ab-vm.sh <rootfs.tar>
#
# Builds the SD image with a kernel that runs on QEMU's virt machine, boots it
# with U-Boot's QEMU build, and runs ci/scripts/hw-ab-test.sh against it. The boot
# script, RAUC configuration and services are the same as on the Pi; only the
# kernel and U-Boot build differ. The broken-kernel test is left out, because with
# no watchdog the VM hangs (KDS-fjl).

set -euo pipefail

ROOTFS=$(realpath "$1")
VM=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$VM/../.." && pwd)
WORK="$REPO/ci/output/ab-vm"
PORT="${KDS_AB_VM_PORT:-2223}"
UBOOT_DEB=http://deb.debian.org/debian/pool/main/u/u-boot/u-boot-qemu_2023.01+dfsg-2+deb12u3_all.deb
UBOOT_SHA=02a14f0876b76c0d7d7934e9b83a77515c5bdbad1e00af84c445bfeb1e6b8665

QEMU_PID=
cleanup() {
    [ -n "$QEMU_PID" ] && kill "$QEMU_PID" 2>/dev/null
    docker run --rm -v "$REPO/ci/output:/o" --entrypoint /bin/rm debian:bookworm-slim -rf /o/ab-vm >/dev/null 2>&1 || true
}
trap cleanup EXIT
cleanup; mkdir -p "$WORK"

[ -f "$VM/vmlinuz" ] && [ -f "$VM/initrd.img" ] && [ -f "$VM/modules.tar" ] || bash "$VM/build-image.sh"
[ -f "$VM/ci_key" ] || ssh-keygen -q -t ed25519 -N '' -C kds-vm-ci -f "$VM/ci_key"

echo "==> Fetching U-Boot for QEMU (pinned)"
if [ ! -f "$VM/u-boot-qemu.bin" ]; then
    curl -fsSL -o "$WORK/u-boot.deb" "$UBOOT_DEB"
    echo "$UBOOT_SHA  $WORK/u-boot.deb" | sha256sum -c - >/dev/null
    dpkg-deb --fsys-tarfile "$WORK/u-boot.deb" | tar -xO ./usr/lib/u-boot/qemu_arm/u-boot.bin > "$VM/u-boot-qemu.bin"
fi

# The VM differs from the Pi only here: a kernel, initrd and modules that run on
# QEMU's virt machine, and no Wi-Fi service, which would wait 90 s for a wlan0
# the VM doesn't have.
echo "==> Building the VM rootfs (QEMU kernel in each slot)"
python3 - "$ROOTFS" "$WORK/rootfs.tar" "$VM/vmlinuz" "$VM/initrd.img" "$VM/modules.tar" <<'PY'
import io, sys, tarfile
src_path, dst_path, kernel, initrd, modules = sys.argv[1:6]
replace = {"boot/vmlinuz-kds": open(kernel, "rb").read()}
extra = {"boot/initrd-kds": open(initrd, "rb").read()}
drop = {"etc/systemd/system/multi-user.target.wants/wpa_supplicant@wlan0.service"}
with tarfile.open(src_path) as src, tarfile.open(dst_path, "w", format=tarfile.GNU_FORMAT) as dst:
    for m in src:
        name = m.name.removeprefix("./")
        if name in drop:
            continue
        data = replace.get(name, src.extractfile(m).read() if m.isreg() else None)
        if data is not None:
            m.size = len(data)
            dst.addfile(m, io.BytesIO(data))
        else:
            dst.addfile(m)
    for name, data in extra.items():
        info = tarfile.TarInfo("./" + name)
        info.size, info.mode = len(data), 0o644
        dst.addfile(info, io.BytesIO(data))
    with tarfile.open(modules) as mods:
        for m in mods:
            m.name = "./usr/" + m.name.removeprefix("./")
            dst.addfile(m, mods.extractfile(m) if m.isreg() else None)
PY

echo "==> Building the VM disk"
bash "$REPO/ci/scripts/build-sd-image.sh" "$WORK/rootfs.tar" "$WORK/sd.img.xz" >/dev/null
xz -dc "$WORK/sd.img.xz" > "$WORK/sd.img" && rm -f "$WORK/sd.img.xz"
# The SSH key goes where a device's owner puts theirs: the boot partition.
docker run --rm -v "$WORK:/w" -v "$VM/ci_key.pub:/key.pub:ro" debian:bookworm-slim bash -c '
    apt-get update -qq; apt-get install -y -qq --no-install-recommends mtools >/dev/null 2>&1
    mmd -i /w/sd.img@@4M ::/kds; mcopy -i /w/sd.img@@4M /key.pub ::/kds/authorized_keys; chmod a+rw /w/sd.img'

echo "==> Booting the A/B VM"
qemu-system-arm -M virt -cpu cortex-a15 -m 512M -bios "$VM/u-boot-qemu.bin" \
    -drive file="$WORK/sd.img",if=none,id=d0,format=raw -device virtio-blk-device,drive=d0 \
    -netdev user,id=n0,hostfwd=tcp::"$PORT"-:22 -device virtio-net-device,netdev=n0 \
    -display none -serial file:"$WORK/serial.log" </dev/null &
QEMU_PID=$!

for _ in $(seq 1 90); do
    ssh -i "$VM/ci_key" -p "$PORT" -o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=no \
        -o UserKnownHostsFile=/dev/null -o LogLevel=ERROR root@localhost true 2>/dev/null && break
    sleep 5
done

if [ -n "${KDS_AB_VM_DEBUG:-}" ]; then
    echo "==> Debug mode: VM left running for ${KDS_AB_VM_DEBUG}s; ssh -i $VM/ci_key -p $PORT root@localhost"
    sleep "$KDS_AB_VM_DEBUG"
    exit 0
fi

rc=0
PI_HOST=root@localhost HW_SSH_PORT="$PORT" HW_SSH_KEY="$VM/ci_key" HW_ROOTFS="$WORK/rootfs.tar" \
    HW_BAD_UPDATES=bad-api bash "$REPO/ci/scripts/hw-ab-test.sh" || rc=$?
if [ "$rc" -ne 0 ]; then
    echo "--- last serial output"; tr -d '\r' < "$WORK/serial.log" | grep -av '^\s*$' | tail -25
fi
exit "$rc"
