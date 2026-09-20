#!/usr/bin/env python3
"""Keep data/defaults-previous holding the defaults the last release shipped.

  python tools/snapshot.py --check    does it still match the last tag
  python tools/snapshot.py            refresh it from data/defaults

A release step, run by tools/release.cmd at both ends. The daemon corrects a
display field an update changed only while that field is still exactly what the
last release shipped, and anything else it leaves alone as the user's. The
snapshot is the only record of what that was, so if it drifts, corrections
quietly stop reaching anybody. Safely, but silently, which is why the check
exists.

The order inside a release is the part worth understanding. A release of N has
to ship the defaults of N-1 as its snapshot, because that is what the people
upgrading are coming from. So the check runs before the tag, confirming the
snapshot still holds N-1, and the refresh runs after the push, setting it to N
for the release after this one. That leaves the copy unstaged on purpose: until
the pipeline is green there is nothing worth committing, and discarding it costs
nothing.

Compared as parsed JSON rather than as text, which is what the daemon does
too: `replace` and `aliases` are hash maps whose key order on disk is
arbitrary, and line endings are git's business.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CURRENT = os.path.join(ROOT, "data", "defaults")
SNAPSHOT = os.path.join(ROOT, "data", "defaults-previous")


def git(*args):
    """Run git in the repo and hand back stdout, or None if it failed."""
    try:
        out = subprocess.run(
            ["git", "-C", ROOT] + list(args),
            capture_output=True,
            check=True,
        )
    except (subprocess.CalledProcessError, OSError):
        return None
    return out.stdout


def last_release_tag():
    """The newest released version tag.

    Filtered to `v*`, because the repo also tags the DCS-BIOS nightly the
    defaults were written against and that is not a release of this project.
    At release time the tag being cut does not exist yet, which release.cmd
    has already checked, so the newest here is the previous release.
    """
    out = git("tag", "--list", "v*", "--sort=-v:refname")
    if out is None:
        return None
    for line in out.decode("utf-8", "replace").splitlines():
        line = line.strip()
        if line:
            return line
    return None


def names(folder):
    if not os.path.isdir(folder):
        return set()
    return {n for n in os.listdir(folder) if n.endswith(".json")}


def load(path):
    with open(path, "rb") as f:
        return json.loads(f.read().decode("utf-8-sig"))


def at_tag(tag, name):
    """One default as that tag shipped it, or None if it had none."""
    out = git("show", "%s:data/defaults/%s" % (tag, name))
    if out is None:
        return None
    return json.loads(out.decode("utf-8-sig"))


def check():
    """Whether the snapshot still holds what the last release shipped."""
    tag = last_release_tag()
    if tag is None:
        print("  no released tag yet, so there is nothing to check against.")
        return 0

    if not os.path.isdir(SNAPSHOT):
        print("ERROR: data/defaults-previous is missing.")
        print("  It should hold the defaults as %s shipped them." % tag)
        return 1

    out = git("ls-tree", "--name-only", "%s:data/defaults" % tag)
    if out is None:
        print("ERROR: could not read data/defaults out of %s." % tag)
        return 1
    want = {
        n.strip()
        for n in out.decode("utf-8", "replace").splitlines()
        if n.strip().endswith(".json")
    }
    have = names(SNAPSHOT)

    wrong = []
    for name in sorted(want - have):
        wrong.append("missing from the snapshot: %s" % name)
    for name in sorted(have - want):
        wrong.append("in the snapshot but not in %s: %s" % (tag, name))
    for name in sorted(want & have):
        try:
            if at_tag(tag, name) != load(os.path.join(SNAPSHOT, name)):
                wrong.append("differs from %s: %s" % (tag, name))
        except (OSError, ValueError) as e:
            wrong.append("could not be read: %s (%s)" % (name, e))

    if wrong:
        print("ERROR: data/defaults-previous does not match %s:" % tag)
        print("")
        for line in wrong:
            print("    %s" % line)
        print("")
        print("  Updates correct a field only while it still matches what the")
        print("  last release shipped, so a snapshot that has drifted means no")
        print("  correction reaches anybody. Refresh it with:")
        print("      python tools\\snapshot.py")
        return 1

    print("  ok, data/defaults-previous matches %s (%d file(s))." % (tag, len(want)))
    return 0


def refresh():
    """Copy the current defaults over the snapshot."""
    if not os.path.isdir(CURRENT):
        print("ERROR: no data/defaults to snapshot.")
        return 1
    if os.path.isdir(SNAPSHOT):
        shutil.rmtree(SNAPSHOT)
    shutil.copytree(CURRENT, SNAPSHOT)
    print("  data/defaults-previous refreshed from data/defaults (%d file(s))."
          % len(names(SNAPSHOT)))
    return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--check",
        action="store_true",
        help="verify the snapshot against the last release tag, and change nothing",
    )
    args = ap.parse_args()
    return check() if args.check else refresh()


if __name__ == "__main__":
    sys.exit(main())
