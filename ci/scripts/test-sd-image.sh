#!/bin/bash
# test-sd-image.sh — check an SD image's layout and boot files (DEC-025)
# Usage: test-sd-image.sh <image.img.xz>
set -uo pipefail
IMAGE=$(realpath "$1")
docker run --rm -v "$IMAGE:/in.xz:ro" debian:bookworm-slim bash -uc '
    apt-get update -qq; apt-get install -y -qq --no-install-recommends fdisk mtools e2fsprogs xz-utils u-boot-tools >/dev/null
    xz -dc /in.xz > /sd.img
    fail=0
    ok() { if eval "$2" >/dev/null 2>&1; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi; }
    start() { sfdisk -d /sd.img | awk -v p="sd.img$1 " "index(\$0,p){gsub(/,/,\"\"); print \$4}"; }
    size()  { sfdisk -d /sd.img | awk -v p="sd.img$1 " "index(\$0,p){gsub(/,/,\"\"); print \$6}"; }
    part()  { dd if=/sd.img of=/p$1 bs=512 skip=$(start $1) count=$(size $1) status=none; }
    part 1; part 2; part 4
    ok "image has four partitions" "[ \$(sfdisk -d /sd.img | grep -c start=) = 4 ]"
    ok "fits an 8 GB card" "[ \$(stat -c%s /sd.img) -lt 7500000000 ]"
    ok "boot partition is bootable FAT" "sfdisk -d /sd.img | grep -E \"sd.img1 .*type=c, bootable\""
    for f in bootcode.bin start.elf fixup.dat start_cd.elf fixup_cd.dat u-boot.bin config.txt boot.scr bcm2708-rpi-zero-w.dtb kds-boot.version; do
        ok "boot has $f" "mdir -i /p1 ::/$f"
    done
    mcopy -i /p1 ::/boot.scr /boot.scr; mcopy -i /p1 ::/config.txt /config.txt
    ok "boot script is a valid image" "dumpimage -l /boot.scr | grep -q Script"
    ok "config boots U-Boot" "grep -q ^kernel=u-boot.bin /config.txt"
    ok "no uboot.env, which would replace the default bootcmd" "! mdir -i /p1 ::/uboot.env"
    ok "gpu_mem=16 has its cut-down firmware" "! grep -q ^gpu_mem=16 /config.txt || mdir -i /p1 ::/start_cd.elf"
    ok "no Wi-Fi details or SSH keys in the boot partition" "! mdir -i /p1 ::/kds"
    dbg() { debugfs -R "$1" /p2 2>/dev/null; }
    ok "slot A has a kernel" "dbg \"stat /boot/vmlinuz-kds\" | grep -q Inode"
    ok "slot A has modules" "dbg \"ls -l /usr/lib/modules\" | grep -q rpi-v6"
    ok "slot A has Wi-Fi firmware" "dbg \"ls /usr/lib/firmware/brcm\" | grep -q brcmfmac43430"
    ok "slot A has RAUC keyring" "dbg \"stat /etc/rauc/keyring.pem\" | grep -q Inode"
    ok "slot A has no SSH host keys" "! dbg \"ls /etc/ssh\" | grep -q ssh_host_"
    ok "slot A mounts data by partition UUID" "dbg \"cat /etc/fstab\" | grep -q PARTUUID=4b445300-04"
    ok "disk ID gives the expected partition UUIDs" "sfdisk -d /sd.img | grep -q \"label-id: 0x4b445300\""
    ok "RAUC finds slots by partition UUID" "dbg \"cat /etc/rauc/system.conf\" | grep -q by-partuuid/4b445300-03"
    ok "fw_setenv uses the A/B state file" "dbg \"cat /etc/fw_env.config\" | grep -q kds-ab.env"
    ok "slot A has the RAUC D-Bus service" "dbg \"stat /usr/lib/systemd/system/rauc.service\" | grep -q Inode"
    ok "slot A enables clock sync" "dbg \"stat /etc/systemd/system/sysinit.target.wants/systemd-timesyncd.service\" | grep -q Inode"
    ok "slot A has no Docker marker" "! dbg \"stat /.dockerenv\" | grep -q Inode"
    ok "slot A resolv.conf points to resolved" "dbg \"stat /etc/resolv.conf\" | grep -q symlink"
    ok "slot A enables DNS" "dbg \"stat /etc/systemd/system/sysinit.target.wants/systemd-resolved.service\" | grep -qE Inode"
    ok "boot script resets if the kernel will not start" "tail -c 200 /boot.scr | grep -aq reset"
    ok "slot A has an empty machine-id" "[ -z \"\$(dbg \"cat /etc/machine-id\")\" ]"
    ok "slot A has no stored D-Bus machine-id" "dbg \"stat /var/lib/dbus/machine-id\" | grep -qE \"symlink|File not found\" || ! dbg \"stat /var/lib/dbus/machine-id\" | grep -q Inode"
    ok "slot A uses MAC-based DHCP" "dbg \"cat /etc/systemd/network/25-wifi.network\" | grep -q ClientIdentifier=mac"
    ok "slot A does not start dnsmasq" "! dbg \"ls /etc/systemd/system/multi-user.target.wants\" | grep -q dnsmasq"
    ok "slot A carries the boot files for updates" "dbg \"ls /usr/lib/kds-boot\" | grep -q update-boot.sh"
    ok "slot A has the health check" "dbg \"stat /usr/local/bin/kds-health-check.sh\" | grep -q Inode"
    ok "root account is locked" "dbg \"cat /etc/shadow\" | grep -qE \"^root:[!*]\""
    ok "data partition is ext4" "dumpe2fs -h /p4 | grep -q \"volume name:.*data\""
    exit $fail
'
rc=$?
[ "$rc" -eq 0 ] && echo "All SD image tests passed." || echo "SD image tests FAILED"
exit "$rc"
