#!/usr/bin/env python3
"""Regenerate the DED font in data/displays/ded.json.

  python tools/gen_ded.py

The font is SimAppPro's own glyphs where a capture has them, and glyphs drawn
by hand in the same style where none has turned up yet. The captures are the
two test fixtures from a live F-16 flight on 2026-09-18:
ded_simapppro_frames.txt, every screen write SimAppPro sent the ICP, and
ded_bios_timeline.txt, what DCS-BIOS said the DED read over the same flight.

A glyph is captured by pairing the two. Each committed frame is cut into its
five lines of 24 cells, and a line's cells are matched to the one DCS-BIOS
text for that line whose pattern of repeated characters fits. A line that fits
none, or more than one, is skipped. A few cells the pairing cannot reach are
read from a frame named by hand, in `EXTRA` and `recover_star`.

Only the font and the two notes that list which glyphs are captured and which
are drawn are rewritten. Everything else in ded.json is left as it is on disk,
so edit it there.

To replace a drawn glyph: capture a flight that shows it, add the frames and
the DCS-BIOS text to the fixtures, delete the glyph from `DRAWN` and rerun. If
the pairing cannot reach it, name its frame in `EXTRA` instead. The script
fails if a glyph is both captured and drawn, so a stale drawing cannot
shadow a capture.
"""
import collections
import json
import os
import re

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FIXTURES = os.path.join(ROOT, "crates", "dsc-config", "tests", "fixtures")
OUT = os.path.join(ROOT, "data", "displays", "ded.json")

GROUP = 25    # bytes per pixel row, 200 pixels
PITCH = 13    # pixel rows per text line
LINES = 5
COLUMNS = 24

ORDER = " ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890a()<>[]+-*/=o|du.,!?:;&_'\"%#@"

# Cells on line 3, column 22, from frames the pairing could not match: the
# clock at 3:02:06 and 3:02:09, and "CMD STRG" on the TCN page.
EXTRA = {"13:05:56.058": "6", "13:05:59.068": "9", "13:05:45.247": "G"}


def frames():
    """Replay SimAppPro's writes; one (time, framebuffer) per commit."""
    fb = bytearray(GROUP * 70)
    out = []
    with open(os.path.join(FIXTURES, "ded_simapppro_frames.txt"), encoding="latin1") as f:
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


def bios_texts():
    """Every (text, format) DCS-BIOS showed on each line."""
    texts = collections.defaultdict(set)
    cur = {}
    with open(os.path.join(FIXTURES, "ded_bios_timeline.txt"), encoding="latin1") as f:
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


def paired(all_frames):
    """Glyphs from lines whose cells match exactly one DCS-BIOS text."""
    texts = bios_texts()
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
                     if pattern([(ch, fm[i] == "i") for i, ch in enumerate(tx)]) == p]
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


# Not yet seen from SimAppPro; each goes when a capture of it does.
DRAWN = {
    'J': g('''
....###.
....###.
.....##.
.....##.
.....##.
.....##.
.##..##.
.######.
..####..'''),
    'K': g('''
.##..##.
.##.##..
.####...
.###....
.###....
.####...
.##.##..
.##..##.
.##..##.'''),
    'W': g('''
.##..##.
.##..##.
.##..##.
.##..##.
.##..##.
.######.
.######.
.##..##.
.#....#.'''),
    'Y': g('''
.##..##.
.##..##.
.##..##.
..####..
...##...
...##...
...##...
...##...
...##...'''),
    'Z': g('''
.######.
.######.
.....##.
....##..
...##...
..##....
.##.....
.######.
.######.'''),
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
    '>': g('''
.##.....
..##....
...##...
....##..
.....##.
....##..
...##...
..##....
.##.....'''),
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
    '-': g('''
........
........
........
........
.######.
.######.
........
........
........'''),
    '/': g('''
.....##.
.....##.
....##..
....##..
...##...
..##....
..##....
.##.....
.##.....'''),
    '=': g('''
........
........
.######.
.######.
........
.######.
.######.
........
........'''),
    '|': g('''
...##...
...##...
...##...
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
........
...##...
...##...
..##....'''),
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
....##..
...##...
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
..##....
........'''),
    '&': g('''
..###...
.##.##..
.##.##..
..###...
.####.#.
.##.##..
.##.##..
.######.
..##.##.'''),
    '_': g('''
........
........
........
........
........
........
........
.######.
.######.'''),
    "'": g('''
...##...
...##...
..##....
........
........
........
........
........
........'''),
    '"': g('''
.##..##.
.##..##.
.##..##.
........
........
........
........
........
........'''),
    '%': g('''
.##...#.
.##..##.
....##..
....##..
...##...
..##....
..##....
.##..##.
.#...##.'''),
    '#': g('''
..#..#..
..#..#..
.######.
..#..#..
..#..#..
..#..#..
.######.
..#..#..
..#..#..'''),
    '@': g('''
..####..
.##..##.
.##.###.
.##.#.#.
.##.#.#.
.##.###.
.##.....
.##..##.
..####..'''),
    'u': g('''
........
........
........
...##...
..####..
.######.
........
........
........'''),
    'd': g('''
........
........
........
.######.
..####..
...##...
........
........
........'''),
}

def splice(text, font, captured):
    """Put the font and its two notes into ded.json's text, leaving the rest."""
    glyphs = ",\n".join(
        "          " + json.dumps(c, ensure_ascii=False) + ": [" + ", ".join(json.dumps(r) for r in rows) + "]"
        for c, rows in font.items())
    block = '"fonts": {\n        "ded": {\n' + glyphs + "\n        }\n      }"
    text, n = re.subn(r'"fonts": \{\n.*?\n        \}\n      \}', lambda _: block, text, flags=re.S)
    assert n == 1, "fonts block not found"

    have = "".join(c for c in ORDER if c in captured).strip()
    drawn = "".join(c for c in ORDER if c not in captured)
    notes = {
        "Captured, exactly": f"Captured, exactly as SimAppPro sent them: {have} and space. '*' was only ever seen inverse and is that cell flipped back.",
        "Drawn here": f"Drawn here in the same style, not yet seen from SimAppPro: {drawn}. Replace any of them with a capture when one turns up.",
    }
    for start, note in notes.items():
        text, n = re.subn(r'^(\s*)"' + start + r'.*?(,?)$',
                          lambda m: m.group(1) + json.dumps(note, ensure_ascii=False) + m.group(2),
                          text, flags=re.M)
        assert n == 1, f"note {start!r} not found"
    return text


def main():
    all_frames = frames()
    captured = paired(all_frames)
    captured["*"] = recover_star(all_frames)
    for t, ch in EXTRA.items():
        c = next(fb for ft, fb in all_frames if ft == t)
        c = cell(c, 2, 22)
        assert not c[0] and not c[1] and not c[11] and not c[12], (t, c)
        captured[ch] = rows_of(c)

    both = set(captured) & set(DRAWN)
    assert not both, f"captured now, delete from DRAWN: {''.join(sorted(both))}"
    missing = [c for c in ORDER if c not in captured and c not in DRAWN]
    assert not missing, missing
    font = {c: captured.get(c) or DRAWN[c] for c in ORDER}

    with open(OUT, encoding="utf-8") as f:
        text = f.read()
    text = splice(text, font, captured)
    json.loads(text)
    with open(OUT, "w", encoding="utf-8", newline="\r\n") as f:
        f.write(text)
    print("captured", len(captured), "drawn", len(font) - len(captured), "total", len(font))


if __name__ == "__main__":
    main()
