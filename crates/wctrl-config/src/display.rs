//! Segment displays: the cell and glyph map, and a host-side screen buffer.
//!
//! A WinCtrl panel with glass does not take text. It takes a bitmap of
//! segments, written a few bytes at a time, and a character position is a set
//! of bit indices scattered through that bitmap. `data/displays/*.json` holds
//! the map, transcribed from SimAppPro's tables and confirmed against captured
//! hardware traffic. See `docs/PROTOCOL.md`.
//!
//! Two things here are correctness requirements rather than optimisations:
//!
//! * A write group holds bits belonging to more than one cell, so changing one
//!   character means read-modify-write of its group. That is what [`Screen`]
//!   exists for; it is not a cache.
//! * A cell can straddle two groups, so a character change can take two frames
//!   and the cell reads as a different, wrong letter in between. Diffing whole
//!   groups from a settled buffer is what keeps that off the glass.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{read_json, Error, Result};

/// One character position on a display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cell {
    pub index: usize,
    /// Which glyph table this cell draws from, such as `alnum16` or `digit7`.
    /// A 7-segment cell cannot show a letter, and asking it to is an error
    /// rather than a blank, because silently blanking a cell looks like a
    /// wiring fault and sends you hunting in the wrong place.
    pub shape: String,
    /// Absolute bit index in the device buffer for each of this cell's segment
    /// slots, in the order the glyph tables index them.
    pub segments: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Display {
    pub key: String,
    pub part_id: u32,
    pub buffer_bytes: usize,
    pub group_bytes: usize,
    pub cells: Vec<Cell>,
    /// Shape name, then glyph, then which of a cell's slots that glyph lights.
    ///
    /// Keyed by the whole field value, not by character. A two-character field
    /// can occupy one cell, and those glyphs are not the union of their parts:
    /// `'0'` and `' 0'` share almost no segments.
    pub glyphs: HashMap<String, HashMap<String, Vec<u8>>>,
}

impl Display {
    pub fn cell(&self, index: usize) -> Option<&Cell> {
        self.cells.get(index)
    }

    /// The glyph for `value` on `cell`, if that cell's shape can draw it.
    pub fn glyph(&self, cell: &Cell, value: &str) -> Option<&Vec<u8>> {
        self.glyphs.get(&cell.shape)?.get(value)
    }

    /// How many write groups the buffer is divided into.
    pub fn groups(&self) -> usize {
        self.buffer_bytes.div_ceil(self.group_bytes)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DisplayCatalogue {
    #[serde(default)]
    pub displays: Vec<Display>,
}

impl DisplayCatalogue {
    /// Load every `*.json` in a directory. A missing directory is not an error:
    /// a user with no glass panels has no `data/displays`.
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut out = Self::default();
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        let mut paths: Vec<_> = entries
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "json"))
            .collect();
        paths.sort();
        for path in paths {
            let one: DisplayCatalogue = read_json(&path)?;
            out.displays.extend(one.displays);
        }
        Ok(out)
    }

    pub fn get(&self, key: &str) -> Option<&Display> {
        self.displays.iter().find(|d| d.key == key)
    }

    /// The display carried by a given part, if any.
    pub fn for_part(&self, part_id: u32) -> Option<&Display> {
        self.displays.iter().find(|d| d.part_id == part_id)
    }
}

/// A host-side copy of a display's segment buffer.
///
/// Starts blank, which matches a display that has just been cleared. It does
/// not match one whose state is unknown, so a first paint should clear the
/// device rather than assume it agrees.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Screen {
    bytes: Vec<u8>,
    group_bytes: usize,
}

impl Screen {
    pub fn new(display: &Display) -> Self {
        Screen {
            bytes: vec![0; display.buffer_bytes],
            group_bytes: display.group_bytes,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn clear(&mut self) {
        self.bytes.iter_mut().for_each(|b| *b = 0);
    }

    /// Draw `value` into one cell, replacing whatever was there.
    ///
    /// Every one of the cell's segments is written, lit or not, so a cell never
    /// keeps a stroke from the character before it.
    pub fn draw(&mut self, display: &Display, index: usize, value: &str) -> Result<()> {
        let cell = display
            .cell(index)
            .ok_or_else(|| Error::NoSuchCell(display.key.clone(), index))?;
        let lit = display.glyph(cell, value).ok_or_else(|| {
            Error::NoSuchGlyph(value.to_string(), cell.shape.clone(), display.key.clone())
        })?;
        for (slot, &bit) in cell.segments.iter().enumerate() {
            let (byte, mask) = (bit as usize / 8, 1u8 << (bit % 8));
            if lit.contains(&(slot as u8)) {
                self.bytes[byte] |= mask;
            } else {
                self.bytes[byte] &= !mask;
            }
        }
        Ok(())
    }

    /// Write groups that differ from `previous`, as `(group index, bytes)`.
    ///
    /// Whole groups, because that is the write granularity: a group cannot be
    /// partially written, and the bytes it holds may belong to cells that did
    /// not change.
    pub fn changes_from(&self, previous: &Screen) -> Vec<(u8, Vec<u8>)> {
        let n = self.group_bytes;
        self.bytes
            .chunks(n)
            .zip(previous.bytes.chunks(n))
            .enumerate()
            .filter(|(_, (now, was))| now != was)
            .map(|(g, (now, _))| (g as u8, now.to_vec()))
            .collect()
    }

    /// Every group, for a first paint or a resync where the device's state is
    /// not known to match ours.
    pub fn all_groups(&self) -> Vec<(u8, Vec<u8>)> {
        self.bytes
            .chunks(self.group_bytes)
            .enumerate()
            .map(|(g, bytes)| (g as u8, bytes.to_vec()))
            .collect()
    }
}

// ------------------------------------------------------------- profile side

/// A run of cells, written `"2-8"` or `"34"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub first: usize,
    pub last: usize,
}

impl CellRange {
    pub fn len(&self) -> usize {
        self.last + 1 - self.first
    }

