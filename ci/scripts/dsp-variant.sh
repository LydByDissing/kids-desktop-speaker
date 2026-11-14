#!/bin/bash
# dsp-variant.sh — switch the speaker's DSP between variants, for A/B listening
# =============================================================================
# Usage: PI_HOST=root@host dsp-variant.sh VARIANT
#        (or: make -C ci dsp-variant PI_HOST=... VARIANT=no-vb)
#
# Writes the variant of ci/device/camilladsp.yml (see dsp-variant.py: installed,
# no-vb, no-duck, no-loudness, no-fill, flat), copies it to the speaker and has
# the running CamillaDSP load it, with a short break in the sound. Nothing is
# saved: VARIANT=installed, or restarting CamillaDSP, goes back to the
# installed config.

set -euo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
VARIANT=${1:?usage: dsp-variant.sh VARIANT}
REPO=$(cd "$(dirname "$0")/../.." && pwd)
PORT=${KDS_VARIANT_PORT:-12341}

if [ "$VARIANT" = installed ]; then
    path=/etc/camilladsp/config.yml
else
    path=/tmp/kds-dsp-$VARIANT.yml
    python3 "$REPO/ci/scripts/dsp-variant.py" "$VARIANT" | ssh "$PI_HOST" "cat > $path"
    ssh "$PI_HOST" "camilladsp --check $path" | grep -q "Config is valid" \
        || { echo "error: the speaker's CamillaDSP rejects variant $VARIANT" >&2; exit 1; }
fi

ssh -N -o ExitOnForwardFailure=yes -L "$PORT:127.0.0.1:1234" "$PI_HOST" &
tunnel=$!
trap 'kill $tunnel 2>/dev/null' EXIT
for _ in $(seq 20); do (exec 3<>"/dev/tcp/127.0.0.1/$PORT") 2>/dev/null && break; sleep 0.5; done
python3 "$REPO/ci/scripts/cdsp-ws.py" "127.0.0.1:$PORT" "{\"SetConfigFilePath\": \"$path\"}" Reload \
    | grep -q '"Reload":{"result":"Ok"}' || { echo "error: CamillaDSP did not load $path" >&2; exit 1; }
echo "==> The speaker plays variant '$VARIANT' ($path)."
