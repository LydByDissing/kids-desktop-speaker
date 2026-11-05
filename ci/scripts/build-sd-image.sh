#!/bin/bash
# build-sd-image.sh — build a flashable SD card image for the Pi Zero W (DEC-025)
# ===============================================================================
# Usage: build-sd-image.sh <rootfs.tar> <output.img.xz>
#
# Layout: boot (FAT) | rootfs A | rootfs B | data, matching ci/rauc/system.conf.
# Slot B starts empty and is formatted on the first update. Needs no privileges,
# loop devices or mounts.

set -euo pipefail

ROOTFS=$(realpath "$1")
OUTPUT=$(realpath -m "$2")
BOOTDIR=$(cd "$(dirname "$0")/../boot" && pwd)
mkdir -p "$(dirname "$OUTPUT")"

docker run --rm \
    -v "$ROOTFS:/rootfs.tar:ro" -v "$BOOTDIR:/bootsrc:ro" -v "$(dirname "$OUTPUT")":/out \
    -e OUT_NAME="$(basename "$OUTPUT")" -e OUT_UID="$(id -u)" -e OUT_GID="$(id -g)" \
    debian:bookworm-slim bash -euc '
        apt-get update -qq
        apt-get install -y -qq --no-install-recommends \
            dosfstools e2fsprogs fdisk mtools u-boot-tools xz-utils >/dev/null

        # Sizes in MiB. An 8 GB card holds the whole layout.
        BOOT=256 SLOT=2048 DATA=1024 START=4
        TOTAL=$((START + BOOT + SLOT + SLOT + DATA))
        W=/work; mkdir -p $W/root $W/boot

        tar -xf /rootfs.tar -C $W/root --numeric-owner --exclude="./dev/*" --exclude="./proc/*" --exclude="./sys/*"
        F=$W/root/usr/lib/kds-boot
        cp $F/bootcode.bin $F/start.elf $F/fixup.dat $F/start_cd.elf $F/fixup_cd.dat $F/u-boot.bin $W/boot/
        cp /bootsrc/config.txt $W/boot/
        cp $W/root/usr/lib/linux-image-*/bcm2708-rpi-zero-w.dtb $W/boot/
        mkimage -q -A arm -T script -C none -n "KDS A/B boot" -d /bootsrc/boot.cmd $W/boot/boot.scr
        # Do not ship uboot.env: a valid file replaces all of U-Boot'"'"'s built-in
        # defaults, including bootcmd, and it then waits at a prompt forever. The
        # boot script sets the slot variables and saves the file on first boot.

        truncate -s "${BOOT}M" $W/p1
        mkfs.vfat -n KDS-BOOT $W/p1 >/dev/null
        mcopy -s -i $W/p1 $W/boot/* ::/

        mke2fs -q -t ext4 -L rootfs-a -d $W/root $W/p2 "${SLOT}M"
        mke2fs -q -t ext4 -L data $W/p4 "${DATA}M"

        IMG=$W/sd.img
        truncate -s "${TOTAL}M" $IMG
        sfdisk -q $IMG <<SFDISK
label: dos
${START}M,${BOOT}M,c,*
,${SLOT}M,83
,${SLOT}M,83
,${DATA}M,83
SFDISK
        dd if=$W/p1 of=$IMG bs=1M seek=$START conv=notrunc,sparse status=none
        dd if=$W/p2 of=$IMG bs=1M seek=$((START + BOOT)) conv=notrunc,sparse status=none
        dd if=$W/p4 of=$IMG bs=1M seek=$((START + BOOT + SLOT + SLOT)) conv=notrunc,sparse status=none

        xz -T0 -3 -c $IMG > "/out/$OUT_NAME"
        chown "$OUT_UID:$OUT_GID" "/out/$OUT_NAME"
    '
echo "==> SD image: $OUTPUT ($(du -h "$OUTPUT" | cut -f1) compressed)"
