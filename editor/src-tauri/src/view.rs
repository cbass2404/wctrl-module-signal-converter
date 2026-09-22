//! Shapes the window receives.
//!
//! These exist so the frontend is not handed the on-disk structures directly.
//! `data/devices.json` carries measurement notes and verification flags that
//! are meaningful to whoever is mapping hardware and noise to everyone else,
//! and a profile summary is cheaper than the profile it summarises.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use dsc_config::{
    Colour, DeviceSpec, Display, DisplayCatalogue, Families, Led, Module, Profile, ShapeArt,
    ValueLabel,
};

#[derive(Serialize)]
pub struct LedView {
    pub name: String,
    pub label: String,
    /// `dimmer` or `indicator`. Decides whether the editor offers a brightness
    /// at all: an indicator acks 255 and lights nothing, which is not "off" and
    /// is not a difference the user should have to discover.
    pub kind: String,
    pub max: u8,
    pub on_value: u8,
    pub dimmable: bool,
    /// False where the lamp has not been confirmed on hardware. Shown, because
    /// a user chasing a lamp that will not light deserves to know we are not
    /// certain of it either.
    pub verified: bool,
    /// Hardware notes, where any were recorded.
    pub note: String,
    /// Lamps this dimmer hides at 0. Non-empty marks a gate, whose value at
    /// zero is its daylight floor.
    pub governs: Vec<String>,
    pub part_id: u32,
    pub index: u8,
}

impl LedView {
    fn of(part_id: u32, led: &Led) -> Self {
        LedView {
            name: led.name.clone(),
            label: led.label.clone(),
            kind: if led.is_dimmable() { "dimmer" } else { "indicator" }.to_string(),
            max: led.max_value(),
            on_value: led.on_value(),
            dimmable: led.is_dimmable(),
            verified: led.verified,
            note: led.note.clone(),
            governs: led.governs.clone(),
            part_id,
            index: led.index,
        }
    }
}

#[derive(Serialize)]
pub struct DeviceView {
    pub key: String,
    pub display_name: String,
    pub product_name: String,
    pub leds: Vec<LedView>,
    /// Segment displays this device carries, if any. Almost every panel has
    /// none, so the window only grows a display section where there is glass.
    pub displays: Vec<DisplayView>,
    /// Other devices that are this one under another name, which a profile
    /// can point it at. Worked out here by `same_hardware` so the window does
    /// not keep a second idea of what counts.
    pub variants: Vec<String>,
}

/// A segment display, described only as far as the window needs it.
///
/// The glyph tables are far too big to hand over and the editor has no use for
/// them: it chooses which signal feeds which cells, and the daemon does the
/// drawing. What it does need is how many cells there are, so a cell run can be
/// checked before it is saved.
/// One font's glyphs, at both sizes, for drawing a preview.
///
/// Each glyph is its rows exactly as the font file writes them, `.` for a dark
/// pixel and `X` for a lit one, because that is already the shape a canvas
/// wants and converting it here would only mean converting it back.
#[derive(Serialize)]
pub struct FontGlyphs {
    pub width: usize,
    pub height: usize,
    /// Character to its rows, at full size.
    pub large: BTreeMap<char, Vec<String>>,
    /// Character to its rows, small. Fewer entries than `large` in every font
    /// here.
    pub small: BTreeMap<char, Vec<String>>,
}

impl FontGlyphs {
    /// Read one font of a text grid, named the way a profile names it.
    pub fn load(text: &dsc_config::TextGrid, file: &str) -> Result<Self, String> {
        let font = dsc_config::mcdu_font::McduFont::load(&text.path(file))
            .map_err(|e| format!("reading the font {file}: {e}"))?;
        let rows = |gs: &[dsc_config::mcdu_font::Glyph]| {
            gs.iter()
                .map(|g| (g.character, g.bit_array.clone()))
                .collect::<BTreeMap<char, Vec<String>>>()
        };
        Ok(FontGlyphs {
            width: font.glyph_width,
            height: font.glyph_height,
            large: rows(&font.large_glyphs),
            small: rows(&font.small_glyphs),
        })
    }
}

/// A font's characters as one sorted string, for the window to check against.
fn sorted(set: Option<&std::collections::HashSet<char>>) -> String {
    let mut chars: Vec<char> = set.map(|s| s.iter().copied().collect()).unwrap_or_default();
    chars.sort_unstable();
    chars.into_iter().collect()
}

/// A font's name for the picker, taken from its file rather than read out of
/// it, so listing the fonts does not mean loading four of them.
///
/// `../mcdu/ah64d-font-21x31.json` is the AH64D font. Anything that does not
/// look like that keeps its file name, which is still better than nothing.
fn font_name(file: &str) -> String {
    Path::new(file)
        .file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.split("-font-").next())
        .map(|s| s.to_uppercase())
        .unwrap_or_else(|| file.to_string())
}

