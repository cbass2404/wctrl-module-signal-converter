//! The MCDU's font upload.
//!
//! The MCDU holds no font of its own. Glyphs live in RAM and are gone after a
//! power cycle, so a host has to upload them before the grid draws anything.
//! Ported from WwDevicesDotnet (BSD-3-Clause, Andrew Whewell and Laurent
//! André; `McduFontPacketMap.cs`, `McduFontGlyph.cs`, `Winctrl/FontWriter.cs`),
//! whose approach is to replay SimAppPro's own upload with the glyph bytes
//! swapped for the font's. Its packet map is that upload as hex, with
//! placeholders where the bytes vary. See `THIRD_PARTY_NOTICES.md`.

use std::path::Path;

use serde::Deserialize;

use crate::{Error, Result};

/// A font: a bitmap per character, in a large and a small size.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct McduFont {
    pub name: String,
    pub glyph_width: usize,
    pub glyph_height: usize,
    /// The cell width the glyphs sit in. Each glyph is drawn at the left of a
    /// cell this wide, which is the spacing between characters.
    pub glyph_full_width: usize,
    pub large_glyphs: Vec<Glyph>,
    pub small_glyphs: Vec<Glyph>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Glyph {
    pub character: char,
    /// One string per row: `X` or `1` is a lit pixel, anything else is dark.
    pub bit_array: Vec<String>,
}

impl Glyph {
    /// Rows of bytes, most significant bit leftmost, each row padded out to a
    /// whole byte.
    fn bytes(&self) -> std::result::Result<Vec<u8>, String> {
        let width = self.bit_array.first().map_or(0, |r| r.chars().count());
        let per_row = width.div_ceil(8);
        let mut out = Vec::with_capacity(per_row * self.bit_array.len());
        for (n, row) in self.bit_array.iter().enumerate() {
            if row.chars().count() != width {
                return Err(format!(
                    "{:?} row {} is not {width} pixels wide",
                    self.character,
                    n + 1
                ));
            }
            let mut bytes = vec![0u8; per_row];
            for (i, c) in row.chars().enumerate() {
                if c == 'X' || c == '1' {
                    bytes[i / 8] |= 0x80 >> (i % 8);
                }
            }
            out.extend(bytes);
        }
        Ok(out)
    }
}

impl McduFont {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string()))
    }
}

/// SimAppPro's upload, as WwDevicesDotnet recorded it.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct PacketMap {
    pub glyph_width: usize,
    pub glyph_height: usize,
    /// Every report as hex. `{CP}` is the part id's low two bytes; `_` is a
    /// glyph byte, `LL` the screen brightness, `XX`/`YY` the grid origin, and
    /// `WW`/`HH` the glyph size.
    packets: Vec<String>,
    x_offset_offset: i64,
    y_offset_offset: i64,
    glyph_width_offsets: Vec<usize>,
    glyph_height_offsets: Vec<usize>,
    display_brightness_offset: i64,
    large_glyph_offsets: Vec<GlyphOffsets>,
    small_glyph_offsets: Vec<GlyphOffsets>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct GlyphOffsets {
    character: char,
    /// Where each of the glyph's bytes goes in the upload, counted across
    /// every report. A `-1` starts a run: the two numbers after it are its
    /// first and last offset.
    glyph_map: Vec<i64>,
}

impl GlyphOffsets {
    fn offsets(&self) -> Vec<usize> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < self.glyph_map.len() {
            match self.glyph_map[i] {
                -1 if i + 2 < self.glyph_map.len() => {
                    out.extend(self.glyph_map[i + 1] as usize..=self.glyph_map[i + 2] as usize);
                    i += 3;
                }
                n => {
                    if n >= 0 {
                        out.push(n as usize);
                    }
                    i += 1;
                }
            }
        }
        out
    }
}

impl PacketMap {
    pub fn load(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)?;
        serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string()))
    }
}

