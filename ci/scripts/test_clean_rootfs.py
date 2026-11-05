#!/usr/bin/env python3
"""Tests for clean-rootfs.py. Run: python3 ci/scripts/test_clean_rootfs.py"""
import importlib.util
import io
import pathlib
import tarfile
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "clean_rootfs", pathlib.Path(__file__).with_name("clean-rootfs.py")
)
clean_rootfs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(clean_rootfs)


def add(tar, name, data=b"", mode=0o644):
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = mode
    tar.addfile(info, io.BytesIO(data))


class CleanRootfsTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        src, dst = pathlib.Path(tmp.name, "in.tar"), pathlib.Path(tmp.name, "out.tar")
        with tarfile.open(src, "w") as tar:
            add(tar, ".dockerenv")
            add(tar, "etc/resolv.conf")
            add(tar, "etc/hostname")
            add(tar, "etc/hosts")
            add(tar, "etc/machine-id", b"1278cb00626849cb8a226e307f448a55\n")
            add(tar, "var/lib/dbus/machine-id", b"1278cb00626849cb8a226e307f448a55\n")
            add(tar, "usr/bin/tool", b"binary", 0o755)
        clean_rootfs.clean(src, dst)
        self.out = tarfile.open(dst)
        self.addCleanup(self.out.close)

    def test_removes_docker_marker(self):
        self.assertNotIn(".dockerenv", self.out.getnames())

    def test_resolv_conf_points_to_resolved(self):
        member = self.out.getmember("etc/resolv.conf")
        self.assertTrue(member.issym())
        self.assertEqual(member.linkname, clean_rootfs.RESOLV_TARGET)

    def test_fills_hostname_and_hosts(self):
        self.assertEqual(self.out.extractfile("etc/hostname").read(), b"kidsspeaker\n")
        self.assertIn(b"localhost", self.out.extractfile("etc/hosts").read())

    def test_empties_machine_id(self):
        self.assertEqual(self.out.extractfile("etc/machine-id").read(), b"")

    def test_dbus_machine_id_follows_etc(self):
        member = self.out.getmember("var/lib/dbus/machine-id")
        self.assertTrue(member.issym())
        self.assertEqual(member.linkname, "/etc/machine-id")

    def test_keeps_other_files_and_modes(self):
        self.assertEqual(self.out.extractfile("usr/bin/tool").read(), b"binary")
        self.assertEqual(self.out.getmember("usr/bin/tool").mode, 0o755)


if __name__ == "__main__":
    unittest.main()
