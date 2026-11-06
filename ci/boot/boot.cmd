# boot.cmd — A/B slot selection for RAUC (DEC-025, DEC-028)
#
# Runs unchanged on the Pi's U-Boot and on U-Boot's QEMU build. The A/B state
# lives in its own file, kds-ab.env, on the boot partition, rather than in
# U-Boot's environment, whose location differs between builds. Its name also
# keeps U-Boot from loading it as its whole environment, which once wiped
# bootcmd. Each attempt uses up one try for the slot. `rauc status mark-good`
# restores the count once a boot is healthy, and an exhausted slot falls through
# to the other one.

# devtype and devnum come from U-Boot's boot scan. Fall back to the Pi's SD card.
test -n "${devtype}" || setenv devtype mmc
test -n "${devnum}" || setenv devnum 0
setenv bootdev "${devtype} ${devnum}"
setenv abenv kds-ab.env

setenv BOOT_ORDER
setenv BOOT_A_LEFT
setenv BOOT_B_LEFT
if load ${bootdev}:1 ${kernel_addr_r} ${abenv}; then
  env import -c ${kernel_addr_r} 0x4000 BOOT_ORDER BOOT_A_LEFT BOOT_B_LEFT
fi
test -n "${BOOT_ORDER}" || setenv BOOT_ORDER "A B"
test -n "${BOOT_A_LEFT}" || setenv BOOT_A_LEFT 3
test -n "${BOOT_B_LEFT}" || setenv BOOT_B_LEFT 3

setenv bootpart
setenv raucslot
for BOOT_SLOT in "${BOOT_ORDER}"; do
  if test "x${bootpart}" != "x"; then
    # A slot is already selected.
    true
  elif test "x${BOOT_SLOT}" = "xA"; then
    if test ${BOOT_A_LEFT} -gt 0; then
      setexpr BOOT_A_LEFT ${BOOT_A_LEFT} - 1
      setenv bootpart 2
      setenv raucslot A
    fi
  elif test "x${BOOT_SLOT}" = "xB"; then
    if test ${BOOT_B_LEFT} -gt 0; then
      setexpr BOOT_B_LEFT ${BOOT_B_LEFT} - 1
      setenv bootpart 3
      setenv raucslot B
    fi
  fi
done

if test -z "${bootpart}"; then
  # Both slots are exhausted. Reset the counts and try again.
  setenv BOOT_A_LEFT 3
  setenv BOOT_B_LEFT 3
fi

# Save the state in the format fw_setenv reads: checksummed and 0x4000 bytes.
# kernel_addr_r is free scratch space until the kernel is loaded; scriptaddr holds
# this running script, so it must not be overwritten.
env export -c -s 0x4000 ${kernel_addr_r} BOOT_ORDER BOOT_A_LEFT BOOT_B_LEFT
fatwrite ${bootdev}:1 ${kernel_addr_r} ${abenv} 0x4000

if test -z "${bootpart}"; then
  reset
fi

# The root partition is found by its partition UUID, which is the same on the Pi
# and in the VM, unlike the device name.
part uuid ${bootdev}:${bootpart} rootuuid
setenv bootargs "console=tty1 console=serial0,115200 console=ttyAMA0,115200 root=PARTUUID=${rootuuid} rootfstype=ext4 rootwait rw fsck.repair=yes panic=10 rauc.slot=${raucslot}"
echo "Booting slot ${raucslot} (A left ${BOOT_A_LEFT}, B left ${BOOT_B_LEFT})"

# The Pi's firmware hands U-Boot a device tree in fdt_addr. QEMU's U-Boot has its own.
test -n "${fdt_addr}" || setenv fdt_addr ${fdtcontroladdr}

load ${bootdev}:${bootpart} ${kernel_addr_r} /boot/vmlinuz-kds
if load ${bootdev}:${bootpart} ${ramdisk_addr_r} /boot/initrd-kds; then
  bootz ${kernel_addr_r} ${ramdisk_addr_r}:${filesize} ${fdt_addr}
else
  bootz ${kernel_addr_r} - ${fdt_addr}
fi

# Only reached if the kernel is missing or invalid. Use up this try and start over.
reset