    pub fn is_empty(&self) -> bool {
        false // `last` is inclusive and never below `first`, so a range is >= 1
    }

    pub fn contains(&self, cell: usize) -> bool {
        cell >= self.first && cell <= self.last
    }

    pub fn overlaps(&self, other: &CellRange) -> bool {
        self.first <= other.last && other.first <= self.last
    }

    pub fn cells(&self) -> impl Iterator<Item = usize> {
        self.first..=self.last
    }
}

impl std::fmt::Display for CellRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.first == self.last {
            write!(f, "{}", self.first)
        } else {
            write!(f, "{}-{}", self.first, self.last)
        }
    }
}

impl std::str::FromStr for CellRange {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, String> {
        let bad = || format!("expected a cell like \"34\" or a run like \"2-8\", got {s:?}");
        let (a, b) = match s.split_once('-') {
            Some((a, b)) => (a.trim(), b.trim()),
            None => (s.trim(), s.trim()),
        };
        let first: usize = a.parse().map_err(|_| bad())?;
        let last: usize = b.parse().map_err(|_| bad())?;
        if last < first {
            return Err(format!("cell run {s:?} ends before it starts"));
        }
        Ok(CellRange { first, last })
    }
}

impl Serialize for CellRange {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for CellRange {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Which end of a cell run the text is anchored to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    #[default]
    Left,
    /// What a scratchpad wants: digits enter at the rightmost cell and shift
    /// left, and DCS-BIOS can hand over more characters than there are cells.
    Right,
}

/// One field of a display, and the signal that feeds it.
///
/// A field has exactly one owner. Nothing chooses between two sources for the
/// same cells at runtime: on an aircraft that drives its own display, the
/// cockpit has already decided what belongs there, and on any other the user
/// has.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Readout {
    pub device: String,
    /// Which display on that device, matching a key in `data/displays`.
    pub display: String,
    pub cells: CellRange,
    /// Catalogue signal id.
    pub source: String,
    /// What the gauge reads in the cockpit, for a numeric source: the real
    /// values at the bottom and top of its travel.
    ///
    /// DCS-BIOS reports a needle as a position, not a quantity, and nothing in
    /// the catalogue says what the face is marked with. So the user reads the
    /// dial and says "this one goes 0 to 300". The conversion is linear in
    /// needle travel, which is exact for an evenly marked dial and approximate
    /// for one that is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reads: Option<[f64; 2]>,
    /// Decimal places for a numeric source.
    #[serde(default, skip_serializing_if = "is_zero_u8")]
    pub decimals: u8,
    #[serde(default, skip_serializing_if = "is_left")]
    pub align: Align,
    /// Values this module words differently from the glyph table.
    ///
    /// DCS-BIOS does not always report what DCS's own indication does: the
    /// Hornet scratchpad's second string arrives as `"--"` where the cockpit
    /// says `"_"`, and `"--"` is not a glyph. That is a property of the module,
    /// so it is recorded with the module's mapping rather than in the engine.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub aliases: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

fn is_left(a: &Align) -> bool {
    *a == Align::Left
}

impl Readout {
    /// Turn a raw signal value into the characters this field should show.
    ///
    /// `max` is the source's own declared maximum, so a needle at 0..65535 and
    /// a selector at 0..10 go through the same arithmetic. A selector whose
    /// value already is the number needs `reads` set to its own range, which
    /// makes the conversion an identity rather than a special case.
    pub fn format_number(&self, value: u16, max: u16) -> String {
        let [low, high] = self.reads.unwrap_or([0.0, max as f64]);
        let travel = if max == 0 { 0.0 } else { value as f64 / max as f64 };
        format!("{:.*}", self.decimals as usize, low + travel * (high - low))
    }

    /// Lay `text` out across the run, one glyph per cell.
    ///
    /// Longer text is cropped from the end the alignment anchors away from,
    /// which is what makes a right aligned scratchpad drop its leading pad
    /// rather than its last digit. Shorter text is padded with blanks, so a
    /// field that shrinks never leaves the old character behind.
    ///
    /// A run of exactly one cell takes the whole value as a single glyph. That
    /// is not a convenience: a two-character field really does occupy one cell
    /// on this hardware, and its glyph is not the union of the two characters.
    pub fn lay_out(&self, text: &str) -> Vec<String> {
        let width = self.cells.len();
        if width == 1 {
            return vec![text.to_string()];
        }
        let chars: Vec<char> = text.chars().collect();
        let mut out = Vec::with_capacity(width);
        if self.align == Align::Right {
            let start = chars.len().saturating_sub(width);
            let pad = width.saturating_sub(chars.len());
            out.extend(std::iter::repeat_n(" ".to_string(), pad));
            out.extend(chars[start..].iter().map(|c| c.to_string()));
        } else {
            out.extend(chars.iter().take(width).map(|c| c.to_string()));
            while out.len() < width {
                out.push(" ".to_string());
            }
        }
        out
    }

    /// Apply this module's wording fixes.
    pub fn alias<'a>(&'a self, value: &'a str) -> &'a str {
        self.aliases.get(value).map(String::as_str).unwrap_or(value)
    }
}
