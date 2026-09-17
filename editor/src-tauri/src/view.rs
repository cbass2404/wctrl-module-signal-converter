//! Shapes the window receives.
//!
//! These exist so the frontend is not handed the on-disk structures directly.
//! `data/devices.json` carries measurement notes and verification flags that
//! are meaningful to whoever is mapping hardware and noise to everyone else,
//! and a profile summary is cheaper than the profile it summarises.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use wctrl_config::{DeviceSpec, Led, Profile};

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
}

impl DeviceView {
    pub fn of(spec: &DeviceSpec) -> Self {
        DeviceView {
            key: spec.key.clone(),
            display_name: spec.display_name.clone(),
            product_name: spec.product_name.clone(),
            leds: spec.leds().map(|(part, led)| LedView::of(part.part_id, led)).collect(),
        }
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
