#!/usr/bin/env python3
"""Replay captured SET_LCDS frames and read the UFC glass back as text.

  python tools/decode_ufc_lcd.py [log] [--layout group_first] [--every]

The UFC display is a 96-byte segment buffer, not a character buffer. SimAppPro
writes it four bytes at a time and only sends groups whose bytes changed, so a
capture is a stream of partial updates with no text in it anywhere.

This rebuilds the buffer from those updates and, after each one, reverse-looks
up every changed cell against the glyph table in data/displays/ufc1.json. If
the frame layout below is right the output reads as words and numbers. If it is
wrong the output is nonsense, which is the point: the decode is a test of the
layout, not a presentation of it.

`--layout` selects where the group index sits in the payload. The default is
the one we expect; the others exist so a wrong guess can be ruled out in one
run rather than by argument.
"""
import argparse
import json
import os
import re

DEFAULT_LOG = os.path.expandvars(r"%APPDATA%\WWTHID\SimAppPro\WWTHID.log")
DEFAULT_MAP = os.path.join(os.path.dirname(__file__), "..", "data", "displays", "ufc1.json")

LINE = re.compile(
    r"HidData:(?P<device>[^,]+),"
    r"(?P<cmd>[A-Z0-9_]+),"
    r"(?P<dir>send|accept),"
    r"channel:(?P<channel>\d+),"
    r"id:\s*(?P<id>(?:[0-9a-f]{2} ?){4}),"
    r"len:(?P<len>\d+),"
    r"data:\s*(?P<data>(?:[0-9a-f]{2} ?)+)"
)

SET_LCDS = 0x4C

# Candidate payload layouts, as (name, group_offset, data_offset). Offsets are
# into the payload including the command byte at 0.
LAYOUTS = {
    "group_first": (1, 2),   # 4c <group> b0 b1 b2 b3
    "group_last": (5, 1),    # 4c b0 b1 b2 b3 <group>
}


def load_display(path):
    doc = json.load(open(path, encoding="utf-8"))
    d = doc["displays"][0]
    # slot -> lit set, reversed so a rendered cell can be named.
    rev = {}
    for shape, table in d["glyphs"].items():
        rev[shape] = {}
        for ch, lit in table.items():
            rev[shape].setdefault(frozenset(lit), []).append(ch)
    return d, rev


def cell_text(cell, buf, rev):
    """Read one cell out of the buffer and name it, or describe it if unknown."""
    lit = frozenset(
        i for i, bit in enumerate(cell["segments"])
        if buf[bit // 8] >> (bit % 8) & 1
    )
    names = rev[cell["shape"]].get(lit)
    if names:
        # Prefer the plain single-character form when several glyphs share a
        # pattern, so ordinary text does not print as its alternate spelling.
        return min(names, key=lambda c: (len(c), c))
    if not lit:
        return " "
    return "<%s:%s>" % (cell["shape"], ",".join(str(i) for i in sorted(lit)))


def render(display, buf, rev):
    return [cell_text(c, buf, rev) for c in display["cells"]]


def show(cells):
    """Lay the 36 cells out the way they sit on the panel."""
    def run(a, b):
        return "".join(cells[i] for i in range(a, b))
    lines = ["  scratchpad: [%s|%s]" % (run(0, 2), run(2, 9))]
    for n, base in enumerate((9, 14, 19, 24, 29), start=1):
        lines.append("  option %d:   [%s%s]" % (n, cells[base], run(base + 1, base + 5)))
    lines.append("  comm:       [%s] [%s]" % (cells[34], cells[35]))
    return "\n".join(lines)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("log", nargs="?", default=DEFAULT_LOG)
    ap.add_argument("--map", default=DEFAULT_MAP)
    ap.add_argument("--layout", choices=sorted(LAYOUTS), default="group_first")
    ap.add_argument("--every", action="store_true",
                    help="print the whole display after every frame, not just at the end")
    ap.add_argument("--raw", action="store_true",
                    help="print each SET_LCDS payload as hex and decode nothing")
    args = ap.parse_args()

    display, rev = load_display(args.map)
    nbytes = display["buffer_bytes"]
    gsize = display["group_bytes"]
    buf = bytearray(nbytes)

    group_at, data_at = LAYOUTS[args.layout]
    frames = skipped = 0
    lengths = {}
    last = render(display, buf, rev)

    with open(args.log, "r", encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = LINE.search(line)
            if not m or m["dir"] != "send":
                continue
            data = [int(x, 16) for x in m["data"].split()][:int(m["len"])]
            if not data or data[0] != SET_LCDS:
                continue
            lengths[len(data)] = lengths.get(len(data), 0) + 1
            if args.raw:
                print("part %s  len=%d  %s"
                      % (m["id"].strip(), len(data),
                         " ".join("%02x" % b for b in data)))
                continue
            if len(data) < data_at + gsize or len(data) <= group_at:
                skipped += 1
                continue
            group = data[group_at]
            payload = data[data_at:data_at + gsize]
            start = group * gsize
            if start + gsize > nbytes:
                skipped += 1
                continue
            buf[start:start + gsize] = bytes(payload)
            frames += 1

            now = render(display, buf, rev)
            changed = [i for i in range(len(now)) if now[i] != last[i]]
            if changed:
                print("group %-2d -> %s   cells %s"
                      % (group, " ".join("%02x" % b for b in payload),
                         ", ".join("%d=%r" % (i, now[i]) for i in changed)))
            last = now
            if args.every:
                print(show(now))

    if args.raw:
        return
    print("\n%d frames applied, %d skipped. Payload lengths seen: %s"
          % (frames, skipped, lengths or "none"))
    if frames:
        print("\nFinal display, layout %r:\n%s" % (args.layout, show(last)))
        unknown = sum(1 for c in last if c.startswith("<"))
        print("\n%d of %d cells did not match any glyph." % (unknown, len(last)))
        if unknown > len(last) // 3:
            print("That is a lot. The layout is probably wrong; try --layout group_last,\n"
                  "or --raw to look at the payloads directly.")


if __name__ == "__main__":
    main()