/// One step of an upload, in the order it has to be sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadStep {
    /// A 64-byte report on the pixel channel, `0xf0`.
    Report(Vec<u8>),
    /// The upload sets the screen's brightness partway through. It is an
    /// ordinary `SET_LEDX`, so it is handed back as one rather than as bytes,
    /// and goes out through the command channel's checks.
    SetLed { index: u8, value: u8 },
}

/// The upload that puts `font` on the panel at `part_id`.
///
/// `origin` is where the grid's top-left sits on the 640x480 surface, and
/// `brightness` is the screen brightness the upload leaves set.
pub fn font_upload(
    map: &PacketMap,
    font: &McduFont,
    part_id: u32,
    origin: (u16, u16),
    brightness: u8,
) -> Result<Vec<UploadStep>> {
    let bad = |why: String| Error::BadFont(font.name.clone(), why);
    let width = font.glyph_full_width.max(font.glyph_width);
    if font.glyph_height != map.glyph_height {
        return Err(bad(format!(
            "glyphs are {} pixels high and the packet map is for {}",
            font.glyph_height, map.glyph_height
        )));
    }
    if width / 8 != map.glyph_width / 8 {
        return Err(bad(format!(
            "glyphs are {width} pixels wide and the packet map is for {}",
            map.glyph_width
        )));
    }

    let prefix = format!("{:02x}{:02x}", part_id & 0xff, (part_id >> 8) & 0xff);
    let mut blob = Vec::new();
    let mut lens = Vec::with_capacity(map.packets.len());
    for packet in &map.packets {
        let hex: String = packet
            .replace("{CP}", &prefix)
            .chars()
            .map(|c| if "_HLWXY".contains(c) { '0' } else { c })
            .collect();
        let start = blob.len();
        for i in (0..hex.len()).step_by(2) {
            let byte = hex
                .get(i..i + 2)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
                .ok_or_else(|| bad(format!("packet {} is not hex", lens.len())))?;
            blob.push(byte);
        }
        lens.push(blob.len() - start);
    }

    for (glyphs, offsets, size) in [
        (&font.large_glyphs, &map.large_glyph_offsets, "large"),
        (&font.small_glyphs, &map.small_glyph_offsets, "small"),
    ] {
        for glyph in glyphs {
            // A character the upload has no slot for cannot reach the panel.
            let Some(slot) = offsets.iter().find(|o| o.character == glyph.character) else {
                continue;
            };
            let bytes = glyph.bytes().map_err(bad)?;
            let at = slot.offsets();
            if at.len() != bytes.len() {
                return Err(bad(format!(
                    "{size} {:?} is {} bytes and the map has room for {}",
                    glyph.character,
                    bytes.len(),
                    at.len()
                )));
            }
            for (offset, byte) in at.into_iter().zip(bytes) {
                *blob.get_mut(offset).ok_or_else(|| {
                    bad(format!("{size} {:?} maps past the end", glyph.character))
                })? = byte;
            }
        }
    }

    let mut set = |offset: i64, value: u8| {
        if let Some(b) = usize::try_from(offset).ok().and_then(|o| blob.get_mut(o)) {
            *b = value;
        }
    };
    set(map.x_offset_offset, origin.0 as u8);
    set(map.y_offset_offset, origin.1 as u8);
    for &o in &map.glyph_width_offsets {
        set(o as i64, width as u8);
    }
    for &o in &map.glyph_height_offsets {
        set(o as i64, font.glyph_height as u8);
    }
    set(map.display_brightness_offset, brightness);

    let mut steps = Vec::with_capacity(lens.len());
    let mut at = 0;
    for len in lens {
        let report = blob[at..at + len].to_vec();
        at += len;
        steps.push(match report.first() {
            Some(0xf0) => UploadStep::Report(report),
            // 02 | part | 03 | 49 index value
            Some(0x02) if report.len() >= 9 && report[5] == 3 && report[6] == 0x49 => {
                UploadStep::SetLed {
                    index: report[7],
                    value: report[8],
                }
            }
            _ => {
                return Err(bad(format!(
                    "unexpected report {:02x?}",
                    &report[..report.len().min(8)]
                )))
            }
        });
    }
    Ok(steps)
}
