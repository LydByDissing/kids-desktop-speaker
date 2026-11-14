#!/bin/bash
# dsp-tune.sh — tune the speaker's DSP live from this computer (KDS-9jt.6)
# =======================================================================
# Usage: PI_HOST=root@host dsp-tune.sh      (or: make -C ci dsp-tune PI_HOST=...)
#
# Runs CamillaGUI here, connected to the speaker's CamillaDSP through an SSH
# tunnel, so filters can be changed and heard at once. Open the printed URL,
# edit the filters and press Apply: the speaker plays with the change right
# away. Nothing on the speaker is saved: stopping the GUI (or restarting the
# speaker) goes back to the installed config. To keep a tuning, save it in the
# GUI (it lands in ci/dsp/work/configs/) and copy it to ci/device/camilladsp.yml.
# CamillaGUI doesn't know our VirtualBass filter, so the speaker plays the
# config without it while tuning (dsp-variant.sh no-vb).
#
# The GUI release matches the speaker's CamillaDSP (4.1.x) and is pinned by
# SHA-256. CamillaDSP listens on the speaker's localhost only; the tunnel is
# the only way in.

set -euo pipefail

: "${PI_HOST:?set PI_HOST=root@host}"
VERSION=v4.1.0
SHA256=86fd3cde575038f312ede7bad0910dc5e46b974cafc048c26115ec3cb9f54792
LOCAL_PORT=${KDS_DSP_PORT:-12340}
GUI_PORT=${KDS_GUI_PORT:-5005}

REPO=$(cd "$(dirname "$0")/../.." && pwd)
DIR="$REPO/ci/dsp"
GUI="$DIR/camillagui"
WORK="$DIR/work"

if [ "$(uname -sm)" != "Linux x86_64" ]; then
    echo "error: this fetches CamillaGUI for Linux x86_64; on $(uname -sm), get the matching bundle_*" >&2
    echo "       from https://github.com/HEnquist/camillagui-backend/releases/tag/$VERSION" >&2
    exit 2
fi

if [ "$(cat "$GUI/version" 2>/dev/null)" != "$VERSION $SHA256" ]; then
    echo "==> Downloading CamillaGUI $VERSION..."
    rm -rf "$GUI" && mkdir -p "$GUI"
    curl -sSfL --retry 3 -o "$GUI/bundle.tar.gz" \
        "https://github.com/HEnquist/camillagui-backend/releases/download/$VERSION/bundle_linux_amd64.tar.gz"
    echo "$SHA256  $GUI/bundle.tar.gz" | sha256sum -c --quiet - \
        || { echo "error: CamillaGUI $VERSION does not match the pinned SHA-256" >&2; exit 1; }
    tar -xzf "$GUI/bundle.tar.gz" -C "$GUI"
    rm "$GUI/bundle.tar.gz"
    echo "$VERSION $SHA256" > "$GUI/version"
fi

mkdir -p "$WORK/configs" "$WORK/coeffs"
# CamillaGUI only knows upstream's filter types, so our VirtualBass (DEC-052)
# is left out while tuning: the speaker plays the no-vb variant, restored to
# the installed config when the GUI stops.
gui_config=installed
if grep -q "type: VirtualBass" "$REPO/ci/device/camilladsp.yml"; then
    gui_config=no-vb
    echo "==> CamillaGUI doesn't know VirtualBass: playing variant no-vb while tuning."
    bash "$REPO/ci/scripts/dsp-variant.sh" no-vb
fi
# Start from what the speaker plays, so the GUI shows it.
python3 "$REPO/ci/scripts/dsp-variant.py" "$gui_config" > "$WORK/configs/speaker.yml"
cp "$WORK/configs/speaker.yml" "$WORK/default_config.yml"
cat > "$WORK/camillagui.yml" <<EOF
---
camilla_host: "127.0.0.1"
camilla_port: $LOCAL_PORT
bind_address: "127.0.0.1"
port: $GUI_PORT
ssl_certificate: null
ssl_private_key: null
gui_config_file: null
config_dir: "$WORK/configs"
coeff_dir: "$WORK/coeffs"
default_config: "$WORK/default_config.yml"
statefile_path: "$WORK/statefile.yml"
log_file: "$WORK/camilladsp.log"
on_set_active_config: null
on_get_active_config: null
supported_capture_types: null
supported_playback_types: null
EOF

echo "==> Tunnelling localhost:$LOCAL_PORT to CamillaDSP on $PI_HOST..."
ssh -N -o ExitOnForwardFailure=yes -L "$LOCAL_PORT:127.0.0.1:1234" "$PI_HOST" &
tunnel=$!
restore() {
    kill $tunnel 2>/dev/null
    if [ "$gui_config" != installed ]; then
        echo "==> Back to the installed DSP config."
        bash "$REPO/ci/scripts/dsp-variant.sh" installed || true
    fi
}
trap restore EXIT
sleep 2
kill -0 "$tunnel" 2>/dev/null || { echo "error: the SSH tunnel did not start" >&2; exit 1; }

echo "==> CamillaGUI on http://localhost:$GUI_PORT (Ctrl-C to stop)"
echo "    Apply changes the speaker at once; saved configs go to $WORK/configs."
"$GUI/camillagui_backend/camillagui_backend" -c "$WORK/camillagui.yml"
