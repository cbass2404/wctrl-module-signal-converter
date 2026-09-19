#!/usr/bin/env python3
"""Fetch the DCS-BIOS nightly the shipped defaults are written against.

  python tools/fetch_bios.py [--zip PATH] [--build-catalogue]

A pipeline step. DCS-BIOS publishes nightlies as one rolling `latest`
pre-release, and each nightly replaces the last, so the one the defaults were
written against cannot be fetched from DCS-BIOS again. We keep our own copy as
an asset on a release in this repository, named in `tools/dcs-bios-pin.json`
with its SHA-256. This downloads it, checks the hash and the version inside,
and unpacks it to `target/dcs-bios-pin`.

`--build-catalogue` then builds `data/catalogue` from it, which the tests and
`tools/nightly_only.py` read. The catalogue remembers where it was built from,
so on a development machine this repoints it away from the DCS-BIOS installed
in Saved Games; `dcs-signal --bios <that doc/json> catalogue --rebuild` puts it
back.

`--zip` checks and unpacks a local copy instead of downloading, for pinning a
new nightly: hash the zip, update the pin, attach the zip to a release named
by `release`, and run this against the local copy to prove the pin matches.
"""
import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import urllib.request
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
REPO = "cbass2404/wctrl-module-signal-converter"
OUT = os.path.join(ROOT, "target", "dcs-bios-pin")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--zip", help="a local copy of the pinned zip instead of downloading")
    ap.add_argument("--build-catalogue", action="store_true",
                    help="build data/catalogue from it afterwards")
    args = ap.parse_args()

    with open(os.path.join(ROOT, "tools", "dcs-bios-pin.json"), encoding="utf-8") as f:
        pin = json.load(f)

    shutil.rmtree(OUT, ignore_errors=True)
    os.makedirs(OUT)
    archive = args.zip
    if not archive:
        archive = os.path.join(OUT, pin["asset"])
        url = "https://github.com/%s/releases/download/%s/%s" % (REPO, pin["release"], pin["asset"])
        print("fetching %s" % url, flush=True)
        urllib.request.urlretrieve(url, archive)

    have = sha256(archive)
    if have != pin["sha256"]:
        sys.exit("%s has SHA-256 %s, the pin says %s" % (archive, have, pin["sha256"]))

    with zipfile.ZipFile(archive) as z:
        z.extractall(OUT)
    config = os.path.join(OUT, "DCS-BIOS", "BIOSConfig.lua")
    with open(config, encoding="utf-8") as f:
        found = re.search(r'version\s*=\s*"([^"]+)"', f.read())
    if not found or found.group(1) != pin["version"]:
        sys.exit("%s says %s, the pin says %s"
                 % (config, found.group(1) if found else "no version", pin["version"]))

    bios_json = os.path.join(OUT, "DCS-BIOS", "doc", "json")
    print("DCS-BIOS %s at %s" % (pin["version"], bios_json), flush=True)

    if args.build_catalogue:
        cmd = ["cargo", "run", "--quiet", "--locked", "--bin", "dcs-signal", "--",
               "--bios", bios_json, "catalogue", "--rebuild"]
        # DSC_DATA names the checkout's `data` outright: a Tauri build copies
        # `data/devices.json` beside `target/*/dcs-signal.exe`, which then
        # takes itself for an installed copy and would build elsewhere.
        env = dict(os.environ, DSC_DATA=os.path.join(ROOT, "data"))
        out = subprocess.run(cmd, cwd=ROOT, env=env, capture_output=True, text=True)
        # Only the first line: the rest is a summary of every module.
        if out.returncode != 0:
            sys.stderr.write(out.stdout + out.stderr)
            sys.exit(out.returncode)
        print(out.stdout.splitlines()[0] if out.stdout else "catalogue built")


if __name__ == "__main__":
    main()
