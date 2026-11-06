#!/usr/bin/env python3
"""Offline tests for collect-sources.py. Run: python3 ci/scripts/test_collect_sources.py"""
import importlib.util
import pathlib
import unittest

spec = importlib.util.spec_from_file_location(
    "collect_sources", pathlib.Path(__file__).with_name("collect-sources.py")
)
cs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cs)

SOURCES = """Package: bluez
Version: 5.66-1+deb12u2
Directory: pool/main/b/bluez
Checksums-Sha256:
 aaa 100 bluez_5.66-1+deb12u2.dsc
 bbb 2000 bluez_5.66.orig.tar.xz

Package: libgpg-error
Version: 1.46-1
Directory: pool/main/libg/libgpg-error
Checksums-Sha256:
 ccc 300 libgpg-error_1.46-1.dsc
"""


class CollectSourcesTest(unittest.TestCase):
    def test_parses_paragraphs_with_multiline_fields(self):
        paras = list(cs.parse_paragraphs(SOURCES))
        self.assertEqual([p["Package"] for p in paras], ["bluez", "libgpg-error"])
        self.assertEqual(
            cs.checksums(paras[0]),
            [("bluez_5.66-1+deb12u2.dsc", 100, "aaa"), ("bluez_5.66.orig.tar.xz", 2000, "bbb")],
        )

    def test_pool_directory_uses_lib_prefix(self):
        self.assertEqual(cs.pool_dir("bluez"), "pool/main/b/bluez")
        self.assertEqual(cs.pool_dir("libgpg-error"), "pool/main/libg/libgpg-error")

    def test_checksums_ignore_malformed_lines(self):
        self.assertEqual(cs.checksums({"Checksums-Sha256": "\nbad line\n abc 5 f.dsc"}), [("f.dsc", 5, "abc")])


if __name__ == "__main__":
    unittest.main()
