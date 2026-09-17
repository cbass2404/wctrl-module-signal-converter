//! Shapes the window receives.
//!
//! These exist so the frontend is not handed the on-disk structures directly.
//! `data/devices.json` carries measurement notes and verification flags that
//! are meaningful to whoever is mapping hardware and noise to everyone else,
//! and a profile summary is cheaper than the profile it summarises.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use wctrl_config::{DeviceSpec, DisplayCatalogue, Led, Module, Profile, ValueLabel};

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
}

/// A segment display, described only as far as the window needs it.
///
/// The glyph tables are far too big to hand over and the editor has no use for
/// them: it chooses which signal feeds which cells, and the daemon does the
/// drawing. What it does need is how many cells there are, so a cell run can be
/// checked before it is saved.
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
        }
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
    /// Lamps with at least one condition, against the total listed.
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
    pub fn of(p: &Profile, file: String, has_default: bool) -> Self {
        ProfileSummary {
            file,
            name: p.name.clone(),
            module: p.module.clone(),
            aircraft: p.aircraft.clone(),
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

/// `index.json` as `tools/build_catalogue.py` writes it.
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
