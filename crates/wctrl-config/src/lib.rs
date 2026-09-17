//! Catalogue, device inventory and profile types.
//!
//! The catalogue is generated from DCS-BIOS by `tools/build_catalogue.py`; the
//! device inventory is `data/devices.json`. Profiles are authored by the user,
//! keyed by LED rather than by signal  see docs/CONFIG.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json in {1}: {0}")]
    Json(serde_json::Error, String),
    #[error("no catalogue entry for aircraft {0:?}")]
    UnknownAircraft(String),
    #[error("profile references unknown signal {0:?}")]
    UnknownSignal(String),
    #[error("profile references unknown LED {0:?} on device {1:?}")]
    UnknownLed(String, String),
    #[error("LED {0:?} was given on={1}, above its maximum of {2}")]
    OutOfRange(String, u8, u8),
    #[error("no shipped default named {0:?} to reset from")]
    NoDefault(String),
}

pub type Result<T> = std::result::Result<T, Error>;

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string()))
}

// ---------------------------------------------------------------- catalogue

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub address: u16,
    pub mask: Option<u16>,
    #[serde(default)]
    pub shift: u8,
    pub max_value: Option<u32>,
    /// Byte length of a string output. Sized separately from `max_value`
    /// because DCS-BIOS uses `max_length` for strings, and the decoder needs it
    /// to know how many bytes to read.
    pub max_length: Option<u16>,
    #[serde(default = "default_type")]
    pub r#type: String,
    #[serde(default)]
    pub description: String,
    /// True when the signal has few enough values to offer as a dropdown.
    #[serde(default)]
    pub discrete: bool,
    /// Present when `discrete`: the exact values the signal can report.
    #[serde(default)]
    pub values: Vec<ValueLabel>,
}

