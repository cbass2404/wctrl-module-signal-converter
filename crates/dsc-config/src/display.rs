//! Displays: the cell and glyph map, and a host-side screen buffer.
//!
//! A WinCtrl panel with glass does not take text. It takes a bitmap of
//! segments, written a few bytes at a time, and a character position is a set
//! of bit indices scattered through that bitmap. `data/displays/*.json` holds
//! the map, transcribed from SimAppPro's tables and confirmed against captured
//! hardware traffic. See `docs/PROTOCOL.md`.
//!
//! A pixel screen is the same model with a regular layout. Its bit index is a
//! pixel, `y * width + x`, so a character cell is the pixels of its box and a
//! glyph is the pixels it lights. The ICP's DED is 120 such cells, generated
//! from a [`Grid`] rather than listed, with the font drawn as rows of `#`.
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

fn is_false(b: &bool) -> bool {
    !*b
}

/// How a display's buffer reaches the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Transport {
    /// `SET_LCDS` on the command channel, one group of `group_bytes` at a time.
    /// Never acknowledged.
    #[default]
    Segment,
    /// Report `0xf0`: writes of any length into a framebuffer, then a commit
    /// to show them. A bit index is a pixel.
    Pixel,
    /// Report `0xf2`: a grid of characters the panel draws from a font it was
    /// sent. The MCDU. Only ever written whole, so the buffer is one group.
    Text,
}

/// A character grid the panel renders itself, from a font uploaded to it.
///
/// Unlike a segment or pixel display, a cell holds a character and a colour,
/// not a bitmap. The panel keeps no font across a power cycle, so which font
/// to upload is part of the display's description. It is not the user's
/// choice: an aircraft with a CDU of its own has glyphs drawn to match what
/// DCS-BIOS sends for it, and that font is the aircraft's.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextGrid {
    pub columns: usize,
    pub rows: usize,
    /// Top-left of the grid on the panel's 640x480 surface, in pixels.
    pub origin: [u16; 2],
    /// Where the font upload puts the text, which is not quite `origin`.
    pub font_origin: [u16; 2],
    /// The font upload to fill with glyphs, relative to this file.
    pub upload: String,
    /// Runtime aircraft name to its font, relative to this file. Keyed by
    /// aircraft rather than module, because one module can carry two
    /// cockpits: the F-14B and F-14B(U) share `F-14` and only one has a CDNU.
    #[serde(default)]
    pub native_fonts: HashMap<String, String>,
    /// The directory this display was loaded from, which the paths above are
    /// relative to.
    #[serde(skip)]
    pub dir: std::path::PathBuf,
    /// Per font file, the characters it can draw in each size.
    #[serde(skip)]
    pub charsets: HashMap<String, FontChars>,
}

/// The characters one font can draw.
#[derive(Debug, Clone, Default)]
pub struct FontChars {
    pub large: std::collections::HashSet<char>,
    pub small: std::collections::HashSet<char>,
}

impl TextGrid {
    /// The font file for an aircraft, if it has a native one.
    pub fn font_for(&self, aircraft: &str) -> Option<&str> {
        self.native_fonts.get(aircraft).map(String::as_str)
    }

    pub fn path(&self, relative: &str) -> std::path::PathBuf {
        self.dir.join(relative)
    }
}

/// The colours a text grid can draw, in the order the panel indexes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Colour {
    Black,
    Amber,
    #[default]
    White,
    Cyan,
    Green,
    Magenta,
    Red,
    Yellow,
    Brown,
    Grey,
    Khaki,
}

impl Colour {
    pub fn ordinal(self) -> u8 {
        self as u8
    }

    /// Every colour, in the order the panel indexes them.
    ///
    /// Here rather than in the editor so a colour cannot be offered that the
    /// hardware has no index for, and so the two lists cannot drift apart.
    pub const ALL: [Colour; 11] = [
        Colour::Black,
        Colour::Amber,
        Colour::White,
        Colour::Cyan,
        Colour::Green,
        Colour::Magenta,
        Colour::Red,
        Colour::Yellow,
        Colour::Brown,
        Colour::Grey,
        Colour::Khaki,
    ];

    /// The name this colour is written under in a profile.
    pub fn name(self) -> &'static str {
        match self {
            Colour::Black => "black",
            Colour::Amber => "amber",
            Colour::White => "white",
            Colour::Cyan => "cyan",
            Colour::Green => "green",
            Colour::Magenta => "magenta",
            Colour::Red => "red",
            Colour::Yellow => "yellow",
            Colour::Brown => "brown",
            Colour::Grey => "grey",
            Colour::Khaki => "khaki",
        }
    }
}

