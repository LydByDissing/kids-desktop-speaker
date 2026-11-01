#!/bin/bash
# gen-signing-ca.sh — create the stable development RAUC signing identity (DEC-023)
# =================================================================================
# Usage: gen-signing-ca.sh [--out-dir DIR] [--force]
#
# Writes the public CA certificate to ci/rauc/keyring.pem, which is committed
# and installed in the image. The signing key and certificate go straight into
# the GitHub secrets RAUC_SIGNING_KEY and RAUC_SIGNING_CERT. The CA private key
# is destroyed, so rotation means generating a new CA.
#
# --out-dir DIR writes all output to DIR and skips GitHub. Used by tests.

set -euo pipefail

REPO=$(cd "$(dirname "$0")/../.." && pwd)
OUT_DIR=""
FORCE=0
while [ "$#" -gt 0 ]; do
    case "$1" in
        --out-dir) OUT_DIR="$2"; shift 2 ;;
        --force) FORCE=1; shift ;;
        *) echo "usage: $0 [--out-dir DIR] [--force]" >&2; exit 2 ;;
    esac
done

KEYRING="${OUT_DIR:-$REPO/ci/rauc}/keyring.pem"
if [ -e "$KEYRING" ] && [ "$FORCE" -ne 1 ]; then
    echo "error: $KEYRING exists; devices trust it. Use --force to rotate." >&2
    exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
chmod 700 "$WORK"
DAYS=3650

openssl req -x509 -newkey rsa:4096 -nodes -days "$DAYS" \
    -keyout "$WORK/ca.key" -out "$WORK/ca.crt" \
    -subj "/O=Lyd by Dissing/CN=KDS Development CA" \
    -addext "basicConstraints=critical,CA:TRUE" \
    -addext "keyUsage=critical,keyCertSign,cRLSign" 2>/dev/null
openssl req -newkey rsa:4096 -nodes \
    -keyout "$WORK/signing.key" -out "$WORK/signing.csr" \
    -subj "/O=Lyd by Dissing/CN=KDS Development Signing" 2>/dev/null
printf 'basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\n' > "$WORK/ext"
openssl x509 -req -days "$DAYS" -in "$WORK/signing.csr" \
    -CA "$WORK/ca.crt" -CAkey "$WORK/ca.key" -CAcreateserial \
    -extfile "$WORK/ext" -out "$WORK/signing.crt" 2>/dev/null

if [ -n "$OUT_DIR" ]; then
    mkdir -p "$OUT_DIR"
    cp "$WORK/signing.key" "$WORK/signing.crt" "$OUT_DIR/"
else
    gh secret set RAUC_SIGNING_KEY < "$WORK/signing.key"
    gh secret set RAUC_SIGNING_CERT < "$WORK/signing.crt"
fi
cp "$WORK/ca.crt" "$KEYRING"
echo "Wrote keyring: $KEYRING"