#[derive(Serialize)]
pub struct DisplayView {
    pub key: String,
    pub cells: usize,
    /// Cell index to shape, so the window can say why a run will not take
    /// letters before the user tries it.
    pub shapes: Vec<String>,
    /// Named areas of the glass, in cell order. The window offers these instead
    /// of asking for a cell run, because a cell run is not something anyone
    /// deciding what to put on a panel can be expected to know.
    pub regions: Vec<RegionView>,
    /// Whether this glass can draw a character inverse. The window offers a
    /// highlighting signal only where it can, because `validate` rejects one
    /// on a display that cannot: it would do nothing.
    pub draws_inverse: bool,
    /// Whether this glass is a text grid, which is what decides whether a
    /// divider is worth offering. A segment display draws from a glyph table
    /// with no rule in it, and `validate` rejects one there.
    pub text_grid: bool,
    /// The colours this glass can draw, in the order the panel indexes them.
    /// Empty on anything but a text grid, which is the only kind that has a
    /// colour to choose.
    pub colours: Vec<String>,
    /// Every font this glass can be given, for a text grid. Empty otherwise.
    ///
    /// The window needs the alphabets to say what can be typed, which differs
    /// per font and per size: only one of these has lowercase, and each draws
    /// fewer characters small than large.
    pub fonts: Vec<FontChoice>,
    /// Runtime aircraft name to the font its own CDU matches.
    ///
    /// An aircraft in here takes that font and is offered no choice, because
    /// the glyphs were drawn to match what its module sends. The window checks
    /// the profile's aircraft against this to decide whether to ask.
    pub native_fonts: BTreeMap<String, String>,
    /// Per shape, what a lit slot looks like, so the window can draw a field
    /// the way this glass will. Empty on a text grid, which draws from a font
    /// the window reads instead, and on any shape nothing here can picture.
    ///
    /// Sent with the displays rather than asked for, unlike a font: this is a
    /// dozen numbers a shape, not four fonts of bitmaps.
    pub art: BTreeMap<String, ShapeArt>,
}

/// What one cell of a display would light, drawing a value.
///
/// The glyph tables are the daemon's, and so is the lookup: which entry a
/// value lands on depends on the cell it is drawn in, and every quirk of that
/// is in `Display::glyph`. The window asks rather than working it out, so a
/// preview cannot draw a character the panel will not.
#[derive(Serialize)]
pub struct CellInk {
    /// The slots lit, with any inverse flip already applied.
    pub lit: Vec<u8>,
    /// Which shape's art draws them.
    pub shape: String,
    /// Whether the glyph table had anything for the value. A cell it does not
    /// is dark on the panel, and the window marks it rather than leaving a
    /// blank that reads as a space.
    pub drawn: bool,
}

/// One cell of a field as the window has laid it out.
#[derive(Deserialize)]
pub struct CellDraw {
    pub cell: usize,
    pub value: String,
    #[serde(default)]
    pub inverse: bool,
}

impl CellInk {
    /// What each of these cells would light. A cell the display does not have
    /// is reported as drawing nothing rather than refused: the window checks
    /// a cell run as it is typed, and half a run is a normal thing to be
    /// looking at mid-keystroke.
    pub fn of(display: &Display, cells: &[CellDraw]) -> Vec<CellInk> {
        cells
            .iter()
            .map(|want| {
                let Some(cell) = display.cell(want.cell) else {
                    return CellInk {
                        lit: Vec::new(),
                        shape: String::new(),
                        drawn: false,
                    };
                };
                let lit = display.lit(cell, &want.value, want.inverse);
                CellInk {
                    drawn: lit.is_some(),
                    lit: lit.unwrap_or_default(),
                    shape: cell.shape.clone(),
                }
            })
            .collect()
    }
}

/// One font a text grid can be given.
#[derive(Serialize)]
pub struct FontChoice {
    /// Path relative to the display, which is what a profile stores.
    pub file: String,
    /// The font's own name, which is the aircraft it was drawn for.
    pub name: String,
    /// Every character it draws at full size, sorted.
    pub large: String,
    /// Every character it draws small, sorted. A subset of `large` in all of
    /// them, so marking a piece small can take a character away.
    pub small: String,
}

#[derive(Serialize)]
pub struct RegionView {
    pub name: String,
    pub cells: String,
    pub note: String,
}

impl DeviceView {
    pub fn of(spec: &DeviceSpec) -> Self {
        DeviceView {
            key: spec.key.clone(),
            display_name: spec.display_name.clone(),
            product_name: spec.product_name.clone(),
            leds: spec.leds().map(|(part, led)| LedView::of(part.part_id, led)).collect(),
            displays: Vec::new(),
            variants: Vec::new(),
        }
    }

