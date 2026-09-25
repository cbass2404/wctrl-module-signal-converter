//! The ICP's DED against SimAppPro's own frames.
//!
//! `fixtures/ded_simapppro_frames.txt` is every screen write SimAppPro sent a
//! ViperAce ICP over 26 seconds of a live F-16 mission, as WWTHID.log printed
//! them. `fixtures/ded_bios_timeline.txt` is what DCS-BIOS said the DED read
//! during the same flight. Replaying the first gives the pixels SimAppPro put
//! on the glass; drawing the second through our map has to give the same
//! pixels, byte for byte, or we disagree with the hardware.
//!
//! The `_2` pair is a second flight, 2026-09-25, through every DED page and
//! sub-page. Its frames are trimmed to the commits that drew a cell not seen
//! earlier in the flight, plus the writes those commits build on.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;

use dsc_config::{Display, DisplayCatalogue, Screen, Transport};

const LINES: usize = 5;
const COLUMNS: usize = 24;
const LINE_BYTES: usize = 13 * 25;
const SCREEN_BYTES: usize = 1600;

/// One line's rows of the framebuffer. Line 5 is a row short.
fn line_of(fb: &[u8], line: usize) -> &[u8] {
    &fb[line * LINE_BYTES..((line + 1) * LINE_BYTES).min(SCREEN_BYTES)]
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name))
        .expect("fixture is readable")
}

fn ded() -> Display {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cat = DisplayCatalogue::load_dir(&root.join("data/displays")).expect("displays load");
    cat.get("DED").expect("DED is in data/displays").clone()
}

/// The framebuffer after each commit, with the time it was committed.
fn committed_frames(name: &str) -> Vec<(String, Vec<u8>)> {
    // SimAppPro sometimes writes all 13 rows of line 5, one more than the
    // screen has, so the buffer runs past it and `line_of` stops at the edge.
    let mut fb = vec![0u8; LINES * LINE_BYTES];
    let mut out = Vec::new();
    for line in fixture(name).lines() {
        let Some(open) = line.find("[f0 ") else { continue };
        let time = line[12..24].to_string();
        let close = open + line[open..].find(']').unwrap();
        let raw: Vec<u8> = line[open + 1..close]
            .split_whitespace()
            .map(|h| u8::from_str_radix(h, 16).unwrap())
            .collect();
        let frame = &raw[1..];
        match frame[4] {
            0x02 => {
                let pixel = u32::from_le_bytes(frame[17..21].try_into().unwrap()) as usize;
                assert_eq!(pixel % 8, 0, "a write starts on a byte");
                let data = &frame[21..];
                fb[pixel / 8..pixel / 8 + data.len()].copy_from_slice(data);
            }
            0x03 => out.push((time, fb.clone())),
            other => panic!("unexpected command 0x{other:02x}"),
        }
    }
    out
}

/// Every text and every format DCS-BIOS sent for each line.
fn bios_lines(name: &str) -> Vec<(BTreeSet<String>, BTreeSet<String>)> {
    let mut out = vec![(BTreeSet::new(), BTreeSet::new()); LINES];
    for line in fixture(name).lines() {
        let Some(at) = line.find("DED_L") else { continue };
        let name = line[at..].split_whitespace().next().unwrap();
        let (Some(a), Some(b)) = (line.find('"'), line.rfind('"')) else { continue };
        let value = line[a + 1..b].to_string();
        assert_eq!(value.chars().count(), COLUMNS, "{line}");
        let n: usize = name[5..6].parse().unwrap();
        if name.ends_with("_FORMAT") {
            out[n - 1].1.insert(value);
        } else {
            out[n - 1].0.insert(value);
        }
    }
    out
}

fn render_line(ded: &Display, line: usize, text: &str, format: &str) -> Vec<u8> {
    let mut screen = Screen::new(ded);
    for (col, (c, f)) in text.chars().zip(format.chars()).enumerate() {
        screen
            .draw_styled(ded, line * COLUMNS + col, &c.to_string(), f == 'i')
            .unwrap_or_else(|e| panic!("{text:?}: {e}"));
    }
    line_of(screen.bytes(), line).to_vec()
}

#[test]
fn the_map_describes_the_ded() {
    let ded = ded();
    assert_eq!(ded.transport, Transport::Pixel);
    assert_eq!(ded.cells.len(), LINES * COLUMNS);
    // Every cell is its whole 8x13 box, so drawing one clears what was there.
    // Except that the screen is 64 rows, not 65, so line 5 has no bottom row.
    for c in &ded.cells {
        let rows = if c.index < 4 * COLUMNS { 13 } else { 12 };
        assert_eq!(c.segments.len(), 8 * rows, "cell {}", c.index);
    }
    // Cell 25 is line 2, column 1: 13 rows down and 8 pixels in.
    assert_eq!(ded.cells[25].segments[0], 13 * 200 + 8);
    // The whole character set SimAppPro draws, and space.
    let table = &ded.glyphs["ded"];
    for c in " ABCDEFGHIJKLMNOPQRSTUVWXYZ1234567890a()<>[]+-*/=o|du.,!?:;&_'\"%#@".chars() {
        assert!(table.contains_key(&c.to_string()), "no glyph for {c:?}");
    }
}

