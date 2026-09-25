#!/usr/bin/env python3
"""Regenerate the DED font in data/displays/ded.json.

  python tools/gen_ded.py
  python tools/gen_ded.py --check-font [path/to/ICP_font_0.png]

The font is SimAppPro's own. Where a flight showed a glyph, it is captured from
what SimAppPro sent the ICP. Each flight is two test fixtures: the
ded_simapppro_frames file, every screen write SimAppPro sent, and the
ded_bios_timeline file, what DCS-BIOS said the DED read over the same flight.
The first flight is 2026-09-18; the second, 2026-09-25, went through every DED
page and sub-page.

The glyphs no flight showed are never put on the F-16's DED, so no capture
will ever have them. They are read from SimAppPro's font file,
config/ICP/ICP_font_0.png, and kept below in `FONT_FILE`. Every captured glyph
matches that file pixel for pixel; `--check-font` rereads it from an installed
SimAppPro and checks all 66.

A glyph is captured by pairing the two. Each committed frame is cut into its
five lines of 24 cells, and a line's cells are matched to the one DCS-BIOS
text for that line whose pattern of repeated characters fits. A line that fits
none, or more than one, is skipped. A text is only a candidate if it also
agrees with every glyph an earlier flight captured: the second flight showed
more pages, so more texts, and the pattern alone matched a wrong one. A few
cells the pairing cannot reach are read from a frame named by hand, in `EXTRA`
and `recover_star`.

Only the font and the two notes that list which glyphs are captured and which
are read from the font file are rewritten. Everything else in ded.json is left
as it is on disk, so edit it there.

If a flight ever shows a glyph in `FONT_FILE`: add its fixtures to `FLIGHTS`,
delete the glyph from `FONT_FILE` and rerun. The script fails if a glyph is
both captured and in `FONT_FILE`, or if two captures disagree.
"""
import collections
import json
import os
import re
import struct
import sys
import zlib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "crates", "dsc-config", "tests", "fixtures")
OUT = os.path.join(ROOT, "data", "displays", "ded.json")

# Oldest first: a later flight is paired against what the earlier ones caught.
FLIGHTS = [
    ("ded_simapppro_frames.txt", "ded_bios_timeline.txt"),
    ("ded_simapppro_frames_2.txt", "ded_bios_timeline_2.txt"),
]

GROUP = 25    # bytes per pixel row, 200 pixels
PITCH = 13    # pixel rows per text line
LINES = 5
COLUMNS = 24

ORDER = " ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890a()<>[]+-*/=o|du.,!?:;&_'\"%#@"

# Cells on line 3, column 22, from first-flight frames the pairing could not
# match: the clock at 3:02:06 and 3:02:09, and "CMD STRG" on the TCN page.
EXTRA = {"13:05:56.058": "6", "13:05:59.068": "9", "13:05:45.247": "G"}


def frames(name):
    """Replay SimAppPro's writes; one (time, framebuffer) per commit."""
    fb = bytearray(GROUP * 70)
    out = []
    with open(os.path.join(FIXTURES, name), encoding="latin1") as f:
        for line in f:
            t = re.search(r"\d\d:\d\d:\d\d\.\d+", line).group()
            b = bytes(int(x, 16) for x in re.search(r"\[len=\d+ \[(.*)\]\]", line).group(1).split())
            if b[5] == 2:
                a = int.from_bytes(b[18:22], "little") // 8
                fb[a:a + len(b[22:])] = b[22:]
            elif b[5] == 3:
                out.append((t, bytes(fb)))
    return out


def cell(fb, line, col):
    return tuple(fb[(line * PITCH + r) * GROUP + col] for r in range(PITCH))


def rows_of(cell13):
    return ["".join("#" if v >> x & 1 else "." for x in range(8)) for v in cell13[2:11]]


def bios_texts(name):
    """Every (text, format) DCS-BIOS showed on each line."""
    texts = collections.defaultdict(set)
    cur = {}
    with open(os.path.join(FIXTURES, name), encoding="latin1") as f:
        for l in f:
            m = re.match(r'\s*\d+ ms\s+(DED_L(\d)(_FORMAT)?)\s+= "(.*)"$', l.rstrip("\n"))
            if m:
                cur[m.group(1)] = m.group(4)
                for n in range(1, LINES + 1):
                    if f"DED_L{n}" in cur:
                        texts[n - 1].add((cur[f"DED_L{n}"], cur.get(f"DED_L{n}_FORMAT", " " * COLUMNS)))
    return texts


def pattern(seq):
    ids = {}
    return tuple(ids.setdefault(x, len(ids)) for x in seq)


