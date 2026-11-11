#!/bin/bash
# license-inventory.sh — list third-party licenses for a release (KDS-khs)
# ========================================================================
# Usage: license-inventory.sh <output.tar.gz> [image]
#
# Produces an archive with:
#   packages.tsv     every Debian package in the image: version, source package,
#                    source version and declared licenses
#   rust-crates.tsv  every Rust crate compiled into our services, with its license
#   librespot-crates.tsv  every crate compiled into librespot (DEC-034)
#   camilladsp-crates.tsv every crate compiled into CamillaDSP (DEC-036)
#   copyright/       each package's full copyright file
# The source versions identify exactly which source code corresponds to the
# binaries, for GPL and LGPL obligations.

set -euo pipefail

OUTPUT=$(realpath -m "$1")
IMAGE="${2:-kids-desktop-speaker:latest}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/licenses/copyright"

docker run --rm --platform linux/arm/v6 -v "$WORK/licenses:/out" \
    -e OUT_UID="$(id -u)" -e OUT_GID="$(id -g)" \
    --entrypoint /bin/bash "$IMAGE" -euc '
        printf "package\tversion\tsource\tsource_version\tlicenses\n" > /out/packages.tsv
        dpkg-query -W -f="\${binary:Package}\t\${Version}\t\${source:Package}\t\${source:Version}\n" |
        while IFS=$(printf "\t") read -r pkg ver src srcver; do
            name=${pkg%%:*}
            file=/usr/share/doc/$name/copyright
            if [ -f "$file" ]; then
                lic=$(sed -n "s/^License: *//p" "$file" | sort -u | paste -sd ";" -)
                cp "$file" "/out/copyright/$name"
            else
                lic="MISSING COPYRIGHT FILE"
            fi
            printf "%s\t%s\t%s\t%s\t%s\n" "$pkg" "$ver" "$src" "$srcver" "${lic:-see copyright file}" >> /out/packages.tsv
        done
        chown -R "$OUT_UID:$OUT_GID" /out
    '

(cd "$REPO" && cargo metadata --format-version 1 --locked) | python3 -c '
import json, sys
meta = json.load(sys.stdin)
ours = set(meta["workspace_members"])
print("crate\tversion\tlicense\tsource")
for p in sorted(meta["packages"], key=lambda p: (p["name"], p["version"])):
    if p["id"] in ours:
        continue
    print("\t".join([p["name"], p["version"], p.get("license") or "UNKNOWN", p.get("source") or "local"]))
' > "$WORK/licenses/rust-crates.tsv"

LIBRESPOT_CRATES="$REPO/ci/librespot/out/crates.tsv"
[ -f "$LIBRESPOT_CRATES" ] || { echo "error: missing $LIBRESPOT_CRATES (run make -C ci librespot)" >&2; exit 1; }
cp "$LIBRESPOT_CRATES" "$WORK/licenses/librespot-crates.tsv"
CAMILLADSP_CRATES="$REPO/ci/camilladsp/out/crates.tsv"
[ -f "$CAMILLADSP_CRATES" ] || { echo "error: missing $CAMILLADSP_CRATES (run make -C ci camilladsp)" >&2; exit 1; }
cp "$CAMILLADSP_CRATES" "$WORK/licenses/camilladsp-crates.tsv"

tar -C "$WORK" -czf "$OUTPUT" licenses
echo "==> License inventory: $OUTPUT ($(($(wc -l < "$WORK/licenses/packages.tsv") - 1)) packages, $(($(wc -l < "$WORK/licenses/rust-crates.tsv") - 1)) crates)"
