#!/bin/bash
# kds-hat-init.sh — route the audio HAT's DAC to its speaker (KDS-ayx)
# ====================================================================
# The IQaudIO Pi-Codec Zero (DA7213 codec) starts with its outputs unrouted, so it
# plays nothing. These settings send the DAC, mixed to mono, to the line output that
# drives the speaker, with the headphone output off. Volume is controlled later in
# software (CamillaDSP); Lineout is left at a fixed level.

set -euo pipefail
CARD="${KDS_HAT_CARD:-IQaudIOCODEC}"

if ! amixer -c "$CARD" info >/dev/null 2>&1; then
    echo "kds-hat-init: sound card $CARD not found; is dtoverlay=iqaudio-codec set?" >&2
    exit 1
fi

set_control() { amixer -q -c "$CARD" cset name="$1" "$2"; }

set_control 'DAC Left Source MUX' 2        # DAI Input Left
set_control 'DAC Right Source MUX' 3       # DAI Input Right
# One speaker: CamillaDSP mixes left and right before its limiter and sends
# the same signal to both channels, so the codec mustn't add them again.
set_control 'DAC Mono Switch' off,off
set_control 'DAC Soft Mute Switch' off
set_control 'DAC Volume' 112,112
set_control 'Mixout Left DAC Left Switch' on
set_control 'Mixout Right DAC Right Switch' on
for input in 'Aux Left' 'Mixin Left' 'Mixin Right'; do
    set_control "Mixout Left $input Switch" off
done
for input in 'Aux Right' 'Mixin Right' 'Mixin Left'; do
    set_control "Mixout Right $input Switch" off
done
set_control 'Lineout Switch' on
# The amplifier's gain is the device volume (DEC-050): the Audio Player sets
# it at start and on every change. 48 (0 dB) until then.
set_control 'Lineout Volume' 48
set_control 'Headphone Switch' off,off
echo "kds-hat-init: speaker output enabled on $CARD"
