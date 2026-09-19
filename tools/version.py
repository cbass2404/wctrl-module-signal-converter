#!/usr/bin/env python3
"""Carry VERSION.md into every manifest that holds a version.

  python tools/version.py            # stamp
  python tools/version.py --check    # exit 1 if anything is out of step
  python tools/version.py --print    # the version, as the release names it
  python tools/version.py --semver   # the form the manifests hold

VERSION.md is the one place the version is set. The release tag, the installer
name and what the programs print all use it exactly as written.

Cargo, Tauri and npm all require semver, which forbids leading zeros in a
numeric pre-release part, so `1.0.0-alpha.001` is stamped into them as
`1.0.0-alpha.1`. The two forms compare the same, so nothing orders differently.
"""
import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CARGO = "Cargo.toml"
TAURI = "editor/src-tauri/tauri.conf.json"
NPM = "editor/package.json"
NPM_LOCK = "editor/package-lock.json"


def version():
    with open(os.path.join(ROOT, "VERSION.md"), encoding="utf-8") as f:
        text = f.read().strip()
    if not re.fullmatch(r"\d+\.\d+\.\d+(-[0-9A-Za-z.]+)?", text):
        sys.exit("VERSION.md holds %r, not a version" % text)
    return text


def semver(v):
    """Leading zeros dropped from each numeric pre-release part."""
    core, _, pre = v.partition("-")
    if not pre:
        return core
    parts = [str(int(p)) if p.isdigit() else p for p in pre.split(".")]
    return core + "-" + ".".join(parts)


def read(path):
    with open(os.path.join(ROOT, path), newline="", encoding="utf-8") as f:
        return f.read()


def write(path, text):
    with open(os.path.join(ROOT, path), "w", newline="", encoding="utf-8") as f:
        f.write(text)


# Each entry: file, the pattern whose group 2 is the version, how many matches.
# Text is edited in place rather than re-serialised, so formatting and line
# endings stay exactly as they were.
TARGETS = [
    (CARGO, r'(\[workspace\.package\][^\[]*?\nversion = ")([^"]*)(")', 1),
    (TAURI, r'(\n  "version": ")([^"]*)(")', 1),
    (NPM, r'(\n  "version": ")([^"]*)(")', 1),
    # The top level, and the root package under "packages".
    (NPM_LOCK, r'((?:\n  |"": \{\s*"name": "[^"]*",\s*)"version": ")([^"]*)(")', 2),
]


def found(path, pattern, count):
    matches = list(re.finditer(pattern, read(path)))
    if len(matches) != count:
        sys.exit("%s: expected %d version(s), found %d" % (path, count, len(matches)))
    return [m.group(2) for m in matches]


def main():
    ap = argparse.ArgumentParser()
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--print", action="store_true")
    mode.add_argument("--semver", action="store_true")
    args = ap.parse_args()

    v = version()
    want = semver(v)
    if args.print:
        print(v)
        return
    if args.semver:
        print(want)
        return

    stale = []
    for path, pattern, count in TARGETS:
        have = found(path, pattern, count)
        if any(h != want for h in have):
            stale.append((path, pattern, have))

    if args.check:
        for path, _, have in stale:
            print("%s holds %s, VERSION.md says %s" % (path, ", ".join(have), want))
        if stale:
            sys.exit("run: python tools/version.py")
        print("%s (%s) everywhere" % (v, want))
        return

    for path, pattern, have in stale:
        write(path, re.sub(pattern, lambda m: m.group(1) + want + m.group(3), read(path)))
        print("%s: %s -> %s" % (path, ", ".join(have), want))
    if any(path == CARGO for path, _, _ in stale):
        # Only the workspace's own entries in the lock file change.
        subprocess.check_call(["cargo", "update", "--workspace", "--quiet"], cwd=ROOT)
        print("Cargo.lock: workspace crates -> %s" % want)
    if not stale:
        print("%s (%s) everywhere already" % (v, want))


if __name__ == "__main__":
    main()
