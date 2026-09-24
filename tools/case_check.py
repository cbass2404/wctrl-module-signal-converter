#!/usr/bin/env python3
"""Check that every file in HEAD is named on disk exactly as git names it.

  python tools/case_check.py

A release step. Windows ignores case, and git here does too
(`core.ignorecase`), so renaming `F-14.json` to `f-14.json` changes the disk
and not git. Everything on this machine then reads the new name, and a fresh
checkout, which is what the pipeline builds from, writes the old one. Code
that compares names as strings, such as the page loader matching a file to its
module, passes here and fails there.

Exits 1 and lists each file whose case differs, with the commands that record
the rename in git.
"""
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def main():
    out = subprocess.run(["git", "ls-tree", "-r", "-z", "--name-only", "HEAD"],
                         cwd=ROOT, capture_output=True, check=True).stdout
    listed = {}
    wrong = []
    for name in filter(None, out.decode("utf-8").split("\0")):
        folder, base = os.path.split(name)
        if folder not in listed:
            try:
                listed[folder] = os.listdir(os.path.join(ROOT, folder))
            except OSError:
                listed[folder] = []
        # A folder renamed by case shows up as each of its files, since the
        # file names are what git holds.
        for actual in listed[folder]:
            if actual != base and actual.lower() == base.lower():
                wrong.append((name, folder + "/" + actual if folder else actual))

    if not wrong:
        print("ok, every file in HEAD has the same name on disk.")
        return
    print("named differently in git and on disk:")
    for git_name, disk_name in wrong:
        print("  git: %s   disk: %s" % (git_name, disk_name))
    print()
    print("A fresh checkout gets git's names. To record the rename:")
    for git_name, disk_name in wrong:
        print("  git rm --cached -q \"%s\"" % git_name)
        print("  git add \"%s\"" % disk_name)
    sys.exit(1)


if __name__ == "__main__":
    main()
