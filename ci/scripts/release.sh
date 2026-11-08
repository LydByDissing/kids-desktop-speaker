#!/bin/bash
# release.sh — tag a release once main is green, then verify what was published
# ==============================================================================
# Usage: release.sh <version> [--verify-only]
#   release.sh v0.1.0-rc.4        wait for main's CI, tag it, wait for the release
#                                 pipeline, then verify the published files
#   release.sh v0.1.0-rc.3 --verify-only
#                                 only verify an existing release
#
# Tags are pushed only after CI and Build Pi Image pass on origin/main. A tag with
# a pre-release suffix (-rc.N) is published as a GitHub prerelease, which devices
# ignore. Exits non-zero if anything fails or any release file is missing.
# Requires gh, authenticated for this repository.

set -uo pipefail

VERSION="${1:-}"
MODE="${2:-}"
REPO=$(cd "$(dirname "$0")/../.." && pwd)
POLL=30
# Every file a release must carry (DEC-023, DEC-026).
REQUIRED=(image.raucb manifest.json sbom.spdx.json licenses.tar.gz sources.tar)

log() { echo "[$(date +%H:%M:%S)] $*"; }
die() { log "error: $*"; exit 1; }

[[ "$VERSION" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$ ]] \
    || die "usage: $0 vX.Y.Z[-rc.N] [--verify-only]"
cd "$REPO"

# Waits for a workflow run to finish and prints its conclusion.
wait_for_run() { # run-id
    local status
    for _ in $(seq 1 240); do
        status=$(gh run view "$1" --json status --jq .status 2>/dev/null)
        [ "$status" = completed ] && break
        sleep "$POLL"
    done
    gh run view "$1" --json conclusion --jq .conclusion
}

verify_release() {
    local fail=0 assets base tmp
    log "Verifying release $VERSION"
    assets=$(gh release view "$VERSION" --json assets --jq '.assets[].name' 2>/dev/null) \
        || die "release $VERSION not found"
    for file in "${REQUIRED[@]}"; do
        if grep -qx "$file" <<<"$assets"; then
            log "  [PASS] has $file"
        else
            log "  [FAIL] missing $file"
            fail=1
        fi
    done
    [ "$fail" -eq 0 ] || return 1

    base="https://github.com/$(gh repo view --json nameWithOwner --jq .nameWithOwner)/releases/download/$VERSION"
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' RETURN
    curl -sfL -o "$tmp/manifest.json" "$base/manifest.json" || { log "  [FAIL] manifest download"; return 1; }
    curl -sfL -o "$tmp/sbom.json" "$base/sbom.spdx.json" || { log "  [FAIL] SBOM download"; return 1; }
    python3 - "$tmp/manifest.json" "$tmp/sbom.json" "$VERSION" <<'PY' || fail=1
import json, sys
manifest, sbom, version = json.load(open(sys.argv[1])), json.load(open(sys.argv[2])), sys.argv[3]
checks = [
    ("manifest names this version", manifest.get("version") == version),
    ("SBOM is SPDX", str(sbom.get("spdxVersion", "")).startswith("SPDX-2.")),
    ("SBOM lists packages", len(sbom.get("packages", [])) > 100),
]
for name, ok in checks:
    print(f"  [{'PASS' if ok else 'FAIL'}] {name}")
sys.exit(0 if all(ok for _, ok in checks) else 1)
PY
    local sources
    sources=$(curl -sfL "$base/sources.tar" | tar -t 2>/dev/null | grep -c '\.dsc$')
    if [ "${sources:-0}" -gt 0 ]; then log "  [PASS] sources.tar has $sources source packages"; else log "  [FAIL] sources.tar has no source packages"; fail=1; fi
    return "$fail"
}

if [ "$MODE" = "--verify-only" ]; then
    verify_release && { log "Release $VERSION is complete."; exit 0; }
    die "release $VERSION is incomplete"
fi
[ -z "$MODE" ] || die "unknown option: $MODE"

git fetch -q origin --tags || die "git fetch failed"
git rev-parse -q --verify "refs/tags/$VERSION" >/dev/null && die "tag $VERSION already exists"
COMMIT=$(git rev-parse origin/main)
log "Releasing $VERSION from origin/main ($(git log -1 --format='%h %s' "$COMMIT" | cut -c1-60))"

for workflow in ci.yml build-image.yml; do
    run=$(gh run list --commit "$COMMIT" --workflow "$workflow" --json databaseId --jq '.[0].databaseId')
    [ -n "$run" ] || die "no $workflow run for $COMMIT yet; push to main first"
    log "Waiting for $workflow on main (run $run)"
    conclusion=$(wait_for_run "$run")
    [ "$conclusion" = success ] || die "$workflow on main is $conclusion; not tagging"
done

git tag -a "$VERSION" -m "Release $VERSION" "$COMMIT" || die "tagging failed"
git push -q origin "$VERSION" || die "pushing the tag failed"
log "Pushed tag $VERSION"

sleep 20
for _ in $(seq 1 20); do
    run=$(gh run list --workflow release.yml --branch "$VERSION" --json databaseId --jq '.[0].databaseId')
    [ -n "$run" ] && break
    sleep 15
done
[ -n "$run" ] || die "no release run started for $VERSION"
log "Waiting for the release pipeline (run $run)"
conclusion=$(wait_for_run "$run")
[ "$conclusion" = success ] || { gh run view "$run" --log-failed 2>/dev/null | tail -30; die "release pipeline is $conclusion"; }

verify_release && { log "Release $VERSION is complete."; exit 0; }
die "release $VERSION is incomplete"
