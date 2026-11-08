#!/bin/bash
# update-boot.sh — update the boot partition from a newly installed slot (DEC-029)
# =================================================================================
# Usage: update-boot.sh <new-slot-boot-files> <boot-partition>
#
# Run by the bundle's post-install hook, from the new slot, before RAUC activates
# it. Copies each boot file that differs: written beside the original, then
# renamed into place, firmware and U-Boot first and the boot script last. Leaves
# everything else on the boot partition alone, including kds-ab.env and kds/.
# Exits non-zero, failing the install, if a file is missing or the boot script
# isn't a valid U-Boot image.

set -euo pipefail

SRC="${1:?usage: update-boot.sh <new-slot-boot-files> <boot-partition>}"
DST="${2:?usage: update-boot.sh <new-slot-boot-files> <boot-partition>}"
# Order matters: the boot script last, so it's never newer than what it boots.
FILES=(bootcode.bin start.elf fixup.dat start_cd.elf fixup_cd.dat u-boot.bin
       bcm2708-rpi-zero-w.dtb config.txt boot.scr kds-boot.version)

log() { echo "update-boot: $*"; }

[ -d "$DST" ] || { log "boot partition $DST not found"; exit 1; }
for f in "${FILES[@]}"; do
    [ -s "$SRC/$f" ] || { log "new slot is missing $f; boot partition unchanged"; exit 1; }
done
# A U-Boot image starts with the magic number 0x27051956.
magic=$(od -An -tx1 -N4 "$SRC/boot.scr" | tr -d ' \n')
[ "$magic" = 27051956 ] || { log "boot.scr is not a U-Boot image; boot partition unchanged"; exit 1; }

changed=0
for f in "${FILES[@]}"; do
    if cmp -s "$SRC/$f" "$DST/$f"; then
        continue
    fi
    cp "$SRC/$f" "$DST/.$f.new"
    sync "$DST/.$f.new" 2>/dev/null || sync
    mv -f "$DST/.$f.new" "$DST/$f"
    sync
    log "updated $f"
    changed=$((changed + 1))
done
log "$changed file(s) updated; boot files now $(cat "$DST/kds-boot.version")"
