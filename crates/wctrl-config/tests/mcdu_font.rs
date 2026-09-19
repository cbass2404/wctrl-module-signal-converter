//! The MCDU font upload, built from the shipped files in `data/mcdu`.
//!
//! Nothing here reaches hardware. What it pins down is that the upload is only
//! ever what WwDevicesDotnet sends: font heads and data, error queries, the
//! format table, and one brightness write. A report outside that would be
//! replaying bytes we do not understand at a panel.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use wctrl_config::mcdu_font::{font_upload, McduFont, PacketMap, UploadStep};

const MCDU: u32 = 0xbb32;

fn data(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../data/mcdu")
        .join(p)
}

fn upload() -> Vec<UploadStep> {
    let map = PacketMap::load(&data("font-packet-map-3x31.json")).expect("packet map loads");
    let font = McduFont::load(&data("a10c-font-21x31.json")).expect("font loads");
    font_upload(&map, &font, MCDU, (0x34, 0x14), 0xc8).expect("the A-10C font fits the map")
}

/// Reassemble the 0xf0 stream and split it into (function, payload).
fn commands(steps: &[UploadStep]) -> Vec<(u32, u32, Vec<u8>)> {
    let mut stream = Vec::new();
    for step in steps {
        if let UploadStep::Report(r) = step {
            assert_eq!(r.len(), 64);
            let n = r[3] as usize;
            assert!(n <= 60, "report carries {n} bytes");
            stream.extend_from_slice(&r[4..4 + n]);
        }
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < stream.len() {
        let part = u32::from_le_bytes(stream[i..i + 4].try_into().unwrap());
        let function = u32::from_le_bytes(stream[i + 4..i + 8].try_into().unwrap());
        let len = u32::from_le_bytes(stream[i + 13..i + 17].try_into().unwrap()) as usize;
        out.push((part, function, stream[i + 17..i + 17 + len].to_vec()));
        i += 17 + len;
    }
    assert_eq!(i, stream.len(), "the stream ends on a command boundary");
    out
}

#[test]
fn the_upload_is_only_font_error_queries_and_the_format_table() {
    let steps = upload();
    let mut seen: BTreeMap<u32, usize> = BTreeMap::new();
    for (part, function, _) in commands(&steps) {
        assert_eq!(part, MCDU, "function 0x{function:03x} addressed elsewhere");
        *seen.entry(function).or_default() += 1;
    }
    let expected: BTreeMap<u32, usize> = [
        (0x105, 48), // getLastErrorString
        (0x106, 2),  // downLoadFontHead, slots 5 and 6
        (0x107, 44), // downLoadFontData
        (0x118, 1),  // setScreenInfo
        (0x119, 27), // setFeatureInfo
        (0x11a, 1),  // setCompositeIndexBytes
        (0x11c, 1),  // buildFormatTable
        (0x11e, 1),  // clearFeatureInfo
    ]
    .into();
    assert_eq!(seen, expected);
}

#[test]
fn the_one_lamp_write_is_the_screen_brightness() {
    let leds: Vec<_> = upload()
        .into_iter()
        .filter_map(|s| match s {
            UploadStep::SetLed { index, value } => Some((index, value)),
            UploadStep::Report(_) => None,
        })
        .collect();
    assert_eq!(
        leds,
        [(1, 0xc8)],
        "Screen_Backlight, at the brightness asked for"
    );
}

#[test]
fn the_font_heads_carry_the_glyph_size() {
    let heads: Vec<Vec<u8>> = commands(&upload())
        .into_iter()
        .filter(|(_, f, _)| *f == 0x106)
        .map(|(_, _, p)| p)
        .collect();
    assert_eq!(heads.len(), 2);
    for head in heads {
        // Cell width 23 (the font's full width, not its 21-pixel glyphs), height 31.
        assert!(head.windows(3).any(|w| w == [23, 0, 31]), "{head:02x?}");
    }
}

#[test]
fn every_glyph_lands_in_the_upload() {
    // A glyph whose bytes are all dark would be indistinguishable from one
    // that was never written, so count lit bytes in the data chunks instead:
    // the font has 130 glyphs, nearly all of which light something.
    let lit: usize = commands(&upload())
        .into_iter()
        .filter(|(_, f, _)| *f == 0x107)
        .map(|(_, _, p)| p.iter().filter(|b| **b != 0).count())
        .sum();
    assert!(lit > 2000, "only {lit} lit bytes reached the upload");
}

#[test]
fn a_font_of_the_wrong_height_is_refused() {
    let map = PacketMap::load(&data("font-packet-map-3x31.json")).unwrap();
    let mut font = McduFont::load(&data("a10c-font-21x31.json")).unwrap();
    font.glyph_height = 32;
    let err = font_upload(&map, &font, MCDU, (0x34, 0x14), 255).unwrap_err();
    assert!(err.to_string().contains("31"), "{err}");
}