    /// Fill in the other devices that are this one under another name.
    pub fn with_variants(mut self, spec: &DeviceSpec, all: &[DeviceSpec]) -> Self {
        self.variants = all
            .iter()
            .filter(|d| d.key != spec.key && d.same_hardware(spec))
            .map(|d| d.key.clone())
            .collect();
        self
    }

    /// Fill in the display descriptions from the loaded maps.
    pub fn with_displays(mut self, spec: &DeviceSpec, maps: &DisplayCatalogue) -> Self {
        self.displays = spec
            .displays()
            .filter_map(|(_, key)| maps.get(key))
            .map(|d| DisplayView {
                key: d.key.clone(),
                cells: d.cells.len(),
                shapes: d.cells.iter().map(|c| c.shape.clone()).collect(),
                regions: d
                    .regions
                    .iter()
                    .map(|r| RegionView {
                        name: r.name.clone(),
                        cells: r.cells.clone(),
                        note: r.note.clone(),
                    })
                    .collect(),
                draws_inverse: d.draws_inverse(),
                text_grid: d.is_text_grid(),
                colours: if d.is_text_grid() {
                    Colour::ALL.iter().map(|c| c.name().to_string()).collect()
                } else {
                    Vec::new()
                },
                fonts: d
                    .text
                    .as_ref()
                    .map(|t| {
                        t.fonts()
                            .into_iter()
                            .map(|file| FontChoice {
                                file: file.to_string(),
                                name: font_name(file),
                                large: sorted(t.charset(file, false)),
                                small: sorted(t.charset(file, true)),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                native_fonts: d
                    .text
                    .as_ref()
                    .map(|t| t.native_fonts.iter().map(|(a, f)| (a.clone(), f.clone())).collect())
                    .unwrap_or_default(),
                art: d.shape_art(),
            })
            .collect();
        self
    }
}

/// One row in the profile list.
#[derive(Serialize)]
pub struct ProfileSummary {
    pub file: String,
    pub name: String,
    pub module: String,
    pub aircraft: Vec<String>,
    /// The family of each of `aircraft`, in order: which aircraft could be
    /// handed to this profile. See `dsc_config::Families`.
    pub families: Vec<String>,
    /// Lamps assigned in any form (not placeholders), against the total listed.
    pub bound: usize,
    pub total: usize,
    /// Whether a shipped default exists to reset back to. A profile the user
    /// created themselves has none, and offering the button would be a lie.
    pub has_default: bool,
    /// Set when the file would not parse. The row is still listed: a profile
    /// the user can see on disk but not in the editor is worse than a broken one.
    pub error: Option<String>,
}

impl ProfileSummary {
    pub fn of(p: &Profile, file: String, has_default: bool, families: &Families) -> Self {
        ProfileSummary {
            file,
            name: p.name.clone(),
            module: p.module.clone(),
            aircraft: p.aircraft.clone(),
            families: p.aircraft.iter().map(|a| families.of(a, &p.module)).collect(),
            bound: p.bindings.iter().filter(|b| !b.is_placeholder()).count(),
            total: p.bindings.len(),
            has_default,
            error: None,
        }
    }

    pub fn broken(file: String, has_default: bool, error: String) -> Self {
        ProfileSummary {
            name: file.clone(),
            file,
            module: String::new(),
            aircraft: Vec::new(),
            families: Vec::new(),
            bound: 0,
            total: 0,
            has_default,
            error: Some(error),
        }
    }
}

/// A module offered when creating a profile.
#[derive(Serialize)]
pub struct ModuleChoice {
    pub key: String,
    /// Runtime names DCS reports for this module. Shown under the entry, since
    /// one module commonly serves several and the mapping is not guessable.
    pub aircraft: Vec<String>,
    pub signals: usize,
    pub lamps: usize,
}

/// `index.json` as `dsc_config::catalogue_build` writes it.
#[derive(Deserialize)]
struct Index {
    #[serde(default)]
    modules: BTreeMap<String, IndexEntry>,
}

#[derive(Deserialize)]
struct IndexEntry {
    #[serde(default)]
    aircraft: Vec<String>,
    #[serde(default)]
    signals: usize,
    #[serde(default)]
    lamps: usize,
}

impl ModuleChoice {
    /// Read the module list without touching the modules themselves.
    ///
    /// A missing index is an empty list rather than an error: the catalogue is
    /// generated from the user's own DCS-BIOS and will not exist until that has
    /// been done once. The window says so in words the user can act on, which a
    /// failed command could not.
    pub fn read_index(path: &Path) -> Result<Vec<ModuleChoice>, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(_) => return Ok(Vec::new()),
        };
        let index: Index = serde_json::from_str(&text)
            .map_err(|e| format!("reading {}: {e}", path.display()))?;
        let mut out: Vec<ModuleChoice> = index
            .modules
            .into_iter()
            .map(|(key, entry)| ModuleChoice {
                key,
                aircraft: entry.aircraft,
                signals: entry.signals,
                lamps: entry.lamps,
            })
            .collect();
        out.sort_by(|a, b| a.key.to_lowercase().cmp(&b.key.to_lowercase()));
        Ok(out)
    }
}

/// One bindable signal, as the typeahead and the hint box need it.
///
/// String outputs are carried but flagged. A lamp binding compares a number, so
/// a signal whose value is text can never satisfy one and the lamp picker hides
/// it. A display field is the opposite case: text is exactly what it wants, and
/// dropping these here would make the UFC's own signals unreachable.
#[derive(Serialize)]
pub struct SignalView {
    pub id: String,
    /// The human label. Every signal in every catalogued module has one, which
    /// is why the typeahead can search on it rather than on identifiers.
    pub description: String,
    /// The cockpit panel. Not decoration: descriptions repeat heavily inside a
    /// single module, 696 of CH-47F's 1,440 signals share one, and the category
    /// is what separates the six identical `Call Button Light (Yellow)` rows.
    pub category: String,
    pub control_type: String,
    /// True for cockpit lamps, which sort first because they are the likely
    /// intent when mapping a panel lamp.
    pub lamp: bool,
    pub max_value: u32,
    /// True when this signal reports characters rather than a number. Decides
    /// which picker offers it, and whether a display field needs a gauge range.
    pub text: bool,
    /// Characters in the field, for a text signal. A field wider than the cells
    /// it is given gets cropped, and the window can say so before it is saved.
    pub length: u32,
    /// Description of the reading itself, such as "0 if light is off, 1 if
    /// light is on". Shown in the hint box.
    pub reads: String,
    /// Present for signals with few enough values to label individually, such
    /// as a three-position switch. The editor offers these instead of a number.
    pub values: Vec<ValueLabel>,
}

impl SignalView {
    /// Every readable numeric signal in one module, lamps first.
    ///
    /// Only the profile's own module is ever loaded. The catalogue is about
    /// 11 MB across fifty files and nothing here needs the other forty-nine.
    pub fn of_module(path: &Path) -> Result<Vec<SignalView>, String> {
        let module = Module::load(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
        let mut out: Vec<SignalView> = module
            .signals
            .iter()
            .filter_map(|sig| {
                let out = sig.primary()?;
                let text = out.r#type == "string" || out.max_length.is_some();
                Some(SignalView {
                    id: sig.id.clone(),
                    description: sig.description.clone(),
                    category: sig.category.clone(),
                    control_type: sig.control_type.clone(),
                    lamp: sig.is_lamp(),
                    max_value: out.max_value.unwrap_or(u16::MAX as u32),
                    text,
                    length: u32::from(out.max_length.unwrap_or(0)),
                    reads: out.description.clone(),
                    values: if out.discrete { out.values.clone() } else { Vec::new() },
                })
            })
            .collect();
        // Lamps first, then everything else alphabetically by what the user
        // reads rather than by identifier.
        out.sort_by(|a, b| {
            b.lamp
                .cmp(&a.lamp)
                .then_with(|| a.description.to_lowercase().cmp(&b.description.to_lowercase()))
        });
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dsc_config::paths::Paths;
    use dsc_config::DeviceInventory;

    /// The window offers a highlighting signal only where the glass can draw
    /// one, and `validate` refuses it anywhere else, so the two have to agree.
    /// The UFC is the case that matters: it is pixel glass like the DED, and
    /// nothing about it on screen says it has no inverse form.
    #[test]
    fn only_glass_that_draws_inverse_says_so() {
        let paths = Paths::resolve();
        let maps = DisplayCatalogue::load_dir(&paths.displays).expect("the shipped display maps load");
        let inv = DeviceInventory::load(&paths.devices).expect("the shipped inventory loads");
        let views: Vec<DisplayView> = inv
            .devices
            .iter()
            .flat_map(|d| DeviceView::of(d).with_displays(d, &maps).displays)
            .collect();
        assert!(!views.is_empty(), "some device has glass");
        for view in views {
            let expected = match view.key.as_str() {
                // Pixel glass with inverse rows, and a text grid, which always
                // has an inverse form.
                "DED" | "MCDU" => true,
                // Seven segment and fixed shapes: no slots to flip.
                "UFC1" => false,
                other => panic!("unmapped display {other:?}; say whether it draws inverse"),
            };
            assert_eq!(view.draws_inverse, expected, "{}", view.key);
        }
    }
}