#[test]
fn every_line_simapppro_drew_is_reproduced_from_the_bios_text() {
    let ded = ded();
    let bios = bios_lines("ded_bios_timeline.txt");
    let frames = committed_frames("ded_simapppro_frames.txt");
    assert!(frames.len() > 20, "only {} frames", frames.len());

    let mut reproduced = BTreeSet::new();
    let mut unexplained = Vec::new();
    for (time, fb) in &frames {
        for (line, (texts, formats)) in bios.iter().enumerate() {
            let want = line_of(fb, line);
            let hit = texts.iter().find(|text| {
                formats.iter().any(|format| render_line(&ded, line, text, format) == want)
            });
            match hit {
                Some(text) => {
                    reproduced.insert(text.clone());
                }
                None => unexplained.push(format!("{time} line {}", line + 1)),
            }
        }
    }

    // The one frame that is not explained is the TCN page as it first came up,
    // showing other values than the listener caught. Its inverse cells are
    // checked on their own below.
    assert_eq!(unexplained, [
        "13:05:45.199 line 1",
        "13:05:45.199 line 3",
        "13:05:45.199 line 4",
        "13:05:45.199 line 5",
    ]);
    // The CNI page, the clock ticking over, the blank lines between, and the
    // TCN page's selected fields, which are only right if the format line was
    // drawn inverse.
    for text in [
        " UHF  305.00  STPT a  1 ",
        " M   4   1337      T  1X",
        "                        ",
        "       *      *CMD STRG ",
    ] {
        assert!(reproduced.contains(text), "{text:?} was never matched");
    }
    assert!(
        reproduced.iter().filter(|t| t.starts_with(" VHF  123.35")).count() > 10,
        "the CNI clock should match on every tick: {reproduced:?}"
    );
}

#[test]
fn the_second_flight_reproduces_a_line_with_each_glyph_it_captured() {
    // Not every line: past CNI and TCN, SimAppPro lays pages out from its own
    // spreadsheet rather than from what DCS shows. It draws "LIST     a  1"
    // where DCS-BIOS sends "LIST        1a", and placeholder X's in fields
    // DCS-BIOS fills in. Those lines are SimAppPro's, not a fault in our map.
    let ded = ded();
    let bios = bios_lines("ded_bios_timeline_2.txt");
    let frames = committed_frames("ded_simapppro_frames_2.txt");
    assert!(frames.len() > 20, "only {} frames", frames.len());

    // Each text drawn once; there are far more frames than texts.
    let drawn: Vec<HashMap<Vec<u8>, &String>> = bios
        .iter()
        .enumerate()
        .map(|(line, (texts, formats))| {
            texts
                .iter()
                .flat_map(|text| formats.iter().map(move |format| (text, format)))
                .map(|(text, format)| (render_line(&ded, line, text, format), text))
                .collect()
        })
        .collect();
    let mut reproduced = BTreeSet::new();
    for (_, fb) in &frames {
        for (line, texts) in drawn.iter().enumerate() {
            if let Some(text) = texts.get(line_of(fb, line)) {
                reproduced.insert((*text).clone());
            }
        }
    }
    for c in "JKWYZ>-/#'".chars() {
        assert!(reproduced.iter().any(|t| t.contains(c)), "no line with {c:?} was matched");
    }
}

#[test]
fn an_inverse_star_is_the_box_simapppro_draws() {
    // On the TCN page SimAppPro marks the selected fields with inverse stars,
    // on three different lines. Each one has to be our inverse star exactly,
    // in its own cell, which checks the inverse rows sit right on every line.
    let ded = ded();
    let (_, fb) = committed_frames("ded_simapppro_frames.txt")
        .into_iter()
        .find(|(t, _)| t == "13:05:45.199")
        .expect("the TCN frame");
    let mut screen = Screen::new(&ded);
    let cells = [(2, 7), (2, 14), (3, 4), (3, 8), (3, 16), (3, 23), (4, 16), (4, 21)];
    for (line, col) in cells {
        screen.draw_styled(&ded, line * COLUMNS + col, "*", true).unwrap();
    }
    for (line, col) in cells {
        for row in 0..13 {
            let at = (line * 13 + row) * 25 + col;
            if at >= SCREEN_BYTES {
                continue;
            }
            assert_eq!(
                screen.bytes()[at], fb[at],
                "line {} column {col} row {row}",
                line + 1
            );
        }
    }
}

#[test]
fn inverse_fills_the_box_and_knocks_the_glyph_out() {
    let ded = ded();
    let mut plain = Screen::new(&ded);
    let mut inverse = Screen::new(&ded);
    plain.draw(&ded, 0, "A").unwrap();
    inverse.draw_styled(&ded, 0, "A", true).unwrap();
    for row in 0..13 {
        let (p, i) = (plain.bytes()[row * 25], inverse.bytes()[row * 25]);
        if (1..=11).contains(&row) {
            assert_eq!(i, !p, "row {row} is flipped");
        } else {
            assert_eq!((p, i), (0, 0), "row {row} is outside the box");
        }
    }
    // A space inverse is a solid box, which is how a selected blank reads.
    let mut blank = Screen::new(&ded);
    blank.draw_styled(&ded, 0, " ", true).unwrap();
    assert_eq!(blank.bytes()[25], 0xff);
}

#[test]
fn the_arrow_is_not_uppercased_into_a_letter() {
    // DCS-BIOS spells the DED's arrow 'a'. Uppercasing first, as the UFC does,
    // would draw 'A' there with no error anywhere.
    let ded = ded();
    let mut arrow = Screen::new(&ded);
    let mut letter = Screen::new(&ded);
    arrow.draw(&ded, 0, "a").unwrap();
    letter.draw(&ded, 0, "A").unwrap();
    assert_ne!(arrow, letter);
}
