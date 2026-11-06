#!/bin/bash
# test-license-inventory.sh — check the license inventory is complete (KDS-khs)
# Requires the image (make build).
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT
fail=0
check() { if eval "$2" >/dev/null 2>&1; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi; }

"$HERE/license-inventory.sh" "$TMP/licenses.tar.gz" >/dev/null || { echo "  [FAIL] inventory script failed"; exit 1; }
tar -xzf "$TMP/licenses.tar.gz" -C "$TMP"
L=$TMP/licenses
installed=$(docker run --rm --platform linux/arm/v6 --entrypoint dpkg-query kids-desktop-speaker:latest -W -f='x\n' | wc -l)

check "lists every installed package" "[ \$((\$(wc -l < $L/packages.tsv) - 1)) -eq $installed ]"
check "no package lacks a copyright file" "! grep -q 'MISSING COPYRIGHT FILE' $L/packages.tsv"
check "includes a copyright file per package" "[ \$(ls $L/copyright | wc -l) -eq $installed ]"
check "records source versions" "awk -F'\t' 'NR>1 && \$4==\"\"{exit 1}' $L/packages.tsv"
check "includes GPL software such as bluez" "grep -P '^bluez\t' $L/packages.tsv | grep -q GPL"
check "lists Rust crates" "[ \$(wc -l < $L/rust-crates.tsv) -gt 50 ]"
check "every crate declares a license" "! grep -q UNKNOWN $L/rust-crates.tsv"
check "our own crates are excluded" "! grep -qP '^(api-server|ota-updater)\t' $L/rust-crates.tsv"

[ "$fail" -eq 0 ] && echo "All license inventory tests passed." || echo "License inventory tests FAILED"
exit "$fail"