def paired(all_frames, texts, known):
    """Glyphs from lines whose cells match exactly one DCS-BIOS text."""
    def fits(cells, tx, fm):
        return all(fm[i] == "i" or ch not in known or rows_of(cells[i]) == known[ch]
                   for i, ch in enumerate(tx))

    glyph = {}
    seen = set()
    for _, fb in all_frames:
        for ln in range(LINES):
            cells = tuple(cell(fb, ln, c) for c in range(COLUMNS))
            if cells in seen:
                continue
            seen.add(cells)
            p = pattern(cells)
            cands = [(tx, fm) for tx, fm in texts[ln]
                     if pattern([(ch, fm[i] == "i") for i, ch in enumerate(tx)]) == p
                     and fits(cells, tx, fm)]
            if len(cands) != 1:
                continue
            tx, fm = cands[0]
            for i, ch in enumerate(tx):
                k = (ch, fm[i] == "i")
                assert glyph.setdefault(k, cells[i]) == cells[i], ("two captures disagree", k)
    # Inverse is drawn by the host, so only the plain glyphs belong in the font.
    captured = {}
    for (ch, inv), b in glyph.items():
        if not inv:
            assert not b[0] and not b[1] and not b[11] and not b[12], ch
            captured[ch] = rows_of(b)
    return captured


def recover_star(all_frames):
    """'*' was only ever seen inverse: the TCN page, line 3, column 7."""
    fb = next(fb for t, fb in all_frames if t == "13:05:45.199")
    c = cell(fb, 2, 7)
    assert c[0] == 0 and c[12] == 0 and all(v == 0xff for v in (c[1], c[11])), c
    return rows_of([v ^ 0xff if 1 <= r <= 11 else v for r, v in enumerate(c)])


def g(text):
    rows = text.strip("\n").split("\n")
    assert len(rows) == 9 and all(len(r) == 8 for r in rows), text
    return rows


# Never shown on the F-16's DED, so read from SimAppPro's ICP_font_0.png.
FONT_FILE = {
    '<': g('''
.....##.
....##..
...##...
..##....
.##.....
..##....
...##...
....##..
.....##.'''),
    '[': g('''
..####..
..####..
..##....
..##....
..##....
..##....
..##....
..####..
..####..'''),
    ']': g('''
..####..
..####..
....##..
....##..
....##..
....##..
....##..
..####..
..####..'''),
    '+': g('''
........
........
...##...
...##...
.######.
.######.
...##...
...##...
........'''),
    '=': g('''
........
........
.######.
.######.
........
........
.######.
.######.
........'''),
    '|': g('''
..##....
..##....
..##....
..##....
..##....
..##....
..##....
..##....
..##....'''),
    'd': g('''
...##...
...##...
...##...
...##...
...##...
...##...
.######.
..####..
...##...'''),
    'u': g('''
...##...
..####..
.######.
...##...
...##...
...##...
...##...
...##...
...##...'''),
    ',': g('''
........
........
........
........
........
...##...
...##...
....#...
...#....'''),
    '!': g('''
...##...
...##...
...##...
...##...
...##...
...##...
........
...##...
...##...'''),
    '?': g('''
..####..
.######.
.##..##.
....###.
...###..
...##...
........
...##...
...##...'''),
    ';': g('''
........
...##...
...##...
........
........
...##...
...##...
....#...
...#....'''),
    '&': g('''
...###..
..#####.
..##....
...##...
..###.#.
.##.###.
.##..#..
.######.
..###.#.'''),
    '_': g('''
........
........
........
........
........
........
........
........
.######.'''),
    '"': g('''
.##.##..
.##.##..
..#..#..
.#..#...
........
........
........
........
........'''),
    '%': g('''
.###....
.#.#..#.
.###.##.
....##..
...##...
..##....
.##.###.
.#..#.#.
....###.'''),
    '@': g('''
..####..
.######.
.#...##.
.....##.
.###.##.
.#.#.##.
.#.#.##.
.######.
..####..'''),
}


def captured_glyphs():
    """Every glyph the flights show, checked to agree between flights."""
    known = {}
    for n, (frames_name, bios_name) in enumerate(FLIGHTS):
        all_frames = frames(frames_name)
        got = paired(all_frames, bios_texts(bios_name), known)
        if n == 0:
            got["*"] = recover_star(all_frames)
            for t, ch in EXTRA.items():
                c = cell(next(fb for ft, fb in all_frames if ft == t), 2, 22)
                assert not c[0] and not c[1] and not c[11] and not c[12], (t, c)
                got[ch] = rows_of(c)
        for ch, rows in got.items():
            assert known.setdefault(ch, rows) == rows, ("flights disagree", ch)
    return known


