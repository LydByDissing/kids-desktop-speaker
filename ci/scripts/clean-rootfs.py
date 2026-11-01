#!/usr/bin/env python3
"""Remove what `docker export` leaves behind, so the rootfs works on a device (DEC-025).

Usage: clean-rootfs.py <input.tar> <output.tar>

Docker adds /.dockerenv, which makes systemd think it runs in a container and skip
services such as clock sync. It also blanks resolv.conf, hostname and hosts.
"""
import io
import sys
import tarfile

RESOLV_TARGET = "/run/systemd/resolve/stub-resolv.conf"
HOSTS = b"127.0.0.1\tlocalhost\n127.0.1.1\tkidsspeaker\n::1\tlocalhost ip6-localhost ip6-loopback\n"
FILES = {"etc/hostname": b"kidsspeaker\n", "etc/hosts": HOSTS}


def clean(src_path, dst_path):
    with tarfile.open(src_path) as src, tarfile.open(dst_path, "w", format=tarfile.GNU_FORMAT) as dst:
        for member in src:
            name = member.name.removeprefix("./")
            if name == ".dockerenv":
                continue
            if name == "etc/resolv.conf":
                member.type = tarfile.SYMTYPE
                member.linkname = RESOLV_TARGET
                member.size = 0
                dst.addfile(member)
            elif name in FILES:
                data = FILES[name]
                member.size = len(data)
                member.mode = 0o644
                dst.addfile(member, io.BytesIO(data))
            else:
                dst.addfile(member, src.extractfile(member) if member.isreg() else None)


if __name__ == "__main__":
    clean(sys.argv[1], sys.argv[2])
