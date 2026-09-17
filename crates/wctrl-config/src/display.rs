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
    /// How many characters this cell holds.
    ///
    /// Nearly always one. The UFC comm windows are two: a units digit and a
    /// partial tens, on one cell, addressed as one glyph. A wide cell fits its
    /// value to this width before the glyph is looked up, because modules do
    /// not agree on the padding: the Hornet sends `" 2"`, the Hind sends `"1"`
    /// from a one-character field and `"1 "` from a two-character one, and all
    /// three mean the same channel.
    #[serde(default = "one")]
    pub width: usize,
    /// Absolute bit index in the device buffer for each of this cell's segment
    /// slots, in the order the glyph tables index them.
    pub segments: Vec<u16>,
}

fn one() -> usize {
    1
}

/// How DCS-BIOS reports which crew station the player is in.
///
/// Spelled the same way in every module that has one, so naming it here is a
/// DCS-BIOS convention rather than knowledge of any particular aircraft. Only 5
/// of the 50 catalogued modules publish it, which is why a seat is optional and
/// rejected where it would never resolve.
pub const SEAT_SIGNAL: &str = "SEAT_POSITION";

/// One named area of a display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub name: String,
    /// The cells it covers, written the way a profile writes them: `"34"` or
    /// `"30-33"`.
    pub cells: String,
    /// What the aircraft this panel was built for puts here. Often the only
    /// thing that makes the name meaningful.
    #[serde(default)]
    pub note: String,
}

/// Fit a value to the number of characters a wide cell holds.
///
/// Trimmed and then right aligned, which is what a number wants: a short value
/// keeps its leading blank and a long one loses its leading digits rather than
/// its trailing ones. Trimming both ends rather than one, because the padding
/// side is the module's choice: the Hornet sends `" 2"`, the Hind sends `"1 "`.
///
/// Trimming is safe here and is not safe on a run of cells. A run gives each
/// cell one character, so its padding is the layout and removing it would shift
/// every character sideways. On one cell the whole value is a single glyph and
/// the padding is only alignment inside it.
fn fit(value: &str, width: usize) -> String {
    let trimmed = value.trim();
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() >= width {
        chars[chars.len() - width..].iter().collect()
    } else {
        let mut out = " ".repeat(width - chars.len());
        out.push_str(trimmed);
        out
    }
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
    /// Named areas of the glass, for choosing where a field goes.
    ///
    /// A cell run says nothing to someone deciding what to put on a panel, and
    /// `30-33` in particular is the kind of thing that turns into a support
    /// question. These name the positions instead. They describe where a region
    /// is rather than what one aircraft uses it for, because the same glass
    /// serves other modules, and the note says what the aircraft it was built
    /// for does with it.
    ///
    /// Advisory, not a constraint: a profile may still name any run of cells.
    #[serde(default)]
    pub regions: Vec<Region>,
    /// Values this hardware spells differently from the way a source reports
    /// them, applied before the glyph is looked up.
    ///
    /// A glyph table is the vendor's, and the vendor did not have to agree with
    /// DCS-BIOS about how to write a number down. The UFC comm preset cells are
    /// the case: two digits on one cell, where the tens is a partial digit that
    /// can only draw 1 or 2, and the vendor spells those `` `X `` and `~X`.
    /// DCS-BIOS sends `"12"`.
    ///
    /// Held on the display rather than on a `Readout`, because it is a fact
    /// about the panel and not a choice the user made. Every profile that ever
    /// drives this cell needs the same rewrite, and none of them should have to
    /// know about it.
    #[serde(default)]
    pub spellings: HashMap<String, String>,
}

impl Display {
    pub fn cell(&self, index: usize) -> Option<&Cell> {
        self.cells.get(index)
    }

    /// The glyph for `value` on `cell`, if that cell's shape can draw it.
    ///
    /// Two passes, uppercase then as sent, and within each pass the form the
    /// cell prefers and then the bare value. Every quirk this hardware has
    /// falls out of those four tries.
    ///
    /// **Uppercase leads** because this panel was built for the Hornet, and
    /// DCS-BIOS reports the Hornet's UFC in capitals throughout. The lowercase
    /// glyphs are real and distinct, `'g'` lights four slots where `'G'` lights
    /// eight, but they were never exercised: every letter in both captured
    /// pages is a capital. The set is also incomplete, with no small r, u, w, y
    /// or z, which is not what a font meant to be used looks like. Another
    /// module naming a guard channel `"g"` should reach the glass looking like
    /// the rest of the panel rather than in a small form nothing else uses.
    ///
    /// Falling back to the value as sent is what keeps that from being a
    /// gamble. `digit7` has a `'p'` and a `'w'` and no capitals at all, so
    /// uppercasing alone would have taken those off the glass.
    ///
    /// The **preferred** form differs by cell. A wide cell wants its full
    /// width, trimmed and right aligned, because the wrong width can still hit
    /// a real entry: a glyph table is shared by every cell of a shape, and the
    /// same slot numbers are different bits on different cells, so `'1'` draws
    /// two strokes of the units digit on a comm window. An ordinary cell wants
    /// the spaced form of a single character, because a digit has two forms
    /// there and the only one ever captured is the spaced one: the COMM page
    /// has cell 0 reading `' 3'`.
    ///
    /// The **bare** value then rescues everything with no spaced form, which is
    /// every letter and every mark. DCS-BIOS pads a string out to its
    /// `max_length` while DCS's own indication does not, so a scratchpad letter
    /// arrives as `" G"` and a guard channel as `" g"`, and both would
    /// otherwise leave the cell dark. A digit never reaches this fallback,
    /// because its preferred form is always in the table.
    pub fn glyph(&self, cell: &Cell, value: &str) -> Option<&Vec<u8>> {
        let table = self.glyphs.get(&cell.shape)?;
        // A spelling only rescues a value the table does not already have, so a
        // display that spells "20" for itself is never overridden by ours.
        let look = |value: &str| -> Option<&Vec<u8>> {
            table
                .get(value)
                .or_else(|| table.get(self.spellings.get(value)?))
        };
        let bare = value.trim();
        let upper = bare.to_uppercase();
        for candidate in [upper.as_str(), bare] {
            let preferred = if cell.width > 1 {
                fit(candidate, cell.width)
            } else if candidate.chars().count() <= 1 {
                format!(" {candidate}")
            } else {
                candidate.to_string()
            };
            if let Some(lit) = look(&preferred).or_else(|| look(candidate)) {
                return Some(lit);
            }
        }
        None
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
    /// Only paint this field from one crew station.
    ///
    /// DCS-BIOS exports the whole cockpit whatever seat you are sitting in, so
    /// a multicrew aircraft publishes both stations at once and a field has no
    /// way to know which one you want. `SEAT_POSITION` is how DCS-BIOS reports
    /// the seat, and it names it the same way in every module that has one, so
    /// this is a convention rather than knowledge of any aircraft.
    ///
    /// Two fields may share cells when their seats differ, which is the point:
    /// the same window shows the pilot one thing and the gunner another. A
    /// field with no seat is always painted, and shares with nothing.
    ///
    /// Only meaningful on a module that reports a seat at all, which is 5 of
    /// the 50 catalogued. Validation rejects it elsewhere rather than silently
    /// never painting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seat: Option<u32>,
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
