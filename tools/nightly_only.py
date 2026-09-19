#!/usr/bin/env python3
"""Regenerate data/nightly-only.json against the latest stable DCS-BIOS.

  python tools/nightly_only.py [--tag v0.11.7]

A release step. The shipped defaults are written against a DCS-BIOS nightly,
and most users run the stable release, so each release of wctrl ships the
short list of signals the defaults read that the current stable lacks or
reports differently. The editor uses it to tell a user which lamps need the
nightly, so they can decide whether to update.

Downloads the stable release from GitHub into a temporary folder and runs
`wctrl nightly-only` against it. The nightly side is the catalogue built from
the DCS-BIOS installed on this machine, which should be the nightly the
defaults were written against.
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import urllib.request
import zipfile

REPO = "DCS-Skunkworks/dcs-bios"


def release(tag):
    url = ("https://api.github.com/repos/%s/releases/%s"
           % (REPO, "tags/" + tag if tag else "latest"))
    req = urllib.request.Request(url, headers={"Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req) as resp:
        return json.load(resp)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--tag", help="a stable tag instead of the latest, e.g. v0.11.7")
    args = ap.parse_args()

    rel = release(args.tag)
    assets = [a for a in rel["assets"]
              if a["name"].startswith("DCS-BIOS") and a["name"].endswith(".zip")]
    if not assets:
        sys.exit("release %s has no DCS-BIOS zip" % rel["tag_name"])
    asset = assets[0]
    print("stable %s: %s" % (rel["tag_name"], asset["name"]), flush=True)

    with tempfile.TemporaryDirectory() as tmp:
        archive = os.path.join(tmp, asset["name"])
        urllib.request.urlretrieve(asset["browser_download_url"], archive)
        with zipfile.ZipFile(archive) as z:
            z.extractall(tmp)
        stable = os.path.join(tmp, "DCS-BIOS", "doc", "json")
        if not os.path.isdir(stable):
            sys.exit("%s has no DCS-BIOS/doc/json" % asset["name"])
        cmd = ["cargo", "run", "--quiet", "--bin", "wctrl", "--",
               "nightly-only", "--stable", stable]
        sys.exit(subprocess.call(cmd))


if __name__ == "__main__":
    main()
