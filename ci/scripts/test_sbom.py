#!/usr/bin/env python3
"""Check a release SBOM against the image (DEC-026).

Usage: test_sbom.py <sbom.spdx.json> <rootfs.tar>
"""
import json
import sys
import tarfile
import unittest

SBOM_PATH, ROOTFS_PATH = sys.argv[1], sys.argv[2]


def installed_packages(rootfs):
    with tarfile.open(rootfs) as tar:
        for name in ("./var/lib/dpkg/status", "var/lib/dpkg/status"):
            try:
                status = tar.extractfile(name).read().decode()
                break
            except KeyError:
                continue
    names = set()
    for block in status.split("\n\n"):
        fields = dict(l.split(": ", 1) for l in block.splitlines() if ": " in l and not l.startswith(" "))
        if fields.get("Status", "").endswith("installed"):
            names.add(fields["Package"])
    return names


class SbomTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with open(SBOM_PATH) as f:
            cls.sbom = json.load(f)
        cls.packages = cls.sbom["packages"]
        cls.debs = {p["name"] for p in cls.packages if "pkg:deb/" in json.dumps(p.get("externalRefs", []))}
        cls.crates = {p["name"] for p in cls.packages if "pkg:cargo/" in json.dumps(p.get("externalRefs", []))}

    def test_is_spdx(self):
        self.assertTrue(self.sbom["spdxVersion"].startswith("SPDX-2."))

    def test_lists_every_installed_debian_package(self):
        missing = installed_packages(ROOTFS_PATH) - self.debs
        self.assertEqual(missing, set())

    def test_lists_rust_crates(self):
        for crate in ("reqwest", "rustls", "tokio", "axum"):
            self.assertIn(crate, self.crates)

    def test_packages_have_versions(self):
        unversioned = [p["name"] for p in self.packages if not p.get("versionInfo")]
        self.assertEqual(unversioned[:5], [])

    def test_debian_packages_carry_licenses(self):
        bluez = [p for p in self.packages if p["name"] == "bluez"]
        self.assertTrue(bluez)
        self.assertNotIn(bluez[0].get("licenseDeclared", "NOASSERTION"), ("NOASSERTION", ""))


if __name__ == "__main__":
    unittest.main(argv=sys.argv[:1])
