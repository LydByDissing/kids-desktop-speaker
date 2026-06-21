#!/bin/bash
# create-bundle.sh — Build a RAUC bundle for CI
# ==============================================
# Generates throwaway dev certs, creates a minimal bundle staging directory,
# and produces a signed .raucb artifact at /output/image.raucb.
#
# Runs inside the ARM container (kids-desktop-speaker:latest).
# openssl and rauc are both available in that image.
#
# The generated certs are CI-only throwaway keys — never stored, never reused.

set -euo pipefail

CERT_DIR=/tmp/kds-certs
STAGING=/tmp/kds-bundle
OUTPUT=/output/image.raucb

mkdir -p "$CERT_DIR" "$STAGING"

echo "==> Generating throwaway dev certs..."
openssl req -x509 -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/ca.key.pem" \
    -out    "$CERT_DIR/ca.cert.pem" \
    -days 365 -subj "/CN=KDS CI CA" 2>/dev/null

openssl req -newkey rsa:2048 -nodes \
    -keyout "$CERT_DIR/signing.key.pem" \
    -out    "$CERT_DIR/signing.csr" \
    -subj "/CN=KDS CI Signing" 2>/dev/null

openssl x509 -req \
    -in "$CERT_DIR/signing.csr" \
    -CA "$CERT_DIR/ca.cert.pem" \
    -CAkey "$CERT_DIR/ca.key.pem" \
    -CAcreateserial \
    -out "$CERT_DIR/signing.cert.pem" \
    -days 365 2>/dev/null

echo "==> Creating bundle staging dir..."
cat > "$STAGING/manifest.raucm" << 'MANIFEST'
[update]
compatible=kids-desktop-speaker
version=0.1.0-dev

[image.rootfs]
filename=rootfs.img
MANIFEST

# Placeholder image — not a real rootfs, CI only
dd if=/dev/zero of="$STAGING/rootfs.img" bs=1024 count=1 2>/dev/null

echo "==> Running rauc bundle..."
rauc bundle \
    --cert="$CERT_DIR/signing.cert.pem" \
    --key="$CERT_DIR/signing.key.pem" \
    "$STAGING" "$OUTPUT"

echo "==> Bundle created: $OUTPUT ($(du -sh "$OUTPUT" | cut -f1))"
