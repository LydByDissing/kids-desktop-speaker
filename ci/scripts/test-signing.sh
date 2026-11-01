#!/bin/bash
# test-signing.sh — test stable-key bundle signing (DEC-023)
# ==========================================================
# Asserts that a bundle signed with the stable identity verifies against its
# keyring, a throwaway-signed bundle does not, and a release build refuses to
# fall back to throwaway keys. Requires the image (make build).

set -uo pipefail

HERE=$(cd "$(dirname "$0")" && pwd)
REPO=$(cd "$HERE/../.." && pwd)
IMAGE=kids-desktop-speaker:latest
PLATFORM=linux/arm/v7

TMP=$(mktemp -d)
trap 'docker run --rm -v "$TMP:/t" --entrypoint /bin/rm --platform "$PLATFORM" "$IMAGE" -rf /t/stable /t/throwaway /t/required >/dev/null 2>&1; rm -rf "$TMP"' EXIT
"$HERE/gen-signing-ca.sh" --out-dir "$TMP/id" >/dev/null || exit 1
mkdir -p "$TMP/stable" "$TMP/throwaway" "$TMP/required"

bundle() { # output-dir [docker args]
    local out="$1"; shift
    docker run --rm --platform "$PLATFORM" --tmpfs /verify:size=1g -v "$out:/output" \
        -v "$TMP/id/keyring.pem:/etc/rauc/keyring.pem:ro" \
        -v "$REPO/ci/create-bundle.sh:/usr/local/bin/create-bundle.sh:ro" \
        "$@" --entrypoint /bin/bash "$IMAGE" /usr/local/bin/create-bundle.sh >"$out.log" 2>&1
}
verify() {
    docker run --rm --platform "$PLATFORM" --tmpfs /verify:size=1g -v "$1:/output" -v "$TMP/id/keyring.pem:/k.pem:ro" \
        --entrypoint /bin/bash "$IMAGE" \
        -c "cp /output/image.raucb /verify/ && rauc info --keyring=/k.pem /verify/image.raucb" >"$1.verify.log" 2>&1
}

fail=0
check() { if [ "$2" = "$3" ]; then echo "  [PASS] $1"; else echo "  [FAIL] $1"; fail=1; fi; }

bundle "$TMP/stable" -e RAUC_SIGNING_KEY="$(cat "$TMP/id/signing.key")" -e RAUC_SIGNING_CERT="$(cat "$TMP/id/signing.crt")"
check "stable-signed bundle builds" "$?" 0
verify "$TMP/stable"
check "stable-signed bundle verifies against keyring" "$?" 0

# The stable path should succeed, so show its output if it did not.
if [ "$fail" -ne 0 ]; then
    echo "--- stable bundle log"; tail -20 "$TMP/stable.log"
    echo "--- stable verify log"; tail -20 "$TMP/stable.verify.log"
fi

bundle "$TMP/throwaway"
check "throwaway bundle builds" "$?" 0
verify "$TMP/throwaway"
[ "$?" -ne 0 ]
check "throwaway bundle is rejected" "$?" 0
grep -qiE 'signature|certificate|verif.*fail' "$TMP/throwaway.verify.log" && ! grep -q 'mounted device' "$TMP/throwaway.verify.log"
check "rejection is a signature failure" "$?" 0
tail -3 "$TMP/throwaway.verify.log" | cut -c1-160

bundle "$TMP/required" -e RAUC_REQUIRE_STABLE_KEY=1
[ "$?" -ne 0 ]
check "release build refuses to use throwaway keys" "$?" 0

if [ "$fail" -ne 0 ]; then echo "signing tests FAILED"; exit 1; fi
echo "All signing tests passed."