/// One cell of a text grid as the buffer holds it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextCell {
    pub ch: char,
    pub fg: u8,
    pub bg: u8,
    pub small: bool,
}

/// Bytes per cell in a text grid's buffer: the character in the low 21 bits,
/// then foreground, background and size. All zero is a blank cell.
pub const TEXT_CELL_BYTES: usize = 4;

impl TextCell {
    fn encode(self) -> [u8; TEXT_CELL_BYTES] {
        let code = u32::from(self.ch)
            | u32::from(self.fg & 0x0f) << 21
            | u32::from(self.bg & 0x0f) << 25
            | u32::from(self.small) << 29;
        code.to_le_bytes()
    }

    fn decode(bytes: &[u8]) -> Self {
        let code = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if code == 0 {
            return TextCell {
                ch: ' ',
                fg: Colour::White.ordinal(),
                bg: 0,
                small: false,
            };
        }
        TextCell {
            ch: char::from_u32(code & 0x1f_ffff).unwrap_or(' '),
            fg: ((code >> 21) & 0x0f) as u8,
            bg: ((code >> 25) & 0x0f) as u8,
            small: code >> 29 & 1 == 1,
        }
    }
}

/// A text grid's buffer as cells, row by row, for the transport to send.
pub fn text_cells(bytes: &[u8]) -> Vec<TextCell> {
    bytes
        .chunks(TEXT_CELL_BYTES)
        .map(TextCell::decode)
        .collect()
}

