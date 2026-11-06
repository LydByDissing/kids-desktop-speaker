#!/usr/bin/env python3
"""Download the source packages behind a release's GPL and LGPL binaries (DEC-026).

Usage: collect-sources.py <packages.tsv> <output-dir> [--all]

packages.tsv comes from license-inventory.sh. Each source package is fetched at
the exact version in the image, from the Raspbian and Raspberry Pi archives, and
checked against the archive's SHA-256. A version no longer in an archive's index
is looked up in its package pool. Exits non-zero if any source can't be fetched,
so a release never ships binaries whose source we can't supply.
"""
import argparse
import gzip
import hashlib
import lzma
import os
import sys
import urllib.error
import urllib.request

ARCHIVES = [
    ("http://archive.raspbian.org/raspbian", "dists/bookworm/main/source/Sources.xz"),
    ("http://archive.raspberrypi.com/debian", "dists/bookworm/main/source/Sources.gz"),
]
COPYLEFT = ("GPL", "LGPL", "AGPL")


def fetch(url):
    with urllib.request.urlopen(url, timeout=120) as r:
        return r.read()


def parse_paragraphs(text):
    """Yields Debian control paragraphs as dicts, keeping multi-line fields."""
    para, key = {}, None
    for line in text.splitlines():
        if not line.strip():
            if para:
                yield para
            para, key = {}, None
        elif line[0] in " \t" and key:
            para[key] += "\n" + line.strip()
        else:
            key, _, value = line.partition(":")
            para[key] = value.strip()
    if para:
        yield para


def checksums(para):
    """Returns (name, size, sha256) for each file a source package lists."""
    out = []
    for line in para.get("Checksums-Sha256", "").splitlines():
        parts = line.split()
        if len(parts) == 3:
            sha, size, name = parts
            out.append((name, int(size), sha))
    return out


def load_indexes():
    index = {}
    for base, path in ARCHIVES:
        raw = fetch(f"{base}/{path}")
        text = (lzma.decompress(raw) if path.endswith(".xz") else gzip.decompress(raw)).decode("utf-8", "replace")
        for para in parse_paragraphs(text):
            index.setdefault((para.get("Package"), para.get("Version")), (base, para))
    return index


def pool_dir(source):
    prefix = source[:4] if source.startswith("lib") else source[0]
    return f"pool/main/{prefix}/{source}"


def find_in_pool(source, version):
    """Looks for a .dsc that an index no longer lists. Returns (base, paragraph)."""
    plain = version.split(":", 1)[-1]
    for base, _ in ARCHIVES:
        url = f"{base}/{pool_dir(source)}/{source}_{plain}.dsc"
        try:
            dsc = fetch(url).decode("utf-8", "replace")
        except urllib.error.URLError:
            continue
        # A .dsc may be signed: drop the signature armour around the paragraph.
        body = dsc.split("\n\n", 1)[1] if dsc.startswith("-----BEGIN PGP") else dsc
        body = body.split("\n-----BEGIN PGP SIGNATURE", 1)[0]
        para = next(parse_paragraphs(body))
        para["Directory"] = pool_dir(source)
        files = checksums(para)
        files.append((f"{source}_{plain}.dsc", len(dsc.encode()), hashlib.sha256(dsc.encode()).hexdigest()))
        para["_files"] = files
        return base, para
    return None


def download(base, para, dest):
    files = para.get("_files") or checksums(para)
    os.makedirs(dest, exist_ok=True)
    for name, size, sha in files:
        target = os.path.join(dest, name)
        if os.path.exists(target) and os.path.getsize(target) == size:
            continue
        data = fetch(f"{base}/{para['Directory']}/{name}")
        if hashlib.sha256(data).hexdigest() != sha:
            raise ValueError(f"checksum mismatch for {name}")
        with open(target, "wb") as f:
            f.write(data)
    return sum(size for _, size, _ in files)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("packages_tsv")
    parser.add_argument("output_dir")
    parser.add_argument("--all", action="store_true", help="include permissively licensed sources")
    args = parser.parse_args()

    wanted = {}
    with open(args.packages_tsv) as f:
        next(f)
        for line in f:
            _, _, source, source_version, licenses = line.rstrip("\n").split("\t")
            if args.all or any(tag in licenses for tag in COPYLEFT):
                wanted[(source, source_version)] = licenses

    index = load_indexes()
    missing, total = [], 0
    for (source, version) in sorted(wanted):
        found = index.get((source, version)) or find_in_pool(source, version)
        if not found:
            missing.append(f"{source} {version}")
            continue
        total += download(*found, os.path.join(args.output_dir, f"{source}_{version.split(':', 1)[-1]}"))

    print(f"==> {len(wanted) - len(missing)} of {len(wanted)} source packages, {total / 1e6:.0f} MB")
    if missing:
        print("error: source not found for: " + ", ".join(missing), file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
