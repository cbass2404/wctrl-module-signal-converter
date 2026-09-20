#!/usr/bin/env python3
"""Carry VERSION.md into every manifest that holds a version.

  python tools/version.py                     # stamp
  python tools/version.py --check             # exit 1 if anything is out of step
  python tools/version.py --check --ref HEAD  # check a commit, not the tree
  python tools/version.py --print             # the version, as the release names it
  python tools/version.py --semver            # the form the manifests hold

VERSION.md is the one place the version is set. The release tag, the installer
name and what the programs print all use it exactly as written.

Cargo, Tauri and npm all require semver, which forbids leading zeros in a
numeric pre-release part, so `1.0.0-alpha.001` is stamped into them as
`1.0.0-alpha.1`. The two forms compare the same, so nothing orders differently.

`--check` is the whole audit, not a sample of it, and prints every place it
looked. Cargo.lock is included: `cargo --locked` fails on a lock that disagrees
with the manifests, which is a version mismatch found late in a build rather
than before the tag. The workspace crates come from Cargo.toml, so a crate
added later is covered without editing this file.

`--ref` reads the files out of a commit instead of the working tree, for
`--check` and for `--print`. A release is built from a tag, so what that
commit holds is what has to agree: a stamp that was run but never committed
passes a working-tree check and then fails in the pipeline, once the tag
already exists, and `--print --ref` names the release the way the pipeline
will name it rather than the way the tree does.
"""
import argparse
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

CARGO = "Cargo.toml"
CARGO_LOCK = "Cargo.lock"
TAURI = "editor/src-tauri/tauri.conf.json"
NPM = "editor/package.json"
NPM_LOCK = "editor/package-lock.json"


def read(path, ref=None):
    if ref is None:
        with open(os.path.join(ROOT, path), newline="", encoding="utf-8") as f:
            return f.read()
    git = subprocess.run(
        ["git", "show", "%s:%s" % (ref, path)],
        cwd=ROOT, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if git.returncode != 0:
        sys.exit("%s: not in %s (%s)" % (
            path, ref, git.stderr.decode("utf-8", "replace").strip()))
    return git.stdout.decode("utf-8")


def write(path, text):
    with open(os.path.join(ROOT, path), "w", newline="", encoding="utf-8") as f:
        f.write(text)


def version(ref=None):
    text = read("VERSION.md", ref).strip()
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

# A crate that sets its own version rather than inheriting the workspace one.
OWN_VERSION = r'(\[package\][^\[]*?\nversion = ")([^"]*)(")'


def found(path, pattern, count, ref=None):
    matches = list(re.finditer(pattern, read(path, ref)))
    if len(matches) != count:
        sys.exit("%s: expected %d version(s), found %d" % (path, count, len(matches)))
    return [m.group(2) for m in matches]


def members(ref=None):
    """(manifest, crate name) for every workspace member, from Cargo.toml."""
    listed = re.search(r"\[workspace\][^\[]*?members = \[(.*?)\]", read(CARGO, ref), re.S)
    if not listed:
        sys.exit("%s: no [workspace] members list" % CARGO)
    out = []
    for path in re.findall(r'"([^"]+)"', listed.group(1)):
        manifest = path + "/Cargo.toml"
        name = re.search(r'\[package\][^\[]*?\nname = "([^"]+)"', read(manifest, ref))
        if not name:
            sys.exit("%s: no [package] name" % manifest)
        out.append((manifest, name.group(1)))
    return out


def targets(ref=None):
    """Every file this stamps. The members are whatever Cargo.toml lists, so a
    new crate is covered; one that inherits the version holds none of its own."""
    out = list(TARGETS)
    for manifest, _ in members(ref):
        text = read(manifest, ref)
        if re.search(r"\[package\][^\[]*?\nversion\.workspace = true", text):
            continue
        if not re.search(OWN_VERSION, text):
            sys.exit("%s: [package] sets no version; use version.workspace = true" % manifest)
        out.append((manifest, OWN_VERSION, 1))
    return out


def lock(names, ref=None):
    """What Cargo.lock records for each workspace crate, in the same order."""
    text = read(CARGO_LOCK, ref)
    out = []
    for name in names:
        held = re.search(
            r'\[\[package\]\]\r?\nname = "%s"\r?\nversion = "([^"]*)"' % re.escape(name),
            text,
        )
        if not held:
            sys.exit("%s: no entry for %s" % (CARGO_LOCK, name))
        out.append(held.group(1))
    return out


def report(where, have, want):
    """One audit line. Returns True when this place is out of step."""
    seen = sorted(set(have))
    stale = any(h != want for h in seen)
    print("  %-5s %-36s %s" % ("STALE" if stale else "ok", where, ", ".join(seen)))
    return stale


def check(v, want, ref):
    print("%s (%s) in %s:" % (v, want, "the working tree" if ref is None else ref))
    stale = False
    for path, pattern, count in targets(ref):
        stale |= report(path, found(path, pattern, count, ref), want)

    crates = [name for _, name in members(ref)]
    held = lock(crates, ref)
    stale |= report("%s (%d crates)" % (CARGO_LOCK, len(crates)), held, want)
    if len(set(held)) > 1:
        # Only worth naming when they disagree with each other; all six being
        # stale together says nothing the line above has not already said.
        for name, h in zip(crates, held):
            print("        %-20s %s" % (name, h))

    if stale:
        print()
        sys.exit("out of step with VERSION.md. Run: python tools/version.py")


def stamp(v, want):
    stale = []
    for path, pattern, count in targets():
        have = found(path, pattern, count)
        if any(h != want for h in have):
            stale.append((path, pattern, have))

    for path, pattern, have in stale:
        write(path, re.sub(pattern, lambda m: m.group(1) + want + m.group(3), read(path)))
        print("%s: %s -> %s" % (path, ", ".join(have), want))

    crates = [name for _, name in members()]
    locked = [h for h in lock(crates) if h != want]
    if stale or locked:
        # Only the workspace's own entries in the lock file change. Run even
        # when just the lock drifted: `cargo --locked` fails on that alone.
        subprocess.check_call(["cargo", "update", "--workspace", "--quiet"], cwd=ROOT)
        print("%s: workspace crates -> %s" % (CARGO_LOCK, want))
    if not stale and not locked:
        print("%s (%s) everywhere already" % (v, want))


def main():
    ap = argparse.ArgumentParser()
    mode = ap.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true")
    mode.add_argument("--print", action="store_true")
    mode.add_argument("--semver", action="store_true")
    ap.add_argument("--ref", metavar="COMMIT",
                    help="read this commit instead of the working tree")
    args = ap.parse_args()

    if args.ref and not (args.check or args.print or args.semver):
        sys.exit("--ref reads a commit; stamping writes the working tree")

    v = version(args.ref)
    want = semver(v)
    if args.print:
        print(v)
    elif args.semver:
        print(want)
    elif args.check:
        check(v, want, args.ref)
    else:
        stamp(v, want)


if __name__ == "__main__":
    main()
