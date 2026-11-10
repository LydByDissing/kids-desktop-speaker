# Cross image for building librespot (ci/scripts/build-librespot.sh, DEC-034):
# cross's ARMv6 image plus the ALSA headers the ALSA backend links against.
# The Pi's own ARMv6 libasound2 is used at run time.
FROM ghcr.io/cross-rs/arm-unknown-linux-gnueabihf:0.2.5
RUN dpkg --add-architecture armhf \
    && apt-get update \
    && apt-get install --assume-yes --no-install-recommends libasound2-dev:armhf pkg-config \
    && rm -rf /var/lib/apt/lists/*
ENV PKG_CONFIG_ALLOW_CROSS=1 \
    PKG_CONFIG_PATH=/usr/lib/arm-linux-gnueabihf/pkgconfig
