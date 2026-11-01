#!/bin/bash
# create-bundle.sh — Build a RAUC bundle for CI
# ==============================================
# Generates throwaway dev certs and produces a signed .raucb at
# /output/image.raucb. Runs inside the ARM container (rauc + openssl present).
#
# Packages the real root filesystem when /output/rootfs.tar is present
# (produced on the host by `make export-rootfs` via `docker export`). The
# rootfs slot is ext4, so the image is a tar archive: RAUC formats the slot
# and extracts the tar on install. Falls back to a placeholder rootfs when no
# exported tar is available, so the script still runs standalone for smoke use.
#
# Version comes from $VERSION (the git tag in CI); defaults to 0.1.0-dev.
#
# Signing (DEC-023): with RAUC_SIGNING_KEY and RAUC_SIGNING_CERT set, the bundle
# is signed with the stable identity and verified against /etc/rauc/keyring.pem.
# Otherwise it gets throwaway certs that no device trusts, unless
# RAUC_REQUIRE_STABLE_KEY=1, which makes that an error (release builds).

set -euo pipefail

CERT_DIR=/tmp/kds-certs
STAGING=/output/.bundle-staging
OUTPUT=/output/image.raucb
ROOTFS_SRC=/output/rootfs.tar
VERSION="${VERSION:-0.1.0-dev}"

rm -rf "$STAGING"
mkdir -p "$CERT_DIR" "$STAGING"
trap 'rm -rf "$STAGING"' EXIT

STABLE_KEY=0
umask 077
if [ -n "${RAUC_SIGNING_KEY:-}" ] && [ -n "${RAUC_SIGNING_CERT:-}" ]; then
    echo "==> Using stable signing identity"
    printf '%s\n' "$RAUC_SIGNING_KEY" > "$CERT_DIR/signing.key.pem"
    printf '%s\n' "$RAUC_SIGNING_CERT" > "$CERT_DIR/signing.cert.pem"
    STABLE_KEY=1
elif [ "${RAUC_REQUIRE_STABLE_KEY:-0}" = "1" ]; then
    echo "error: RAUC_SIGNING_KEY and RAUC_SIGNING_CERT are required" >&2
    exit 1
else
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
fi
umask 022

echo "==> Creating bundle staging dir (version $VERSION)..."
if [ -f "$ROOTFS_SRC" ]; then
    echo "==> Using exported rootfs: $ROOTFS_SRC ($(du -sh "$ROOTFS_SRC" | cut -f1))"
    ln -f "$ROOTFS_SRC" "$STAGING/rootfs.tar" 2>/dev/null || cp "$ROOTFS_SRC" "$STAGING/rootfs.tar"
    IMG=rootfs.tar
else
    echo "==> WARNING: $ROOTFS_SRC not found; using placeholder rootfs (CI smoke only)"
    dd if=/dev/zero of="$STAGING/rootfs.img" bs=1024 count=1 2>/dev/null
    IMG=rootfs.img
fi

cat > "$STAGING/manifest.raucm" <<MANIFEST
[update]
compatible=kids-desktop-speaker
version=$VERSION

[image.rootfs]
filename=$IMG
MANIFEST

echo "==> Running rauc bundle..."
rm -f "$OUTPUT"
rauc bundle \
    --cert="$CERT_DIR/signing.cert.pem" \
    --key="$CERT_DIR/signing.key.pem" \
    "$STAGING" "$OUTPUT"

if [ "$STABLE_KEY" = "1" ]; then
    echo "==> Verifying bundle against /etc/rauc/keyring.pem..."
    # RAUC must identify the bundle's mount, which can fail on bind mounts, so
    # verify a copy on the tmpfs the caller mounts at /verify.
    VERIFY_COPY="$OUTPUT"
    if mountpoint -q /verify 2>/dev/null; then
        VERIFY_COPY=/verify/image.raucb
        cp "$OUTPUT" "$VERIFY_COPY"
    fi
    rauc info --keyring=/etc/rauc/keyring.pem "$VERIFY_COPY" >/dev/null
fi

echo "==> Bundle created: $OUTPUT ($(du -sh "$OUTPUT" | cut -f1))"
