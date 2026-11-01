# boot.cmd — A/B slot selection for RAUC (DEC-025)
# Each attempt uses up one try for the slot. `rauc mark-good` restores the count
# once a boot is healthy, and an exhausted slot falls through to the other one.

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
  saveenv
  reset
fi

saveenv
setenv bootargs "console=tty1 console=serial0,115200 root=/dev/mmcblk0p${bootpart} rootfstype=ext4 rootwait rw fsck.repair=yes rauc.slot=${raucslot}"
echo "Booting slot ${raucslot} (A left ${BOOT_A_LEFT}, B left ${BOOT_B_LEFT})"
load mmc 0:${bootpart} ${kernel_addr_r} /boot/vmlinuz-kds
bootz ${kernel_addr_r} - ${fdt_addr}
