#!/usr/bin/env python3
"""Keep the -previous folders holding what the last release shipped.

  python tools/snapshot.py --check    do they still match the last tag
  python tools/snapshot.py            refresh them from what ships now

Two pairs, handled alike: data/defaults into data/defaults-previous, and the
pages, data/default-pages into data/default-pages-previous. An update
reconciles the profiles and the pages apart, each against its own snapshot.

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

Only the .json files are the snapshot. Anything else in a folder, such as the
README saying what it is for, is left where it is by a refresh.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# What ships, and the snapshot of it the next release compares against, as
# paths under data/.
PAIRS = [
    ("defaults", "defaults-previous"),
    ("default-pages", "default-pages-previous"),
]


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


def at_tag(tag, current, name):
    """One file of `current` as that tag shipped it, or None if it had none."""
    out = git("show", "%s:data/%s/%s" % (tag, current, name))
    if out is None:
        return None
    return json.loads(out.decode("utf-8-sig"))


def shipped_at(tag, current):
    """The .json files `current` held at that tag. Empty for a folder the tag
    did not have, which is every release before it was added: that release
    shipped nothing there, so its snapshot is rightly empty."""
    out = git("ls-tree", "--name-only", "%s:data/%s" % (tag, current))
    if out is None:
        return set()
    return {
        n.strip()
        for n in out.decode("utf-8", "replace").splitlines()
        if n.strip().endswith(".json")
    }


def check_pair(tag, current, previous):
    """Whether data/<previous> still holds what data/<current> was at `tag`."""
    snapshot = os.path.join(ROOT, "data", previous)
    want = shipped_at(tag, current)
    if not os.path.isdir(snapshot):
        if not want:
            print("  ok, %s shipped nothing in data/%s, and there is no snapshot of it." % (tag, current))
            return 0
        print("ERROR: data/%s is missing." % previous)
        print("  It should hold data/%s as %s shipped it." % (current, tag))
        return 1
    have = names(snapshot)

    wrong = []
    for name in sorted(want - have):
        wrong.append("missing from the snapshot: %s" % name)
    for name in sorted(have - want):
        wrong.append("in the snapshot but not in %s: %s" % (tag, name))
    for name in sorted(want & have):
        try:
            if at_tag(tag, current, name) != load(os.path.join(snapshot, name)):
                wrong.append("differs from %s: %s" % (tag, name))
        except (OSError, ValueError) as e:
            wrong.append("could not be read: %s (%s)" % (name, e))

    if wrong:
        print("ERROR: data/%s does not match %s:" % (previous, tag))
        print("")
        for line in wrong:
            print("    %s" % line)
        print("")
        print("  Updates correct a field only while it still matches what the")
        print("  last release shipped, so a snapshot that has drifted means no")
        print("  correction reaches anybody. Refresh it with:")
        print("      python tools\\snapshot.py")
        return 1

    print("  ok, data/%s matches %s (%d file(s))." % (previous, tag, len(want)))
    return 0


def check():
    """Whether every snapshot still holds what the last release shipped."""
    tag = last_release_tag()
    if tag is None:
        print("  no released tag yet, so there is nothing to check against.")
        return 0
    failed = 0
    for current, previous in PAIRS:
        failed |= check_pair(tag, current, previous)
    return failed


def refresh_pair(current, previous):
    """Put the .json files of data/<current> in data/<previous>, and only them."""
    source = os.path.join(ROOT, "data", current)
    snapshot = os.path.join(ROOT, "data", previous)
    if not os.path.isdir(source):
        print("ERROR: no data/%s to snapshot." % current)
        return 1
    os.makedirs(snapshot, exist_ok=True)
    for name in names(snapshot):
        os.remove(os.path.join(snapshot, name))
    for name in names(source):
        shutil.copy2(os.path.join(source, name), os.path.join(snapshot, name))
    print("  data/%s refreshed from data/%s (%d file(s))."
          % (previous, current, len(names(snapshot))))
    return 0


def refresh():
    """Copy what ships now over each snapshot."""
    failed = 0
    for current, previous in PAIRS:
        failed |= refresh_pair(current, previous)
    return failed


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument(
        "--check",
        action="store_true",
        help="verify the snapshots against the last release tag, and change nothing",
    )
    args = ap.parse_args()
    return check() if args.check else refresh()


if __name__ == "__main__":
    sys.exit(main())