def splice(text, font, captured):
    """Put the font and its two notes into ded.json's text, leaving the rest."""
    glyphs = ",\n".join(
        "          " + json.dumps(c, ensure_ascii=False) + ": [" + ", ".join(json.dumps(r) for r in rows) + "]"
        for c, rows in font.items())
    block = '"fonts": {\n        "ded": {\n' + glyphs + "\n        }\n      }"
    text, n = re.subn(r'"fonts": \{\n.*?\n        \}\n      \}', lambda _: block, text, flags=re.S)
    assert n == 1, "fonts block not found"

    have = "".join(c for c in ORDER if c in captured).strip()
    read = "".join(c for c in ORDER if c not in captured)
    notes = {
        "Captured, exactly": f"Captured, exactly as SimAppPro sent them: {have} and space. '*' was only ever seen inverse and is that cell flipped back.",
        "Read from SimAppPro": f"Read from SimAppPro's font file, config/ICP/ICP_font_0.png, because the F-16's DED never shows them: {read}. Every captured glyph matches that file pixel for pixel.",
    }
    for start, note in notes.items():
        text, n = re.subn(r'^(\s*)"' + start + r'.*?(,?)$',
                          lambda m: m.group(1) + json.dumps(note, ensure_ascii=False) + m.group(2),
                          text, flags=re.M)
        assert n == 1, f"note {start!r} not found"
    return text


def font():
    captured = captured_glyphs()
    both = set(captured) & set(FONT_FILE)
    assert not both, f"captured now, delete from FONT_FILE: {''.join(sorted(both))}"
    missing = [c for c in ORDER if c not in captured and c not in FONT_FILE]
    assert not missing, missing
    return {c: captured.get(c) or FONT_FILE[c] for c in ORDER}, captured


def read_png(path):
    """(width, lit(x, y)) for an 8-bit, non-interlaced PNG."""
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", path
    pos, idat = 8, b""
    while pos < len(data):
        n, = struct.unpack(">I", data[pos:pos + 4])
        kind, body = data[pos + 4:pos + 8], data[pos + 8:pos + 8 + n]
        pos += 12 + n
        if kind == b"IHDR":
            w, h, depth, ctype, _, _, interlace = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
    assert depth == 8 and interlace == 0, (depth, interlace)
    ch = {0: 1, 2: 3, 4: 2, 6: 4}[ctype]
    raw, stride = zlib.decompress(idat), w * ch
    rows, prev, p = [], bytearray(stride), 0
    for _ in range(h):
        kind, line = raw[p], bytearray(raw[p + 1:p + 1 + stride])
        p += 1 + stride
        for i in range(stride):
            a = line[i - ch] if i >= ch else 0
            b = prev[i]
            c = prev[i - ch] if i >= ch else 0
            if kind == 1:
                line[i] = (line[i] + a) & 255
            elif kind == 2:
                line[i] = (line[i] + b) & 255
            elif kind == 3:
                line[i] = (line[i] + (a + b) // 2) & 255
            elif kind == 4:
                pa, pb, pc = abs(b - c), abs(a - c), abs(a + b - 2 * c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        rows.append(bytes(line))
        prev = line

    def lit(x, y):
        px = rows[y][x * ch:(x + 1) * ch]
        return px[0] > 127 and (ctype not in (4, 6) or px[-1] > 127)
    return lit


def check_font(path):
    """Compare every glyph with SimAppPro's font file.

    The file is 8 glyphs to a row in 64x52 cells, looked up by
    textfont_config_new.json beside it. A glyph is its 6x9 pixels drawn 4x,
    each pixel a small plus centred on (21 + 4x, 9 + 4y) in its cell, and
    lands in columns 1 to 6 of our 8.
    """
    lit = read_png(path)
    with open(os.path.join(os.path.dirname(path), "textfont_config_new.json"), encoding="utf-8") as f:
        index = {e["key"]: e["value"] for e in json.load(f)["DCS"]}
    ours, _ = font()
    wrong = []
    for c in ORDER:
        r, k = divmod(index[c], 8)
        theirs = ["." + "".join("#" if lit(64 * k + 21 + 4 * x, 52 * r + 9 + 4 * y) else "." for x in range(6)) + "."
                  for y in range(9)]
        if theirs != ours[c]:
            wrong.append(c)
    assert not wrong, f"differ from {path}: {''.join(wrong)}"
    print("all", len(ORDER), "glyphs match", path)


def main():
    if sys.argv[1:2] == ["--check-font"]:
        default = os.path.expandvars(
            r"%LOCALAPPDATA%\Programs\SimAppPro\resources\app.asar.unpacked\config\ICP\ICP_font_0.png")
        check_font(sys.argv[2] if len(sys.argv) > 2 else default)
        return

    full, captured = font()
    with open(OUT, encoding="utf-8") as f:
        text = f.read()
    text = splice(text, full, captured)
    json.loads(text)
    with open(OUT, "w", encoding="utf-8", newline="\r\n") as f:
        f.write(text)
    print("captured", len(captured), "from the font file", len(full) - len(captured), "total", len(full))


if __name__ == "__main__":
    main()