/// A regular grid of character cells over a pixel framebuffer.
///
/// Spares a pixel display from listing a bit index for every pixel of every
/// cell, which for the DED would be 12,480 numbers. The cells are generated
/// from this at load, row by row, left to right.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    /// Pixels in one framebuffer row.
    pub width: usize,
    /// The glyph table every generated cell draws from.
    pub shape: String,
    /// How far below the top of its cell a font bitmap starts.
    #[serde(default)]
    pub ink_top: usize,
    /// First and last row of a cell that an inverse character fills, counted
    /// from the top of the cell.
    pub inverse_rows: [usize; 2],
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
    #[serde(default)]
    pub transport: Transport,
    /// Worked out from `text` on a text grid, so it can be left out there.
    #[serde(default)]
    pub buffer_bytes: usize,
    /// The unit a change is found and written in. For a segment display that
    /// is the device's write group; for a pixel display it is one row; for a
    /// text grid it is the whole screen.
    #[serde(default)]
    pub group_bytes: usize,
    /// Generates `cells` when they are not listed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<Grid>,
    /// The character grid, on a `text` display.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextGrid>,
    #[serde(default)]
    pub cells: Vec<Cell>,
    /// Shape name, then glyph, then which of a cell's slots that glyph lights.
    ///
    /// Keyed by the whole field value, not by character. A two-character field
    /// can occupy one cell, and those glyphs are not the union of their parts:
    /// `'0'` and `' 0'` share almost no segments.
    #[serde(default)]
    pub glyphs: HashMap<String, HashMap<String, Vec<u8>>>,
    /// Glyphs drawn as rows of `#` and `.`, per shape, added to `glyphs` at
    /// load. Only on a display with a `grid`, which says how wide a row is and
    /// where in the cell the first one goes.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub fonts: HashMap<String, HashMap<String, Vec<String>>>,
    /// Look glyphs up only as sent, never uppercased first.
    ///
    /// The DED needs it: DCS-BIOS spells its arrow `a` and its degree sign
    /// `o`, so trying `A` and `O` first would draw the wrong character with
    /// no error anywhere.
    #[serde(default, skip_serializing_if = "is_false")]
    pub exact_case: bool,
    /// Per shape, the slots an inverse character flips. Generated from the
    /// grid; a shape with none cannot be drawn inverse.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub inverse: HashMap<String, Vec<u8>>,
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

    /// Whether any cell here can be drawn inverse. A text grid always can: it
    /// swaps a cell's colours.
    /// Whether this glass is a text grid.
    ///
    /// A grid draws characters from a font it was given, so it can draw
    /// anything in that font; the others draw from a fixed glyph table. That is
    /// what decides colour, the small font and a divider.
    pub fn is_text_grid(&self) -> bool {
        self.transport == Transport::Text
    }

    pub fn draws_inverse(&self) -> bool {
        self.transport == Transport::Text || self.inverse.values().any(|slots| !slots.is_empty())
    }

    /// Build the cells, glyphs and inverse slots a `grid` implies. Called at
    /// load; a display without a grid is left as it is.
    pub fn expand(&mut self) -> Result<()> {
        let key = self.key.clone();
        let bad = |why: String| Error::BadDisplay(key.clone(), why);
        if self.transport == Transport::Text {
            let Some(t) = &self.text else {
                return Err(bad(
                    "a text display needs a `text` block saying its size".into()
                ));
            };
            if self.grid.is_some() || !self.fonts.is_empty() || !self.glyphs.is_empty() {
                return Err(bad(
                    "a text display draws from the panel's font, not from glyphs or a pixel grid"
                        .into(),
                ));
            }
            let n = t.columns * t.rows;
            self.cells = (0..n)
                .map(|index| Cell {
                    index,
                    shape: "text".into(),
                    width: 1,
                    segments: Vec::new(),
                })
                .collect();
            self.buffer_bytes = n * TEXT_CELL_BYTES;
            self.group_bytes = self.buffer_bytes;
            return Ok(());
        }
        if self.text.is_some() {
            return Err(bad("only a text display takes a `text` block".into()));
        }
        if self.buffer_bytes == 0 || self.group_bytes == 0 {
            return Err(bad("buffer_bytes and group_bytes are required".into()));
        }
        let Some(g) = self.grid.clone() else {
            if !self.fonts.is_empty() {
                return Err(bad("a font needs a grid to say where its rows go".into()));
            }
            return Ok(());
        };
        let slots = g.cell_width * g.cell_height;
        if slots > 256 {
            return Err(bad(format!("a {}x{} cell has more pixels than a glyph can name", g.cell_width, g.cell_height)));
        }
        if g.columns * g.cell_width > g.width {
            return Err(bad(format!("{} columns of {} pixels do not fit in {}", g.columns, g.cell_width, g.width)));
        }
        // The last line may hang off the bottom of the buffer. The DED does:
        // five 13 row lines on a 64 row screen, so line 5 has no bottom row.
        // That row is margin, so it is dropped rather than refused, as long as
        // everything a character can light is still on the screen.
        let bits = (self.buffer_bytes * 8).min(usize::from(u16::MAX) + 1);
        let lowest = (g.rows - 1) * g.cell_height + g.inverse_rows[1].max(g.ink_top);
        if (lowest + 1) * g.width > bits {
            return Err(bad(format!("row {lowest} of the grid is past the end of a {bits} bit buffer")));
        }
        if self.cells.is_empty() {
            for row in 0..g.rows {
                for col in 0..g.columns {
                    // Row by row, so a clipped cell loses only its last slots
                    // and every slot a glyph names still means the same pixel.
                    let mut segments = Vec::with_capacity(slots);
                    'rows: for y in 0..g.cell_height {
                        for x in 0..g.cell_width {
                            let pixel = (row * g.cell_height + y) * g.width + col * g.cell_width + x;
                            if pixel >= bits {
                                break 'rows;
                            }
                            segments.push(pixel as u16);
                        }
                    }
                    self.cells.push(Cell {
                        index: self.cells.len(),
                        shape: g.shape.clone(),
                        width: 1,
                        segments,
                    });
                }
            }
        }
        for (shape, font) in &self.fonts {
            let table = self.glyphs.entry(shape.clone()).or_default();
            for (value, rows) in font {
                if g.ink_top + rows.len() > g.cell_height {
                    return Err(bad(format!("glyph {value:?} is {} rows and the cell has room for {}", rows.len(), g.cell_height - g.ink_top)));
                }
                let mut lit = Vec::new();
                for (y, row) in rows.iter().enumerate() {
                    if row.chars().count() > g.cell_width {
                        return Err(bad(format!("glyph {value:?} has a row wider than {} pixels", g.cell_width)));
                    }
                    for (x, c) in row.chars().enumerate() {
                        match c {
                            '#' => lit.push(((g.ink_top + y) * g.cell_width + x) as u8),
                            '.' => {}
                            other => {
                                return Err(bad(format!("glyph {value:?} has {other:?} in it; a row is # and . only")))
                            }
                        }
                    }
                }
                table.insert(value.clone(), lit);
            }
        }
        let [top, bottom] = g.inverse_rows;
        if top > bottom || bottom >= g.cell_height {
            return Err(bad(format!("inverse rows {top} to {bottom} are not inside a {} row cell", g.cell_height)));
        }
        self.inverse
            .entry(g.shape.clone())
            .or_insert_with(|| (top * g.cell_width..(bottom + 1) * g.cell_width).map(|s| s as u8).collect());
        Ok(())
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
        let passes: &[&str] = if self.exact_case {
            &[bare]
        } else {
            &[upper.as_str(), bare]
        };
        for &candidate in passes {
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
            let mut one: DisplayCatalogue = read_json(&path)?;
            for display in &mut one.displays {
                display.expand()?;
                if let Some(text) = &mut display.text {
                    text.dir = dir.to_path_buf();
                    load_charsets(&display.key, text)?;
                }
            }
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

/// Read what each native font can draw, so a profile can be checked against
/// it. A font that will not load is an error now rather than a blank screen
/// in the middle of a flight.
fn load_charsets(key: &str, text: &mut TextGrid) -> Result<()> {
    let files: std::collections::BTreeSet<String> = text.native_fonts.values().cloned().collect();
    for file in files {
        let font = crate::mcdu_font::McduFont::load(&text.path(&file))
            .map_err(|e| Error::BadDisplay(key.to_string(), format!("font {file}: {e}")))?;
        text.charsets.insert(
            file,
            FontChars {
                large: font.large_glyphs.iter().map(|g| g.character).collect(),
                small: font.small_glyphs.iter().map(|g| g.character).collect(),
            },
        );
    }
    Ok(())
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
        self.draw_styled(display, index, value, false)
    }

    /// Draw `value`, optionally inverse: the glyph knocked out of a filled box.
    ///
    /// Inverse is drawn here, not by the device. SimAppPro does the same; the
    /// screen has no such mode. On a shape with no inverse slots the flag does
    /// nothing, which validation reports before it gets this far.
    pub fn draw_styled(
        &mut self,
        display: &Display,
        index: usize,
        value: &str,
        inverse: bool,
    ) -> Result<()> {
        let cell = display
            .cell(index)
            .ok_or_else(|| Error::NoSuchCell(display.key.clone(), index))?;
        let lit = display.glyph(cell, value).ok_or_else(|| {
            Error::NoSuchGlyph(value.to_string(), cell.shape.clone(), display.key.clone())
        })?;
        // A slot is a u8, so 256 covers every slot a glyph can name. A lookup
        // table rather than `contains`, because a DED cell is 104 slots and
        // the whole screen is repainted on every batch.
        let mut on = [false; 256];
        for &slot in lit {
            on[slot as usize] = true;
        }
        if inverse {
            for &slot in display.inverse.get(&cell.shape).into_iter().flatten() {
                on[slot as usize] ^= true;
            }
        }
        for (slot, &bit) in cell.segments.iter().enumerate() {
            let (byte, mask) = (bit as usize / 8, 1u8 << (bit % 8));
            if on.get(slot).copied().unwrap_or(false) {
                self.bytes[byte] |= mask;
            } else {
                self.bytes[byte] &= !mask;
            }
        }
        Ok(())
    }

    /// Put one character in a text grid's cell, replacing what was there.
    ///
    /// Inverse swaps the colours, which is how a text grid highlights: the
    /// cell fills with the field's colour and the character is cut out of it
    /// in black. The character is not checked against a font here; which
    /// characters a font draws is a profile check, and a stray one on the live
    /// stream draws as a gap rather than stopping the rest of the screen.
    pub fn draw_text(
        &mut self,
        display: &Display,
        index: usize,
        ch: char,
        colour: Colour,
        small: bool,
        inverse: bool,
    ) -> Result<()> {
        if display.transport != Transport::Text || index >= display.cells.len() {
            return Err(Error::NoSuchCell(display.key.clone(), index));
        }
        let (fg, bg) = if inverse {
            (Colour::Black, colour)
        } else {
            (colour, Colour::Black)
        };
        let cell = TextCell {
            ch,
            fg: fg.ordinal(),
            bg: bg.ordinal(),
            small,
        };
        let at = index * TEXT_CELL_BYTES;
        self.bytes[at..at + TEXT_CELL_BYTES].copy_from_slice(&cell.encode());
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

/// The narrowest run a divider can rule: a dash with a blank each side.
pub const MIN_DIVIDER_CELLS: usize = 3;

/// The rule a divider draws across `width` cells, one glyph per cell.
///
/// A blank cell at each end, so the line never runs into the frame or into
/// whatever sits beside it, and an unbroken run of dashes between them:
/// ` ------- `. Spaced dashes were tried first, on the glass, and read as a
/// dotted line rather than a rule.
///
/// A run too narrow to hold a dash between two margins draws blank rather than
/// crowding the ends, and `problems` refuses one before it gets here. This is a
/// function of the width alone so the editor can show the same rule it will
/// draw, by asking rather than working it out again.
pub fn divider_rule(width: usize) -> Vec<String> {
    let mut out = vec![" ".to_string(); width];
    if width < MIN_DIVIDER_CELLS {
        return out;
    }
    for cell in out.iter_mut().take(width - 1).skip(1) {
        *cell = "-".to_string();
    }
    out
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
    /// Catalogue signal id. Empty on a divider, which reads nothing, and on a
    /// field nobody has finished yet, which `problems` says so about.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    /// Draw a fixed rule across these cells instead of reading a signal.
    ///
    /// A screen only half used needs somewhere for the eye to stop. The Apache
    /// puts its keyboard unit on the bottom line and the A-10C starts ten lines
    /// down, and with the rest of the glass dark the page has no edge. A rule
    /// gives it one. It names no signal, so it is drawn from the moment the
    /// aircraft loads and never changes afterwards.
    ///
    /// Text grids only, like `colour` and `small`: a segment display draws from
    /// a glyph table, and none of them has a rule in it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub divider: bool,
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
    /// A second string signal, laid out like `source`, whose `i` marks the
    /// characters to draw inverse.
    ///
    /// The F-16 DED is the case: DCS-BIOS sends each line as `DED_Ln` and its
    /// highlighting as `DED_Ln_FORMAT`, one character for one. Any other mark
    /// draws normally; `b`, for big, is sent too and this screen has no large
    /// font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// The colour a text grid draws this field in. Text grids only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour: Option<Colour>,
    /// Draw in the text grid's small font. Text grids only.
    #[serde(default, skip_serializing_if = "is_false")]
    pub small: bool,
    /// A second string signal, laid out like `source`, whose characters pick
    /// each cell's colour through `codes`. Text grids only.
    ///
    /// The CH-47F is the case: DCS-BIOS sends each CDU line with a
    /// `_COLOR` twin, one letter per character. The letters are the module's
    /// own, so the profile says what they mean. A letter with no entry, and
    /// every cell until the signal arrives, draws in `colour`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colours: Option<ColourSource>,
    /// Characters this module sends in place of the ones it means, rewritten
    /// one for one before the field is laid out.
    ///
    /// DCS-BIOS cannot export every symbol a CDU draws, so it sends a
    /// stand-in: the A-10C's arrows arrive as `»` and `«`. Unlike `aliases`,
    /// which swap a whole value, this works inside a line of text. Each key
    /// and value is one character.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub replace: HashMap<String, String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// Where a readout's per-character colours come from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColourSource {
    /// Catalogue signal id, a string one character per cell.
    pub source: String,
    /// Each letter the module sends, and the colour it means.
    pub codes: HashMap<String, Colour>,
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

    /// The rule this divider draws, one glyph per cell.
    pub fn divider_cells(&self) -> Vec<String> {
        divider_rule(self.cells.len())
    }

    /// Which cells of the run draw inverse, given the format signal's text.
    pub fn inverse_cells(&self, format: &str) -> Vec<bool> {
        self.lay_out(format).iter().map(|m| m == "i").collect()
    }

    /// The colour of each cell of the run, given the colour signal's text.
    ///
    /// None where the text names no colour, so the field's own applies.
    pub fn colour_cells(&self, codes: &str) -> Vec<Option<Colour>> {
        let Some(source) = &self.colours else {
            return Vec::new();
        };
        self.lay_out(codes)
            .iter()
            .map(|c| source.codes.get(c).copied())
            .collect()
    }

    /// Apply this module's wording fixes.
    pub fn alias<'a>(&'a self, value: &'a str) -> &'a str {
        self.aliases.get(value).map(String::as_str).unwrap_or(value)
    }

    /// Swap each stand-in character for the one it stands for.
    pub fn replace_chars(&self, text: &str) -> String {
        if self.replace.is_empty() {
            return text.to_string();
        }
        text.chars()
            .map(|c| {
                let mut buf = [0u8; 4];
                self.replace
                    .get(c.encode_utf8(&mut buf) as &str)
                    .and_then(|r| r.chars().next())
                    .unwrap_or(c)
            })
            .collect()
    }
}