fn default_type() -> String {
    "integer".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValueLabel {
    pub value: u32,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: String,
    #[serde(default)]
    pub category: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub control_type: String,
    pub outputs: Vec<Output>,
}

impl Signal {
    /// The output a binding reads. Controls occasionally publish more than one;
    /// the first is the canonical value.
    pub fn primary(&self) -> Option<&Output> {
        self.outputs.first()
    }

    /// Whether this is a cockpit lamp, as opposed to a switch, dial or gauge.
    /// Lamps sort first in the editor, but every signal is bindable.
    pub fn is_lamp(&self) -> bool {
        self.control_type == "led"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub module: String,
    #[serde(default)]
    pub aircraft: Vec<String>,
    pub signals: Vec<Signal>,
}

impl Module {
    pub fn load(path: &Path) -> Result<Self> {
        read_json(path)
    }

    pub fn signal(&self, id: &str) -> Option<&Signal> {
        self.signals.iter().find(|s| s.id == id)
    }
}

/// Every module in `data/catalogue`, indexed by the aircraft names DCS reports.
#[derive(Debug, Default)]
pub struct Catalogue {
    modules: HashMap<String, Module>,
    by_aircraft: HashMap<String, String>,
}

impl Catalogue {
    pub fn load_dir(dir: &Path) -> Result<Self> {
        let mut cat = Catalogue::default();
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if path.file_name().and_then(|f| f.to_str()) == Some("index.json") {
                continue;
            }
            let module: Module = read_json(&path)?;
            for aircraft in &module.aircraft {
                cat.by_aircraft
                    .insert(aircraft.clone(), module.module.clone());
            }
            cat.modules.insert(module.module.clone(), module);
        }
        Ok(cat)
    }

    /// Build a catalogue from modules already in memory.
    ///
    /// The editor holds modules it has parsed or edited without writing them to
    /// disk first, and tests need a fixture that does not depend on the
    /// generated `data/catalogue` (which is machine-local and gitignored).
    pub fn from_modules(modules: Vec<Module>) -> Self {
        let mut cat = Catalogue::default();
        for module in modules {
            for aircraft in &module.aircraft {
                cat.by_aircraft
                    .insert(aircraft.clone(), module.module.clone());
            }
            cat.modules.insert(module.module.clone(), module);
        }
        cat
    }

    /// Resolve the runtime aircraft name (`LoGetSelfData().Name`) to a module.
    pub fn for_aircraft(&self, aircraft: &str) -> Option<&Module> {
        self.by_aircraft
            .get(aircraft)
            .and_then(|key| self.modules.get(key))
    }

    pub fn module(&self, key: &str) -> Option<&Module> {
        self.modules.get(key)
    }

    pub fn modules(&self) -> impl Iterator<Item = &Module> {
        self.modules.values()
    }
}

// ------------------------------------------------------------------ devices

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedKind {
    /// Accepts the full 0..=255 range.
    Dimmer,
    /// Accepts 0 or 1 only. Writing 255 is out of range: the device acks it and
    /// nothing lights, which is not the same as "off" and cost us a long
    /// debugging detour. `max` is 1 for these, and `resolve` clamps to it.
    Indicator,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Led {
    pub index: u8,
    pub name: String,
    #[serde(default)]
    pub label: String,
    pub kind: LedKind,
    /// Highest value the lamp accepts. Absent means unmeasured - deliberately
    /// not defaulted to 255, because assuming that produced writes that acked
    /// and did nothing.
    pub max: Option<u8>,
    /// Value meaning "on". Indicators want 1, not 255.
    pub on_value: Option<u8>,
    #[serde(default)]
    pub verified: bool,
}

impl Led {
    /// Highest value this lamp accepts, falling back to its kind.
    pub fn max_value(&self) -> u8 {
        self.max.unwrap_or(match self.kind {
            LedKind::Dimmer => 255,
            LedKind::Indicator => 1,
        })
    }

    /// The value to write for "fully on".
    pub fn on_value(&self) -> u8 {
        self.on_value.unwrap_or(self.max_value())
    }

    pub fn is_dimmable(&self) -> bool {
        self.kind == LedKind::Dimmer
    }
}

/// A sub-device addressed by its own part id. One USB interface can host
/// several: the Orion II answers as base plus two handles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub part_id: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub leds: Vec<Led>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSpec {
    pub key: String,
    pub display_name: String,
    #[serde(default)]
    pub product_name: String,
    pub usb_pid: u16,
    pub parts: Vec<Part>,
}

impl DeviceSpec {
    /// Find an LED by name across every part, returning the part that owns it.
    pub fn led(&self, name: &str) -> Option<(&Part, &Led)> {
        self.parts
            .iter()
            .find_map(|p| p.leds.iter().find(|l| l.name == name).map(|l| (p, l)))
    }

    /// Every LED on the device, with its owning part.
    pub fn leds(&self) -> impl Iterator<Item = (&Part, &Led)> {
        self.parts.iter().flat_map(|p| p.leds.iter().map(move |l| (p, l)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInventory {
    pub devices: Vec<DeviceSpec>,
}

impl DeviceInventory {
    pub fn load(path: &Path) -> Result<Self> {
        read_json(path)
    }

    pub fn device(&self, key: &str) -> Option<&DeviceSpec> {
        self.devices.iter().find(|d| d.key == key)
    }
}

// ----------------------------------------------------------------- profiles

/// When a binding considers its source "on".
///
/// `Scale` is the odd one out: it has no threshold, and the lamp follows the
/// source continuously. It is what backlights and dimmers use.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnWhen {
    Equals(u32),
    In(Vec<u32>),
    Gte(u32),
    Lte(u32),
    Between([u32; 2]),
    Scale([u32; 2]),
}

impl OnWhen {
    /// Resolve a raw signal value to a lamp value in `0..=led_max`.
    pub fn resolve(&self, value: u32, on: u8, off: u8, led_max: u8) -> u8 {
        let lit = match self {
            OnWhen::Equals(v) => value == *v,
            OnWhen::In(vs) => vs.contains(&value),
            OnWhen::Gte(v) => value >= *v,
            OnWhen::Lte(v) => value <= *v,
            OnWhen::Between([lo, hi]) => value >= *lo && value <= *hi,
            OnWhen::Scale([lo, hi]) => {
                let (lo, hi) = (*lo, *hi);
                if hi <= lo {
                    return off;
                }
                let clamped = value.clamp(lo, hi) - lo;
                let span = hi - lo;
                // u64 so the multiply cannot overflow on a 0..65535 source.
                let scaled = (clamped as u64 * led_max as u64) / span as u64;
                return scaled as u8;
            }
        };
        if lit {
            on.min(led_max)
        } else {
            off
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Binding {
    pub device: String,
    pub led: String,
    /// Every condition must hold for the lamp to light.
    ///
    /// An empty list is a placeholder: the user has not configured this lamp
    /// yet. That is a normal state, not an error. The editor lists every lamp on
    /// the hardware whether or not it is configured, and a half-filled profile
    /// has to load and run so it can be filled in one lamp at a time.
    #[serde(default)]
    pub conditions: Vec<Condition>,
    /// Omitted means "fully on for this lamp", resolved from the LED itself.
    pub on: Option<u8>,
    #[serde(default)]
    pub off: u8,
    /// Why this mapping, in the author's words. Profiles are meant to be
    /// shared, and a bare signal id does not say whether a mapping is the
    /// obvious counterpart or someone's deliberate reinterpretation of a spare
    /// lamp. The editor shows it; nothing at runtime reads it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// One signal and the test applied to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub source: String,
    pub on_when: OnWhen,
}

impl Binding {
    /// A row that names a lamp but drives nothing.
    pub fn is_placeholder(&self) -> bool {
        self.conditions.is_empty()
    }

    /// Resolve every condition against current signal values and combine them.
    ///
    /// `read` returns the raw value of a signal, or `None` if it has not been
    /// seen yet. An unseen signal makes the whole binding unresolved rather than
    /// false, so lamps hold their swept value instead of flickering while the
    /// post-load flood arrives.
    ///
    /// Conditions are combined by taking the **dimmest** value any of them asks
    /// for. That single rule gives boolean AND for on/off tests, since each
    /// resolves to either `on` or zero, and it also lets a continuous source be
    /// gated: a scaled backlight behind a power switch yields the scaled value
    /// while the switch is on and zero while it is off.
    pub fn resolve<F>(&self, led: &Led, mut read: F) -> Option<u8>
    where
        F: FnMut(&str) -> Option<u32>,
    {
        if self.conditions.is_empty() {
            return None;
        }
        let on = self.on.unwrap_or_else(|| led.on_value());
        let led_max = led.max_value();

        let mut value = u8::MAX;
        for condition in &self.conditions {
            let raw = read(&condition.source)?;
            // `off` is deliberately zero here rather than `self.off`: it is the
            // combining identity for "this condition is not met". The binding's
            // own `off` is applied once, at the end.
            let resolved = condition.on_when.resolve(raw, on, 0, led_max);
            value = value.min(resolved);
            if value == 0 {
                break;
            }
        }

        Some(if value == 0 { self.off } else { value })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    #[serde(default = "default_schema")]
    pub schema_version: u32,
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub profile_version: String,
    /// Runtime aircraft names this profile serves.
    pub aircraft: Vec<String>,
    /// Catalogue key the signal ids resolve against.
    pub module: String,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}

fn default_schema() -> u32 {
    1
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self> {
        read_json(path)
    }

    /// A profile with one unassigned row per LED on every device given.
    ///
    /// This is what gets written when the user flies an aircraft nothing is
    /// configured for. Every lamp appears, none of them drive anything, and the
    /// user fills them in over time. Starting from a complete list of their
    /// hardware beats starting from an empty file, because the question the
    /// editor asks is "what should this lamp do", not "which lamps exist".
    pub fn stub(name: &str, aircraft: &str, module: &str, devices: &DeviceInventory) -> Self {
        let bindings = devices
            .devices
            .iter()
            .flat_map(|d| {
                d.leds().map(|(_, led)| Binding {
                    device: d.key.clone(),
                    led: led.name.clone(),
                    conditions: Vec::new(),
                    on: None,
                    off: 0,
                    note: String::new(),
                })
            })
            .collect();

        Profile {
            schema_version: default_schema(),
            name: name.to_string(),
            author: String::new(),
            profile_version: "0.1.0".to_string(),
            aircraft: vec![aircraft.to_string()],
            module: module.to_string(),
            bindings,
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Json(e, path.display().to_string()))?;
        std::fs::write(path, text)?;
        Ok(())
    }

    /// Check every binding resolves against the catalogue and the hardware.
    ///
    /// Worth doing on load rather than at evaluation time: a profile shared by
    /// someone with different hardware, or built against a newer DCS-BIOS,
    /// should fail loudly once instead of silently never lighting a lamp.
    pub fn validate(&self, module: &Module, devices: &DeviceInventory) -> Result<()> {
        for b in &self.bindings {
            // The lamp must exist even on a placeholder row: it names real
            // hardware. Only the conditions are allowed to be undecided.
            for condition in &b.conditions {
                if module.signal(&condition.source).is_none() {
                    return Err(Error::UnknownSignal(condition.source.clone()));
                }
            }
            let device = devices
                .device(&b.device)
                .ok_or_else(|| Error::UnknownLed(b.led.clone(), b.device.clone()))?;
            let (_, led) = device
                .led(&b.led)
                .ok_or_else(|| Error::UnknownLed(b.led.clone(), b.device.clone()))?;
            // A profile asking for a brightness an indicator cannot produce is
            // a real authoring error, not something to silently clamp away.
            if let Some(on) = b.on {
                if on > led.max_value() {
                    return Err(Error::OutOfRange(b.led.clone(), on, led.max_value()));
                }
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------- shipped copies

/// Shipped profiles, and the folder the user actually edits.
///
/// Profiles ship as a product rather than a sample, so they are copied into the
/// active folder rather than consulted as a second layer at load time. One
/// folder is in use, which means what a user sees in it is what runs: nothing is
/// shadowed and no lookup order has to be explained.
///
/// Seeding only ever adds. A profile the user has is theirs, and an update never
/// rewrites it. The deliberate cost is that a correction shipped to a default
/// never reaches someone who already has that profile, including someone who
/// never opened it. [`reset_to_default`] is the remedy, and it is the only path
/// that overwrites.
pub struct Profiles {
    pub defaults: PathBuf,
    pub active: PathBuf,
}

impl Profiles {
    pub fn new(defaults: impl Into<PathBuf>, active: impl Into<PathBuf>) -> Self {
        Profiles {
            defaults: defaults.into(),
            active: active.into(),
        }
    }

    /// Copy in every default the active folder does not already have, returning
    /// the file names copied. Creates the active folder if it is missing.
    ///
    /// Safe to run on every start, which is the point: install, update and a
    /// user who deleted the folder all take the same path.
    pub fn seed(&self) -> Result<Vec<String>> {
        if !self.defaults.is_dir() {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(&self.active)?;
        let mut copied = Vec::new();
        for entry in std::fs::read_dir(&self.defaults)? {
            let from = entry?.path();
            if from.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let Some(name) = from.file_name() else { continue };
            let to = self.active.join(name);
            if to.exists() {
                continue;
            }
            std::fs::copy(&from, &to)?;
            copied.push(name.to_string_lossy().into_owned());
        }
        copied.sort();
        Ok(copied)
    }

    /// True when a shipped default exists for this file name, which is what
    /// decides whether the editor offers a reset button on the row.
    pub fn has_default(&self, file: &str) -> bool {
        self.defaults.join(file).is_file()
    }

    /// Overwrite one active profile with its shipped default.
    ///
    /// The only call that destroys user work, so it is never reached except by
    /// someone clicking reset.
    pub fn reset_to_default(&self, file: &str) -> Result<()> {
        let from = self.defaults.join(file);
        if !from.is_file() {
            return Err(Error::NoDefault(file.to_string()));
        }
        std::fs::create_dir_all(&self.active)?;
        std::fs::copy(&from, self.active.join(file))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equals_drives_full_brightness() {
        let w = OnWhen::Equals(1);
        assert_eq!(w.resolve(1, 255, 0, 255), 255);
        assert_eq!(w.resolve(0, 255, 0, 255), 0);
    }

    #[test]
    fn on_is_clamped_to_the_lamp_range() {
        // Master_Caution is max 1; asking for 255 must not overflow it.
        assert_eq!(OnWhen::Equals(1).resolve(1, 255, 0, 1), 1);
    }

    #[test]
    fn multi_position_switch() {
        let w = OnWhen::In(vec![1, 2]);
        assert_eq!(w.resolve(0, 255, 0, 255), 0);
        assert_eq!(w.resolve(1, 255, 0, 255), 255);
        assert_eq!(w.resolve(2, 255, 0, 255), 255);
    }

    #[test]
    fn scale_maps_a_dimmer_across_the_lamp_range() {
        let w = OnWhen::Scale([0, 65535]);
        assert_eq!(w.resolve(0, 255, 0, 255), 0);
        assert_eq!(w.resolve(65535, 255, 0, 255), 255);
        assert_eq!(w.resolve(32768, 255, 0, 255), 127);
    }

    #[test]
    fn scale_with_a_degenerate_range_is_off_not_a_panic() {
        assert_eq!(OnWhen::Scale([10, 10]).resolve(10, 255, 0, 255), 0);
    }

    /// Parses the real inventory, so a change to `data/devices.json` that the
    /// Rust types cannot represent fails here rather than at runtime.
    #[test]
    fn ships_a_parseable_device_inventory() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json");
        let inventory = DeviceInventory::load(&path).expect("data/devices.json parses");

        let pto2 = inventory
            .device("TAKEOFF_PLANEL_2")
            .expect("PTO2 present in the inventory");

        // Indices 0, 1, 2 are dimmers; 4..=17 are indicators. Measured, not assumed.
        let (_, backlight) = pto2.led("Backlight").expect("Backlight present");
        assert_eq!(backlight.kind, LedKind::Dimmer);
        assert_eq!(backlight.max_value(), 255);

        for name in ["Master_Caution", "HOOK", "FLAPS"] {
            let (_, led) = pto2.led(name).unwrap_or_else(|| panic!("{name} present"));
            assert_eq!(led.kind, LedKind::Indicator, "{name}");
            // The whole point: an indicator must never be driven with 255. The
            // device acks that and lights nothing, which reads as a dead lamp.
            assert_eq!(led.max_value(), 1, "{name}");
            assert_eq!(led.on_value(), 1, "{name}");
            assert_eq!(
                OnWhen::Equals(1).resolve(1, 255, 0, led.max_value()),
                1,
                "{name} must clamp a 255 request down to 1"
            );
        }
    }

    /// One USB interface can host several parts; the Orion II is the case that
    /// broke the earlier flat model.
    #[test]
    fn a_device_may_span_several_parts() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json");
        let inventory = DeviceInventory::load(&path).unwrap();
        let orion = inventory.device("Orion_Throttle_Base_II").unwrap();
        assert!(orion.parts.len() > 1, "base plus handles");

        let (part, aa) = orion.led("A/A").expect("A/A present");
        assert_eq!(part.part_id, 0xbe60, "A/A belongs to the base, not the USB pid");
        assert_eq!(aa.kind, LedKind::Indicator);
    }
}
