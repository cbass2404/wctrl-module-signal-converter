//! Catalogue, device inventory and profile types.
//!
//! The catalogue is generated from DCS-BIOS by `tools/build_catalogue.py`; the
//! device inventory is `data/devices.json`. Profiles are authored by the user,
//! keyed by LED rather than by signal  see docs/CONFIG.md.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod display;
pub mod mcdu_font;

pub use display::{
    text_cells, Align, Cell, CellRange, Colour, ColourSource, Display, DisplayCatalogue, Grid,
    Readout, Region, Screen, TextCell, TextGrid, Transport, SEAT_SIGNAL,
};

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
    #[error("LED {0:?} has a condition with no signal chosen yet; pick one or delete the condition")]
    UnfinishedCondition(String),
    #[error("the field on {1} of display {0:?} has no signal chosen yet; pick one or remove the field")]
    UnfinishedField(String, String),
    #[error("profile references unknown LED {0:?} on device {1:?}")]
    UnknownLed(String, String),
    #[error("LED {0:?} was given on={1}, above its maximum of {2}")]
    OutOfRange(String, u8, u8),
    #[error("no shipped default named {0:?} to reset from")]
    NoDefault(String),
    #[error("{0:?} shipped with wctrl, so it cannot be deleted; reset it instead")]
    ShippedProfile(String),
    #[error("{0:?} is not a profile file name")]
    NotAProfileFile(String),
    #[error("LED {0:?} is set to always on but also carries conditions; it can have one or the other")]
    AlwaysWithConditions(String),
    #[error("LED {0:?} carries both conditions and any_of; put every alternative in any_of")]
    ConditionsWithAnyOf(String),
    #[error("LED {0:?} has an alternative in any_of with no conditions in it")]
    EmptyBranch(String),
    #[error("LED {0:?} picks between alternatives but has none; pick applies only to any_of")]
    PickWithoutAlternatives(String),
    #[error("LED {0:?} mirrors {1:?}, which is not a lamp on device {2:?}")]
    UnknownMirror(String, String, String),
    #[error("LED {0:?} mirrors {1:?}, which mirrors something itself; a mirror must point at a lamp that reads signals")]
    MirrorChain(String, String),
    #[error("LED {0:?} mirrors another lamp and also carries its own conditions; it can have one or the other")]
    MirrorWithConditions(String),
    #[error("LED {0:?} and {1:?} cannot mirror each other; only lamps that dim can, because an on/off lamp has no level to follow")]
    MirrorNotDimmable(String, String),
    #[error("display {0:?} is malformed: {1}")]
    BadDisplay(String, String),
    #[error("MCDU font {0:?} does not fit its upload: {1}")]
    BadFont(String, String),
    #[error("{0:?} has no font for {1}: its screen follows the aircraft's own CDU, and this one has none that DCS-BIOS exports")]
    NoNativeFont(String, String),
    #[error("{0:?} is not a character the font {1} draws, but cells {2} are told to put it there")]
    NotInFont(char, String, String),
    #[error("a replacement swaps one character for one character; {0:?} to {1:?} is not that")]
    ReplaceNotOneChar(String, String),
    #[error("a colour code is one character; {0:?} is not")]
    ColourCodeNotOneChar(String),
    #[error("cells {1} of display {0:?} are given a colour or size, which only a text grid draws")]
    StyleNotDrawn(String, String),
    #[error("display {0:?} has no cell {1}")]
    NoSuchCell(String, usize),
    #[error("{0:?} cannot be drawn on a {1} cell of display {2:?}")]
    NoSuchGlyph(String, String, String),
    #[error("device {0:?} has no display named {1:?}")]
    NoDisplayOnDevice(String, String),
    #[error("no display map named {0:?}; expected one in data/displays")]
    UnknownDisplay(String),
    #[error("display {0:?} has {1} cells, so the run {2} runs off the end of it")]
    CellsOutOfRange(String, usize, String),
    #[error("cell runs {0} and {1} on display {2:?} overlap; a field has one source")]
    CellsOverlap(String, String, String),
    #[error("{0:?} is a number, so it needs a range: what the gauge reads in the cockpit at each end of its travel")]
    RangeMissing(String),
    #[error("{0:?} already reports characters, so a range would mean nothing")]
    RangeOnText(String),
    #[error("profile disables device {0:?}, which is not a device we know")]
    DisablesUnknownDevice(String),
    #[error("a field is set to seat {0}, but module {1:?} does not report {2}, so nothing would ever be painted there")]
    SeatNotReported(u32, String, &'static str),
    #[error("seat {0} is not one this module has; {1} reports 0 to {2}")]
    NoSuchSeat(u32, &'static str, u32),
    #[error("display {0:?} cannot draw inverse characters, so the format signal on {1} would do nothing")]
    FormatNotDrawn(String, String),
    #[error("{0:?} is a number; a format signal has to be characters, one per cell")]
    FormatNotText(String),
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
    /// Anything worth knowing about this lamp that its name does not say, such
    /// as HOOK being physically dim rather than wrongly driven. Written while
    /// mapping the hardware, and shown in the editor so the next person does
    /// not have to rediscover it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// Lamps this dimmer hides when it is at 0, by name. The PTO2's SL gates
    /// every indicator and FLAG gates the flag lamps, so a profile that drives
    /// either to 0 with the cockpit dark blanks them in daylight. Empty for a
    /// dimmer that governs nothing, such as a backlight.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub governs: Vec<String>,
    /// This lamp lights its part's display. A profile binds it like any other
    /// lamp, but the binding only counts while the profile puts fields on that
    /// display: with nothing drawn the screen is held at 0, so it is black
    /// rather than lit and empty. A bound value not yet known is taken as full.
    ///
    /// The ICP's DED backlight and the MCDU's screen are the cases. Both start
    /// held at full ([`Binding::fresh`]), because at 0 a correctly drawn page
    /// is invisible.
    #[serde(default, skip_serializing_if = "is_false")]
    pub lights_display: bool,
    /// A panel backlight: legends or a lit feature, not an indicator and not a
    /// gate. Every shipped default drives all of these from one cockpit knob,
    /// and `tests/shipped_defaults.rs` holds them to it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub backlight: bool,
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
    /// Key of this part's segment display in `data/displays`, if it has one.
    ///
    /// Held on the part rather than the device because a part id is what a
    /// display write is addressed to, and one USB interface can host several
    /// parts of which only some have glass.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
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
    ///
    /// Names must be unique within a device, because a profile addresses a lamp
    /// by device and name and has no way to say which part it meant. The UFC
    /// found this the hard way: the vendor calls a lamp `INST_PNL_Backlight` on
    /// both the UFC part and the HUD part, and the second was simply
    /// unreachable, with no error anywhere. `every_lamp_name_is_unique_within_its_device`
    /// keeps that from happening again.
    pub fn led(&self, name: &str) -> Option<(&Part, &Led)> {
        self.leds().find(|(_, l)| l.name == name)
    }

    /// Every LED a profile can bind, with its owning part, including the lamps
    /// that light a display.
    pub fn leds(&self) -> impl Iterator<Item = (&Part, &Led)> {
        self.parts.iter().flat_map(|p| p.leds.iter().map(move |l| (p, l)))
    }

    /// The lamps that light a display, with their owning part.
    pub fn display_lamps(&self) -> impl Iterator<Item = (&Part, &Led)> {
        self.parts
            .iter()
            .flat_map(|p| p.leds.iter().filter(|l| l.lights_display).map(move |l| (p, l)))
    }

    /// The part carrying a named display.
    pub fn part_with_display(&self, key: &str) -> Option<&Part> {
        self.parts
            .iter()
            .find(|p| p.display.as_deref() == Some(key))
    }

    /// Every display this device carries, with its owning part.
    pub fn displays(&self) -> impl Iterator<Item = (&Part, &str)> {
        self.parts
            .iter()
            .filter_map(|p| p.display.as_deref().map(|d| (p, d)))
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
    /// Light this lamp whenever the profile is active, reading nothing.
    ///
    /// Distinct from an empty condition list, which means "not decided yet" and
    /// drives the lamp off. Some lamps have no counterpart in the cockpit and
    /// the honest answer is that the user simply wants them lit, and on a lamp
    /// that dims this is also how a fixed brightness is set: a panel backlight
    /// held at one level rather than following the cockpit dimmer.
    ///
    /// Mutually exclusive with `conditions`. Carrying both is rejected by
    /// `validate` rather than silently resolved, because either reading of it
    /// would be a guess at what the author meant.
    #[serde(default, skip_serializing_if = "is_false")]
    pub always: bool,
    /// Alternatives, any one of which lights the lamp.
    ///
    /// Each branch is its own `conditions` list and holds only when all of them
    /// hold, so this is a list of ANDs joined by OR. Any boolean expression can
    /// be written that way, and it avoids parentheses and precedence, which are
    /// what make a general expression editor hard to use and easy to misread.
    ///
    /// The case this exists for is a multicrew aircraft, where a lamp follows
    /// whichever station is occupied:
    ///
    /// ```jsonc
    /// "any_of": [
    ///   { "conditions": [ { "source": "STATION", "on_when": { "equals": 1 } },
    ///                     { "source": "CPG_BRIGHT", "on_when": { "scale": [0, 65535] } } ] },
    ///   { "conditions": [ { "source": "STATION", "on_when": { "equals": 0 } },
    ///                     { "source": "PLT_BRIGHT", "on_when": { "scale": [0, 65535] } } ] }
    /// ]
    /// ```
    ///
    /// Mutually exclusive with `conditions` and with `always`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub any_of: Vec<Branch>,
    /// How the alternatives in `any_of` combine. See [`Pick`].
    #[serde(default, skip_serializing_if = "Pick::is_brightest")]
    pub pick: Pick,
    /// Mirror another lamp on the same device, by name.
    ///
    /// A link rather than a copy: change what the other lamp reads and this one
    /// follows. The PTO2 is the case it exists for, because its three
    /// brightness governors are usually meant to sit at one level, and copying
    /// the conditions into all three means every later change has to be made
    /// three times or they drift apart silently.
    ///
    /// This binding's own `off` still applies when the mirrored lamp resolves
    /// to zero, which is what keeps the daylight floor available: `FLAG` can
    /// follow the backlight at night and still go full bright when the console
    /// is off.
    ///
    /// Chains are not allowed, so the target must read signals of its own.
    /// That rules out cycles without any cycle detection to get wrong.
    ///
    /// Mutually exclusive with `conditions`, `any_of` and `always`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_as: Option<String>,
    /// Omitted means "fully on for this lamp", resolved from the LED itself.
    ///
    /// Skipped when absent, and `off` when zero, so a profile the editor saves
    /// stays as readable as one written by hand. These files are shipped and
    /// diffed, and a rewrite that added `"on": null` to every lamp would bury
    /// the one line that actually changed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<u8>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub off: u8,
    /// Why this mapping, in the author's words. Profiles are meant to be
    /// shared, and a bare signal id does not say whether a mapping is the
    /// obvious counterpart or someone's deliberate reinterpretation of a spare
    /// lamp. The editor shows it; nothing at runtime reads it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// The dimmest value any of these conditions asks for, or `None` if a signal
/// has not been seen yet.
///
/// `off` is deliberately zero here rather than the binding's: it is the
/// combining identity for "this condition is not met". The binding's own `off`
/// is applied once, at the end.
fn all_of<F>(conditions: &[Condition], on: u8, led_max: u8, read: &mut F) -> Option<u8>
where
    F: FnMut(&str) -> Option<u32>,
{
    // Tests before scales. A test (a seat, a power switch) is often zero and
    // ends the loop, while a scale is only zero with its knob fully off, so a
    // gated branch never reads the source behind the gate. The dimmest-value
    // rule gives the same answer in any order; only the reads change, and with
    // them whether an unseen source behind a closed gate can hold the lamp.
    let is_scale = |c: &&Condition| matches!(c.on_when, OnWhen::Scale(_));
    let tests = conditions.iter().filter(|c| !is_scale(c));
    let mut value = u8::MAX;
    for condition in tests.chain(conditions.iter().filter(is_scale)) {
        let raw = read(condition.source.as_str())?;
        value = value.min(condition.on_when.resolve(raw, on, 0, led_max));
        if value == 0 {
            break;
        }
    }
    Some(value)
}

/// How a lamp's alternatives combine into one value.
///
/// A general rule, not a per-aircraft one: it says nothing about what the
/// signals are, only how their branches are chosen between.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Pick {
    /// The brightest branch wins. Boolean OR for on/off tests, and for a
    /// branch gated behind a seat test, the live seat supplies the value.
    #[default]
    Brightest,
    /// The branch whose signals last changed value wins, falling back to the
    /// brightest until one has.
    ///
    /// The case it exists for is a two-seat aircraft with a lighting knob per
    /// seat and no signal saying which seat the player is in. Taking the
    /// brightest would mean turning both knobs down to dim, and taking the
    /// dimmest both up to brighten. The knob last turned is the one in the
    /// player's hand; in multiplayer the other crew member can take it back by
    /// turning theirs.
    Latest,
}

impl Pick {
    fn is_brightest(&self) -> bool {
        *self == Pick::Brightest
    }
}

/// One alternative within `any_of`: conditions that must all hold together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Branch {
    #[serde(default)]
    pub conditions: Vec<Condition>,
}

/// One signal and the test applied to it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub source: String,
    pub on_when: OnWhen,
}

impl Binding {
    /// The row a lamp starts with when a profile first meets it, in a stub or
    /// in the merge that brings an old profile up to the hardware.
    ///
    /// Unassigned, except where unassigned means dark in a way the user cannot
    /// trace. A gate at 0 hides every lamp beneath it, and a screen's lamp at 0
    /// hides a correctly drawn page, so both start held at full. A gate also
    /// carries its daylight floor, so switching it to follow a cockpit dimmer
    /// later does not blank the panel by day.
    pub fn fresh(device: &str, led: &Led) -> Self {
        let gate = !led.governs.is_empty();
        let held = gate || led.lights_display;
        let note = if gate {
            "Held at full, because at 0 it hides the lamps beneath it. To follow a cockpit dimmer instead, keep the value at zero at full, since a dark cockpit means daylight."
        } else if led.lights_display {
            "Held at full, because at 0 the screen shows nothing. It lights only while this profile puts fields on the screen, whatever it is bound to, so a screen with nothing on it stays black."
        } else {
            ""
        };
        Binding {
            device: device.to_string(),
            led: led.name.clone(),
            conditions: Vec::new(),
            always: held,
            any_of: Vec::new(),
            pick: Pick::default(),
            same_as: None,
            on: None,
            off: if held { led.max_value() } else { 0 },
            note: note.to_string(),
        }
    }

    /// A row that names a lamp but drives nothing.
    ///
    /// An always-on lamp is configured, not undecided, so it is not a
    /// placeholder even though it reads no signals.
    pub fn is_placeholder(&self) -> bool {
        self.conditions.is_empty()
            && self.any_of.is_empty()
            && !self.always
            && self.same_as.is_none()
    }

    /// Every signal this binding reads, across all forms.
    ///
    /// The engine indexes a binding under each address it reads so it can be
    /// re-evaluated when one moves. Having one place that answers "what does
    /// this read" keeps that index correct as new forms are added: when
    /// `any_of` arrived, the index needed no knowledge of it.
    pub fn sources(&self) -> impl Iterator<Item = &str> {
        self.conditions
            .iter()
            .chain(self.any_of.iter().flat_map(|b| b.conditions.iter()))
            .map(|c| c.source.as_str())
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
    pub fn resolve<F>(&self, led: &Led, read: F) -> Option<u8>
    where
        F: FnMut(&str) -> Option<u32>,
    {
        self.resolve_with_moves(led, read, |_| None)
    }

    /// [`Binding::resolve`], told when each signal last changed value.
    ///
    /// `moved` answers with any number that grows with time, or `None` for a
    /// signal that has not changed since it was first seen. Only
    /// [`Pick::Latest`] asks it anything.
    pub fn resolve_with_moves<F, M>(&self, led: &Led, mut read: F, mut moved: M) -> Option<u8>
    where
        F: FnMut(&str) -> Option<u32>,
        M: FnMut(&str) -> Option<u64>,
    {
        let on = self.on.unwrap_or_else(|| led.on_value());
        let led_max = led.max_value();

        // Reads nothing, so it resolves the same on the module-load sweep as it
        // would at any other moment, and no later write ever revisits it.
        if self.always {
            return Some(on.min(led_max));
        }
        // Alternatives take the **brightest** branch, the exact dual of the
        // dimmest-condition rule within a branch. For on/off tests that is
        // boolean OR, and for a continuous source it means the branch that is
        // actually live supplies the value while the gated ones sit at zero.
        //
        // With `Pick::Latest` the branch whose signals changed most recently
        // supplies the value instead, dark or not: the knob just turned down is
        // the one the player means. A branch's time is the latest of any
        // signal it reads.
        if !self.any_of.is_empty() {
            let mut best = 0u8;
            let mut latest: Option<(u64, u8)> = None;
            for branch in &self.any_of {
                if branch.conditions.is_empty() {
                    continue;
                }
                let value = all_of(&branch.conditions, on, led_max, &mut read)?;
                best = best.max(value);
                if self.pick == Pick::Latest {
                    let at = branch.conditions.iter().filter_map(|c| moved(&c.source)).max();
                    if let Some(at) = at {
                        if latest.is_none_or(|(t, _)| at > t) {
                            latest = Some((at, value));
                        }
                    }
                }
            }
            let value = latest.map_or(best, |(_, v)| v);
            return Some(if value == 0 { self.off } else { value });
        }

        if self.conditions.is_empty() {
            return None;
        }
        let value = all_of(&self.conditions, on, led_max, &mut read)?;
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
    /// Fields of a segment display, and what feeds each of them.
    ///
    /// Separate from `bindings` because a lamp and a display field have nothing
    /// in common beyond both being output: one resolves to a brightness through
    /// conditions, the other to characters through a glyph table.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readouts: Vec<Readout>,
    /// Devices this aircraft should not drive at all.
    ///
    /// Not the same as binding nothing. An unbound device is still swept, so
    /// its lamps go dark and its glass goes blank, which is what you want for a
    /// panel you can see. A disabled device is never written to, which is what
    /// you want for one you cannot.
    ///
    /// The case this exists for is physical. A WinWing ICP and UFC share a
    /// swing arm: whichever is in use covers the other. Flying the Hornet with
    /// the ICP swung away, there is nothing to be gained by driving the ICP,
    /// and its panel lighting is better left as the user set it than forced to
    /// zero by a sweep.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub disabled_devices: Vec<String>,
}

fn default_schema() -> u32 {
    1
}

impl Profile {
    pub fn load(path: &Path) -> Result<Self> {
        read_json(path)
    }

    /// Whether this profile drives a device at all.
    ///
    /// A disabled device keeps whatever was last written to it, because panel
    /// state latches on this hardware. That is the point: it is hidden, and
    /// leaving its backlight where the user set it beats zeroing it.
    pub fn drives(&self, device: &str) -> bool {
        !self.disabled_devices.iter().any(|d| d == device)
    }

    /// A profile with one unassigned row per LED on every device given.
    ///
    /// This is what gets written when the user flies an aircraft nothing is
    /// configured for. Every lamp appears, none of them drive anything, and the
    /// user fills them in over time. Starting from a complete list of their
    /// hardware beats starting from an empty file, because the question the
    /// editor asks is "what should this lamp do", not "which lamps exist".
    ///
    /// The exceptions are the lamps that are dark in a way the user cannot
    /// trace when left unassigned: gates and screens, see [`Binding::fresh`].
    pub fn stub(name: &str, aircraft: &str, module: &str, devices: &DeviceInventory) -> Self {
        let bindings = devices
            .devices
            .iter()
            .flat_map(|d| d.leds().map(|(_, led)| Binding::fresh(&d.key, led)))
            .collect();

        Profile {
            schema_version: default_schema(),
            name: name.to_string(),
            author: String::new(),
            profile_version: "0.1.0".to_string(),
            aircraft: vec![aircraft.to_string()],
            module: module.to_string(),
            bindings,
            // A stub lists hardware, and a display field is not hardware: it is
            // a decision about what to show. There is no useful blank row for
            // one, so the editor offers to add them instead.
            readouts: Vec::new(),
            disabled_devices: Vec::new(),
        }
    }

    /// Write the profile, replacing any existing file in one step.
    ///
    /// Written to a temporary file and renamed rather than written in place,
    /// because the running daemon watches this directory and a plain write is
    /// visible while it is still half finished. A reader would see truncated
    /// JSON, and the profile would appear to vanish for as long as it took to
    /// finish writing.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Json(e, path.display().to_string()))?;
        let temp = path.with_extension("json.saving");
        std::fs::write(&temp, text)?;
        // Rename replaces an existing file on Windows as well as on Unix.
        std::fs::rename(&temp, path)?;
        Ok(())
    }

    /// The binding a mirroring lamp points at, if any.
    fn mirrored<'a>(&'a self, b: &'a Binding) -> Option<&'a Binding> {
        let target = b.same_as.as_deref()?;
        self.bindings
            .iter()
            .find(|o| o.device == b.device && o.led == target)
    }

    /// Every signal a binding depends on, following a mirror to its target.
    ///
    /// The engine indexes a binding under each address it reads so it can be
    /// re-evaluated when one moves. A mirroring lamp reads nothing itself, so
    /// without this it would be written once by the sweep and then never
    /// follow the lamp it is supposed to be mirroring.
    pub fn sources_of<'a>(&'a self, b: &'a Binding) -> Vec<&'a str> {
        match self.mirrored(b) {
            Some(target) => target.sources().collect(),
            None => b.sources().collect(),
        }
    }

    /// Resolve one binding, following a mirror to the lamp it copies.
    ///
    /// The mirrored value is taken as-is and then clamped to what *this* lamp
    /// accepts, and this lamp's own `off` applies when that value is zero. So a
    /// lamp can follow another one and still carry its own daylight floor.
    pub fn resolve_binding<F>(&self, b: &Binding, led: &Led, read: F) -> Option<u8>
    where
        F: FnMut(&str) -> Option<u32>,
    {
        self.resolve_binding_with_moves(b, led, read, |_| None)
    }

    /// [`Profile::resolve_binding`], told when each signal last changed value.
    /// See [`Binding::resolve_with_moves`].
    pub fn resolve_binding_with_moves<F, M>(
        &self,
        b: &Binding,
        led: &Led,
        read: F,
        moved: M,
    ) -> Option<u8>
    where
        F: FnMut(&str) -> Option<u32>,
        M: FnMut(&str) -> Option<u64>,
    {
        let Some(target) = self.mirrored(b) else {
            return b.resolve_with_moves(led, read, moved);
        };
        // Resolved against the target's own lamp range, then brought into this
        // one. `validate` rejects chains, so this never recurses further.
        let value = target.resolve_with_moves(led, read, moved)?;
        Some(if value == 0 {
            b.off
        } else {
            value.min(led.max_value())
        })
    }

    /// Check every binding resolves against the catalogue and the hardware.
    ///
    /// Worth doing on load rather than at evaluation time: a profile shared by
    /// someone with different hardware, or built against a newer DCS-BIOS,
    /// should fail loudly once instead of silently never lighting a lamp.
    ///
    /// Stops at the first fault, because the daemon's answer to any of them is
    /// the same: skip the profile. The editor wants the whole list instead, so
    /// the work lives in [`problems`](Self::problems) and this picks the first.
    pub fn validate(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
    ) -> Result<()> {
        match self.problems(module, devices, displays).into_iter().next() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }

    /// Every reason this profile would be rejected, rather than just the first.
    ///
    /// The editor checks after each edit and shows the list, so a fault is
    /// found where it was made rather than on the ramp with the panels dark.
    /// One fault must not hide another: a user who fixes the only problem shown
    /// and gets a second one has been told the same bad news twice.
    ///
    /// Each binding and each field is checked independently and a fault in one
    /// does not stop the rest, so the count reported is the real count.
    pub fn problems(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
    ) -> Vec<Error> {
        let mut out = Vec::new();
        for b in &self.bindings {
            if b.same_as.is_some()
                && !(b.conditions.is_empty() && b.any_of.is_empty() && !b.always)
            {
                out.push(Error::MirrorWithConditions(b.led.clone()));
            }
            if let Some(target) = &b.same_as {
                match self
                    .bindings
                    .iter()
                    .find(|o| o.device == b.device && &o.led == target)
                {
                    None => out.push(Error::UnknownMirror(
                        b.led.clone(),
                        target.clone(),
                        b.device.clone(),
                    )),
                    Some(other) => {
                        if other.same_as.is_some() {
                            out.push(Error::MirrorChain(b.led.clone(), target.clone()));
                        }
                        // Only lamps that dim, on both ends. An indicator takes
                        // 0 or 1, so it has no level to follow and none to
                        // offer: mirroring one either way would be a setting
                        // that cannot mean what it says.
                        if let Some(device) = devices.device(&b.device) {
                            let dims = |name: &str| {
                                device
                                    .led(name)
                                    .map(|(_, led)| led.is_dimmable())
                                    .unwrap_or(false)
                            };
                            if !dims(&b.led) || !dims(target) {
                                out.push(Error::MirrorNotDimmable(b.led.clone(), target.clone()));
                            }
                        }
                    }
                }
            }
            if b.always && !(b.conditions.is_empty() && b.any_of.is_empty()) {
                out.push(Error::AlwaysWithConditions(b.led.clone()));
            }
            if !b.conditions.is_empty() && !b.any_of.is_empty() {
                out.push(Error::ConditionsWithAnyOf(b.led.clone()));
            }
            if b.any_of.iter().any(|branch| branch.conditions.is_empty()) {
                out.push(Error::EmptyBranch(b.led.clone()));
            }
            if b.pick != Pick::Brightest && b.any_of.is_empty() {
                out.push(Error::PickWithoutAlternatives(b.led.clone()));
            }
            // The lamp must exist even on a placeholder row: it names real
            // hardware. Only the conditions are allowed to be undecided.
            //
            // A condition with no signal chosen is reported once however many
            // there are, because it is one thing to go and finish.
            let mut unfinished = false;
            for source in b.sources() {
                if source.is_empty() {
                    if !unfinished {
                        unfinished = true;
                        out.push(Error::UnfinishedCondition(b.led.clone()));
                    }
                } else if module.signal(source).is_none() {
                    out.push(Error::UnknownSignal(source.to_string()));
                }
            }
            let Some(device) = devices.device(&b.device) else {
                out.push(Error::UnknownLed(b.led.clone(), b.device.clone()));
                continue;
            };
            let Some((_, led)) = device.led(&b.led) else {
                out.push(Error::UnknownLed(b.led.clone(), b.device.clone()));
                continue;
            };
            // A profile asking for a brightness an indicator cannot produce is
            // a real authoring error, not something to silently clamp away.
            if let Some(on) = b.on {
                if on > led.max_value() {
                    out.push(Error::OutOfRange(b.led.clone(), on, led.max_value()));
                }
            }
        }
        self.readout_problems(module, devices, displays, &mut out);
        out
    }

    /// Bindings and readouts that will never run, because their device is
    /// disabled. Not an error: turning a panel off should not mean deleting
    /// the work of configuring it. But silence would be a trap, so the caller
    /// is given something to say.
    pub fn inert(&self) -> Vec<String> {
        let mut out = Vec::new();
        for device in &self.disabled_devices {
            let lamps = self.bindings.iter().filter(|b| &b.device == device).count();
            let fields = self.readouts.iter().filter(|r| &r.device == device).count();
            if lamps + fields > 0 {
                out.push(format!(
                    "{device} is disabled in this profile, so {lamps} lamp                      binding(s) and {fields} display field(s) on it do nothing"
                ));
            }
        }
        out
    }

    /// Gates that go to 0 with the cockpit dark, and so hide the lamps beneath
    /// them in daylight. Not an error: the profile loads and does what it says.
    /// But it is the fault that left the flap lamps invisible once and blanked
    /// every indicator on the A-10C later, and nothing on the panel says why.
    ///
    /// "Dark" is every signal reading 0, which is every lighting knob down.
    /// The fix is the binding's `off`, which applies whenever it resolves to 0.
    pub fn cautions(&self, devices: &DeviceInventory) -> Vec<String> {
        let mut out = Vec::new();
        for b in &self.bindings {
            if b.is_placeholder() || self.disabled_devices.contains(&b.device) {
                continue;
            }
            let Some((_, led)) = devices.device(&b.device).and_then(|d| d.led(&b.led)) else {
                continue;
            };
            if led.governs.is_empty() {
                continue;
            }
            if self.resolve_binding(b, led, |_| Some(0)) == Some(0) {
                let name = if led.label.is_empty() { &led.name } else { &led.label };
                out.push(format!(
                    "{name} goes to 0 with the cockpit lighting off, which in daylight hides the {} lamps it governs. Set its value at zero, usually {}, to keep them readable.",
                    led.governs.len(),
                    led.max_value()
                ));
            }
        }
        out
    }

    /// Check the display fields: that they name real glass, sit inside it, do
    /// not fight over cells, and read a source that can actually fill them.
    ///
    /// Appends rather than returning, for the reason given on
    /// [`problems`](Self::problems): one fault must not hide another.
    fn readout_problems(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
        out: &mut Vec<Error>,
    ) {
        for name in &self.disabled_devices {
            if devices.device(name).is_none() {
                out.push(Error::DisablesUnknownDevice(name.clone()));
            }
        }
        for (i, r) in self.readouts.iter().enumerate() {
            let Some(device) = devices.device(&r.device) else {
                out.push(Error::NoDisplayOnDevice(r.device.clone(), r.display.clone()));
                continue;
            };
            if device.part_with_display(&r.display).is_none() {
                out.push(Error::NoDisplayOnDevice(r.device.clone(), r.display.clone()));
                continue;
            }
            let Some(display) = displays.get(&r.display) else {
                out.push(Error::UnknownDisplay(r.display.clone()));
                continue;
            };
            if r.cells.last >= display.cells.len() {
                out.push(Error::CellsOutOfRange(
                    r.display.clone(),
                    display.cells.len(),
                    r.cells.to_string(),
                ));
            }

            // A seat is only meaningful where DCS-BIOS reports one, which is 5
            // of the 50 catalogued modules. Saying so beats accepting the field
            // and never painting it.
            if let Some(seat) = r.seat {
                match module.signal(SEAT_SIGNAL).and_then(|s| s.primary()) {
                    None => out.push(Error::SeatNotReported(
                        seat,
                        module.module.clone(),
                        SEAT_SIGNAL,
                    )),
                    Some(reported) => {
                        let highest = reported.max_value.unwrap_or(0);
                        if seat > highest {
                            out.push(Error::NoSuchSeat(seat, SEAT_SIGNAL, highest));
                        }
                    }
                }
            }

            // One field, one source. Nothing arbitrates between two readouts
            // claiming a cell, because nothing needs to: the cockpit has
            // already decided what belongs there, or the user has.
            //
            // Two seats are the exception, and the only one. They cannot both
            // be occupied, so they cannot both be painting, and sharing a
            // window between them is the whole reason the field exists.
            //
            // Each pair is looked at once, from the earlier field, so an
            // overlap is one problem rather than the same one said twice.
            for other in self.readouts.iter().skip(i + 1) {
                let both_live = match (r.seat, other.seat) {
                    (Some(a), Some(b)) => a == b,
                    _ => true,
                };
                if both_live
                    && other.device == r.device
                    && other.display == r.display
                    && other.cells.overlaps(&r.cells)
                {
                    out.push(Error::CellsOverlap(
                        r.cells.to_string(),
                        other.cells.to_string(),
                        r.display.clone(),
                    ));
                }
            }

            // A field with nothing chosen yet is unfinished work rather than a
            // mistake, but it still stops the profile loading, so it is said
            // plainly and in those terms.
            if r.source.is_empty() {
                out.push(Error::UnfinishedField(r.display.clone(), r.cells.to_string()));
                continue;
            }
            let Some(output) = module.signal(&r.source).and_then(|s| s.primary()) else {
                out.push(Error::UnknownSignal(r.source.clone()));
                continue;
            };
            // A needle reports a position, not a quantity, and nothing in the
            // catalogue says what its face is marked with. So the range is the
            // user's to give, and asking for it beats printing 0 to 65535 and
            // letting them wonder what broke.
            if output.r#type == "string" {
                if r.reads.is_some() {
                    out.push(Error::RangeOnText(r.source.clone()));
                }
            } else if r.reads.is_none() {
                out.push(Error::RangeMissing(r.source.clone()));
            }

            if let Some(format) = &r.format {
                if !display.draws_inverse() {
                    out.push(Error::FormatNotDrawn(r.display.clone(), r.cells.to_string()));
                }
                match module.signal(format).and_then(|s| s.primary()) {
                    None => out.push(Error::UnknownSignal(format.clone())),
                    Some(o) if o.r#type != "string" => out.push(Error::FormatNotText(format.clone())),
                    Some(_) => {}
                }
            }

            if let Some(colours) = &r.colours {
                match module.signal(&colours.source).and_then(|s| s.primary()) {
                    None => out.push(Error::UnknownSignal(colours.source.clone())),
                    Some(o) if o.r#type != "string" => {
                        out.push(Error::FormatNotText(colours.source.clone()))
                    }
                    Some(_) => {}
                }
                for code in colours.codes.keys() {
                    if code.chars().count() != 1 {
                        out.push(Error::ColourCodeNotOneChar(code.clone()));
                    }
                }
            }

            for (from, to) in &r.replace {
                if from.chars().count() != 1 || to.chars().count() != 1 {
                    out.push(Error::ReplaceNotOneChar(from.clone(), to.clone()));
                }
            }
            self.text_problems(r, display, out);
        }
    }

    /// What only a text grid can take, and what a text grid needs.
    ///
    /// Its font is fixed by the aircraft, not chosen here: an aircraft with a
    /// CDU of its own has glyphs drawn to match what DCS-BIOS sends for it. So
    /// every aircraft this profile covers must have one, and every character
    /// the field is told to substitute in must be one that font can draw.
    fn text_problems(&self, r: &Readout, display: &Display, out: &mut Vec<Error>) {
        let Some(text) = &display.text else {
            if r.colour.is_some() || r.colours.is_some() || r.small {
                out.push(Error::StyleNotDrawn(r.display.clone(), r.cells.to_string()));
            }
            return;
        };
        let mut fonts = Vec::new();
        for aircraft in &self.aircraft {
            match text.font_for(aircraft) {
                Some(file) => fonts.push(file),
                None => out.push(Error::NoNativeFont(r.display.clone(), aircraft.clone())),
            }
        }
        fonts.sort_unstable();
        fonts.dedup();
        for file in fonts {
            let Some(chars) = text.charsets.get(file) else {
                continue;
            };
            let set = if r.small { &chars.small } else { &chars.large };
            for to in r.replace.values() {
                if let Some(c) = to.chars().next().filter(|c| !set.contains(c)) {
                    out.push(Error::NotInFont(c, file.to_string(), r.cells.to_string()));
                }
            }
        }
    }
}

/// Order bindings by device, then part, then hardware index, and say whether
/// that changed anything.
///
/// Devices sort by the name the editor shows, so the file reads in the same
/// order as the window. Within a device, parts keep the order the inventory
/// declares them in, which groups a physical panel together: the UFC and the
/// HUD beneath it are one device with two parts, and interleaving their lamps
/// by index alone would split each panel in half. Within a part, lamps keep
/// index order, because that is how they sit on the panel; sorting those by
/// name would scatter a gear indicator away from the rest of the gear.
///
/// A device the inventory does not know sorts last rather than being dropped.
/// That is a panel the user has unplugged, not a mistake to tidy away.
fn sort_bindings(bindings: &mut [Binding], devices: &DeviceInventory) -> bool {
    let key = |b: &Binding| {
        let device = devices.device(&b.device);
        let label = device
            .map(|d| d.display_name.to_lowercase())
            .unwrap_or_else(|| format!("~{}", b.device.to_lowercase()));
        let (part, index) = device
            .and_then(|d| {
                d.parts.iter().enumerate().find_map(|(n, p)| {
                    p.leds.iter().find(|l| l.name == b.led).map(|l| (n, l.index))
                })
            })
            .unwrap_or((usize::MAX, u8::MAX));
        (label, part, index, b.led.to_lowercase())
    };
    let was: Vec<(String, String)> = bindings
        .iter()
        .map(|b| (b.device.clone(), b.led.clone()))
        .collect();
    bindings.sort_by_key(key);
    was != bindings
        .iter()
        .map(|b| (b.device.clone(), b.led.clone()))
        .collect::<Vec<_>>()
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

/// The file name, without `.json`, for a profile named after `name`.
///
/// Lowercase, with each run of anything else collapsed to one dash and none at
/// either end: `A-10C_2` becomes `a-10c-2`, `F/A-18C Hornet` `f-a-18c-hornet`.
/// The one rule for every generated profile, whether the daemon names it after
/// the aircraft it saw, the editor after the module, or a copy after its name.
/// Two rules had drifted apart once, so there is one.
///
/// Empty for a name with nothing alphanumeric in it, which callers must refuse
/// rather than write `.json`.
pub fn file_stem(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// What DCS-BIOS reports as the aircraft when the player has none of their
/// own: it takes the name from `LoGetSelfData()` and falls back to this when
/// that returns nothing, logging "unloaded aircraft". Not an unsupported
/// module, which is reported under its real name, and not the main menu,
/// where nothing is exported at all. DCS-BIOS files it under its FC3 module
/// only because that is where its extras go.
pub const NO_AIRCRAFT: &str = "NONE";

/// The name to give a profile made for `aircraft`, for a person to read.
///
/// The aircraft name itself, except [`NO_AIRCRAFT`], which reads as a mistake
/// in a profile list. File names follow from this through [`file_stem`].
pub fn profile_name_for(aircraft: &str) -> &str {
    if aircraft == NO_AIRCRAFT {
        "No aircraft"
    } else {
        aircraft
    }
}

impl Profiles {
    pub fn new(defaults: impl Into<PathBuf>, active: impl Into<PathBuf>) -> Self {
        Profiles {
            defaults: defaults.into(),
            active: active.into(),
        }
    }

    /// Every aircraft an active profile already claims, with the name of the
    /// profile that claims it.
    ///
    /// Two profiles for one aircraft is the conflict worth preventing, not two
    /// on one module: the F/A-18C and F/A-18E profiles share a module and
    /// claim different aircraft, which is fine. A profile that will not parse
    /// claims nothing, since it cannot be loaded to fly either.
    pub fn claimed_aircraft(&self) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(entries) = std::fs::read_dir(&self.active) else {
            return out;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            if let Ok(p) = Profile::load(&path) {
                for a in &p.aircraft {
                    out.entry(a.clone()).or_insert_with(|| p.name.clone());
                }
            }
        }
        out
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

    /// Add rows for anything new to every active profile, changing nothing the
    /// user has already decided. Returns a line per profile it touched.
    ///
    /// [`seed`] only helps a profile the user does not have yet. This is the
    /// other half: a user who has flown the Hornet since before we supported
    /// the UFC has a Hornet profile with no UFC in it, and nothing would ever
    /// put one there. Their own file is the one that runs, so a row missing
    /// from it is a lamp they cannot configure at all.
    ///
    /// Two sources, in order. A shipped default may have gained bindings for
    /// hardware it did not cover before, and those are worth having. Anything
    /// still unaccounted for becomes a blank row, because the question the
    /// editor asks is "what should this lamp do", and it cannot ask about a
    /// lamp that is not listed.
    ///
    /// Existing rows are never touched, in either direction: not rewritten, not
    /// removed, not reordered relative to what they say. A profile whose device
    /// has been unplugged keeps its rows, because unplugging a panel for an
    /// evening is not a decision to discard its configuration.
    pub fn merge_new(&self, devices: &DeviceInventory) -> Result<Vec<String>> {
        if !self.active.is_dir() {
            return Ok(Vec::new());
        }
        let mut notes = Vec::new();
        let mut files: Vec<PathBuf> = std::fs::read_dir(&self.active)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        files.sort();

        for path in files {
            let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            // A profile that will not parse is left alone. Rewriting one we do
            // not understand is how an editing mistake becomes data loss.
            let Ok(mut profile) = Profile::load(&path) else {
                continue;
            };

            let mut have: std::collections::HashSet<(String, String)> = profile
                .bindings
                .iter()
                .map(|b| (b.device.clone(), b.led.clone()))
                .collect();
            let before = profile.bindings.len();
            let mut from_default = 0usize;

            if let Ok(shipped) = Profile::load(&self.defaults.join(&name)) {
                for b in shipped.bindings {
                    if have.insert((b.device.clone(), b.led.clone())) {
                        profile.bindings.push(b);
                        from_default += 1;
                    }
                }
                // A readout is added only where it cannot collide. The user may
                // have claimed those cells for something of their own, and a
                // shipped suggestion does not outrank that.
                for r in shipped.readouts {
                    let clash = profile.readouts.iter().any(|o| {
                        o.device == r.device && o.display == r.display && o.cells.overlaps(&r.cells)
                    });
                    if !clash {
                        profile.readouts.push(r);
                    }
                }
            }

            for device in &devices.devices {
                for (_, led) in device.leds() {
                    if have.insert((device.key.clone(), led.name.clone())) {
                        profile.bindings.push(Binding::fresh(&device.key, led));
                    }
                }
            }

            let added = profile.bindings.len() - before;
            let order_changed = sort_bindings(&mut profile.bindings, devices);
            if added == 0 && !order_changed {
                continue;
            }
            profile.save(&path)?;
            if added > 0 {
                notes.push(format!(
                    "{name}: added {added} row(s), {from_default} from the shipped default"
                ));
            } else {
                notes.push(format!("{name}: reordered"));
            }
        }
        Ok(notes)
    }

    /// Delete a profile the user made.
    ///
    /// Refused for one that shipped: it would be seeded straight back on the
    /// next start, so deleting it only looks like it worked. Reset is the way
    /// back for those. What this is for is the profile left over from splitting
    /// one aircraft list in two, which nothing else can remove.
    pub fn delete(&self, file: &str) -> Result<()> {
        // A bare file name, so nothing outside the active folder is reachable.
        if Path::new(file).file_name().and_then(|n| n.to_str()) != Some(file) {
            return Err(Error::NotAProfileFile(file.to_string()));
        }
        if self.has_default(file) {
            return Err(Error::ShippedProfile(file.to_string()));
        }
        std::fs::remove_file(self.active.join(file))?;
        Ok(())
    }

    /// Overwrite one active profile with its shipped default.
    ///
    /// Destroys user work, so it is never reached except by someone clicking
    /// reset.
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
    fn no_aircraft_gets_a_name_a_person_can_read() {
        assert_eq!(profile_name_for(NO_AIRCRAFT), "No aircraft");
        assert_eq!(file_stem(profile_name_for(NO_AIRCRAFT)), "no-aircraft");
        assert_eq!(profile_name_for("A-10C_2"), "A-10C_2");
    }

    #[test]
    fn file_stems_match_the_names_already_shipped() {
        // Aircraft names, which is how the daemon names what it writes.
        assert_eq!(file_stem("A-10C_2"), "a-10c-2");
        assert_eq!(file_stem("F-14BU"), "f-14bu");
        assert_eq!(file_stem("NONE"), "none");
        // Module keys, which is how the editor names a new profile.
        assert_eq!(file_stem("FA-18C_hornet"), "fa-18c-hornet");
        assert_eq!(file_stem("Christen Eagle II"), "christen-eagle-ii");
        // Profile names, which is how a copy is named.
        assert_eq!(file_stem("FA-18E"), "fa-18e");
        assert_eq!(file_stem("F/A-18C Hornet copy"), "f-a-18c-hornet-copy");
        // Nothing usable, so nothing: callers refuse rather than write ".json".
        assert_eq!(file_stem("   "), "");
    }

    fn lamp(kind: LedKind, max: u8) -> Led {
        Led {
            index: 4,
            name: "TEST".into(),
            label: String::new(),
            kind,
            max: Some(max),
            on_value: None,
            verified: true,
            note: String::new(),
            governs: Vec::new(),
            lights_display: false,
            backlight: false,
        }
    }

    fn always_on(on: Option<u8>) -> Binding {
        Binding {
            device: "D".into(),
            led: "TEST".into(),
            conditions: Vec::new(),
            always: true,
            any_of: Vec::new(),
            pick: Pick::default(),
            same_as: None,
            on,
            off: 0,
            note: String::new(),
        }
    }

    /// Always-on reads nothing, so it must resolve without any signal having
    /// arrived. If it returned `None` the sweep would drive the lamp to zero,
    /// which is the exact opposite of what the user asked for.
    #[test]
    fn always_on_resolves_with_no_signals_seen() {
        let led = lamp(LedKind::Indicator, 1);
        let value = always_on(None).resolve(&led, |_| panic!("must not read any signal"));
        assert_eq!(value, Some(1));
    }

    /// The same mechanism sets a fixed brightness, which is how a backlight is
    /// held at one level instead of following the cockpit dimmer.
    #[test]
    fn always_on_carries_a_chosen_brightness() {
        let led = lamp(LedKind::Dimmer, 255);
        assert_eq!(always_on(Some(128)).resolve(&led, |_| None), Some(128));
        // Still clamped to what the lamp accepts: an indicator acks 255 and
        // lights nothing, which is indistinguishable from a dead lamp.
        let indicator = lamp(LedKind::Indicator, 1);
        assert_eq!(always_on(Some(255)).resolve(&indicator, |_| None), Some(1));
    }

    /// An empty condition list means "not decided yet" and drives the lamp off.
    /// Always-on is a decision, so it must not be swept away as unassigned.
    #[test]
    fn always_on_is_not_a_placeholder() {
        assert!(!always_on(None).is_placeholder());
        let mut undecided = always_on(None);
        undecided.always = false;
        assert!(undecided.is_placeholder());
    }

    fn cond(source: &str, on_when: OnWhen) -> Condition {
        Condition {
            source: source.into(),
            on_when,
        }
    }

    /// The case `any_of` exists for: a multicrew lamp that follows whichever
    /// station is occupied. Each branch gates a brightness behind a station
    /// test, so the branch for the empty seat resolves to zero and the live one
    /// supplies the value.
    fn multicrew() -> Binding {
        Binding {
            device: "D".into(),
            led: "TEST".into(),
            conditions: Vec::new(),
            always: false,
            pick: Pick::default(),
            same_as: None,
            any_of: vec![
                Branch {
                    conditions: vec![
                        cond("STATION", OnWhen::Equals(1)),
                        cond("CPG_BRIGHT", OnWhen::Scale([0, 65535])),
                    ],
                },
                Branch {
                    conditions: vec![
                        cond("STATION", OnWhen::Equals(0)),
                        cond("PLT_BRIGHT", OnWhen::Scale([0, 65535])),
                    ],
                },
            ],
            on: None,
            off: 0,
            note: String::new(),
        }
    }

    #[test]
    fn any_of_takes_the_branch_that_is_live() {
        let led = lamp(LedKind::Dimmer, 255);
        let binding = multicrew();

        // In the CPG seat: the CPG branch supplies its brightness and the PLT
        // branch is gated to zero, so the brighter of the two is the CPG value.
        let value = binding.resolve(&led, |s| match s {
            "STATION" => Some(1),
            "CPG_BRIGHT" => Some(65535),
            "PLT_BRIGHT" => Some(0),
            _ => None,
        });
        assert_eq!(value, Some(255));

        // In the PLT seat, with the CPG dimmer left high. The station test, not
        // the brightness, is what decides, which is the whole point.
        let value = binding.resolve(&led, |s| match s {
            "STATION" => Some(0),
            "CPG_BRIGHT" => Some(65535),
            "PLT_BRIGHT" => Some(32768),
            _ => None,
        });
        assert_eq!(value, Some(127));
    }

    #[test]
    fn a_closed_gate_is_read_before_the_scale_behind_it() {
        // Written scale first. The empty seat's dimmer has not arrived, yet the
        // lamp resolves, because its seat test closes that branch first.
        let mut binding = multicrew();
        for branch in &mut binding.any_of {
            branch.conditions.reverse();
        }
        let led = lamp(LedKind::Dimmer, 255);
        let value = binding.resolve(&led, |s| match s {
            "STATION" => Some(0),
            "PLT_BRIGHT" => Some(65535),
            _ => None,
        });
        assert_eq!(value, Some(255));
    }

    #[test]
    fn any_of_is_off_when_no_branch_holds() {
        let led = lamp(LedKind::Dimmer, 255);
        let value = multicrew().resolve(&led, |s| match s {
            "STATION" => Some(7),
            _ => Some(65535),
        });
        assert_eq!(value, Some(0), "no station matched, so nothing lights");
    }

    /// Same conservatism as a single group: a signal nobody has seen leaves the
    /// lamp at its swept value rather than forcing it somewhere.
    #[test]
    fn any_of_is_unresolved_while_a_deciding_signal_is_unseen() {
        let led = lamp(LedKind::Dimmer, 255);
        let value = multicrew().resolve(&led, |s| match s {
            "CPG_BRIGHT" | "PLT_BRIGHT" => Some(65535),
            // The station is what decides, and nothing has reported it yet.
            _ => None,
        });
        assert_eq!(value, None);
    }

    /// A branch already gated off stops before reading the rest of itself.
    ///
    /// Worth pinning, because it is what keeps a multicrew lamp working in the
    /// occupied seat when the other seat's dimmer has never been touched and so
    /// has never been exported. Requiring every branch to be fully readable
    /// would leave the lamp unresolved and therefore dark.
    #[test]
    fn a_branch_that_cannot_hold_does_not_need_the_rest_of_its_signals() {
        let led = lamp(LedKind::Dimmer, 255);
        let value = multicrew().resolve(&led, |s| match s {
            "STATION" => Some(1),
            "CPG_BRIGHT" => Some(65535),
            // The PLT branch is already false at the station test.
            _ => None,
        });
        assert_eq!(value, Some(255));
    }

    /// Two seats with a lighting knob each, and no signal saying which seat
    /// the player is in.
    fn two_knobs(pick: Pick) -> Binding {
        Binding {
            device: "D".into(),
            led: "TEST".into(),
            conditions: Vec::new(),
            always: false,
            pick,
            same_as: None,
            any_of: vec![
                Branch {
                    conditions: vec![cond("PLT_KNOB", OnWhen::Scale([0, 8]))],
                },
                Branch {
                    conditions: vec![cond("RIO_KNOB", OnWhen::Scale([0, 8]))],
                },
            ],
            on: None,
            off: 0,
            note: String::new(),
        }
    }

    fn knobs(s: &str) -> Option<u32> {
        match s {
            "PLT_KNOB" => Some(2),
            "RIO_KNOB" => Some(8),
            _ => None,
        }
    }

    #[test]
    fn latest_follows_the_knob_last_turned_even_when_it_is_the_dimmer_one() {
        let led = lamp(LedKind::Dimmer, 255);
        // The pilot just turned theirs down to 2; the RIO's sits at 8 from
        // earlier. Brightest would ignore the pilot entirely.
        let moved = |s: &str| match s {
            "PLT_KNOB" => Some(5),
            "RIO_KNOB" => Some(3),
            _ => None,
        };
        let value = two_knobs(Pick::Latest).resolve_with_moves(&led, knobs, moved);
        assert_eq!(value, Some(63));
        let value = two_knobs(Pick::Brightest).resolve_with_moves(&led, knobs, moved);
        assert_eq!(value, Some(255), "brightest does not ask when anything moved");
    }

    #[test]
    fn latest_is_the_brightest_until_a_knob_has_moved() {
        // Nothing turned since the mission loaded, so there is no one to
        // follow yet, and a lit panel is the safer guess than a dark one.
        let led = lamp(LedKind::Dimmer, 255);
        let value = two_knobs(Pick::Latest).resolve_with_moves(&led, knobs, |_| None);
        assert_eq!(value, Some(255));
    }

    #[test]
    fn latest_turned_to_zero_takes_the_value_at_zero() {
        // Turning your own knob fully off is a choice, not a reason to fall
        // back to the other seat's.
        let led = lamp(LedKind::Dimmer, 255);
        let mut binding = two_knobs(Pick::Latest);
        binding.off = 40;
        let read = |s: &str| match s {
            "PLT_KNOB" => Some(0),
            _ => Some(8),
        };
        let moved = |s: &str| (s == "PLT_KNOB").then_some(9);
        assert_eq!(binding.resolve_with_moves(&led, read, moved), Some(40));
    }

    #[test]
    fn pick_is_left_out_of_the_file_unless_it_is_latest() {
        let plain = serde_json::to_value(two_knobs(Pick::Brightest)).unwrap();
        assert!(plain.get("pick").is_none(), "every existing profile stays byte for byte");
        let latest = serde_json::to_value(two_knobs(Pick::Latest)).unwrap();
        assert_eq!(latest["pick"], "latest");
        let back: Binding = serde_json::from_value(latest).unwrap();
        assert_eq!(back.pick, Pick::Latest);
    }

    /// The engine indexes a binding by every address it reads, so a signal
    /// named only inside a branch still has to be reported.
    #[test]
    fn sources_reach_inside_every_branch() {
        let binding = multicrew();
        let mut found: Vec<&str> = binding.sources().collect();
        found.sort_unstable();
        found.dedup();
        assert_eq!(found, vec!["CPG_BRIGHT", "PLT_BRIGHT", "STATION"]);
    }

    fn mirror_profile(same_as: Option<&str>) -> Profile {
        Profile {
            schema_version: 1,
            name: "t".into(),
            author: String::new(),
            profile_version: String::new(),
            aircraft: vec!["A".into()],
            module: "M".into(),
            bindings: vec![
                Binding {
                    device: "D".into(),
                    led: "Backlight".into(),
                    conditions: vec![cond("DIM", OnWhen::Scale([0, 65535]))],
                    always: false,
                    any_of: Vec::new(),
                    pick: Pick::default(),
                    same_as: None,
                    on: None,
                    off: 0,
                    note: String::new(),
                },
                Binding {
                    device: "D".into(),
                    led: "FLAG".into(),
                    conditions: Vec::new(),
                    always: false,
                    any_of: Vec::new(),
                    pick: Pick::default(),
                    same_as: same_as.map(str::to_string),
                    on: None,
                    off: 255,
                    note: String::new(),
                },
            ],
            readouts: Vec::new(),
            disabled_devices: Vec::new(),
        }
    }

    /// A mirror follows the lamp it points at, which is the whole point: change
    /// the backlight and the governors above it move with it.
    #[test]
    fn a_mirror_follows_its_target() {
        let led = lamp(LedKind::Dimmer, 255);
        let profile = mirror_profile(Some("Backlight"));
        let flag = &profile.bindings[1];

        let value = profile.resolve_binding(flag, &led, |_| Some(65535));
        assert_eq!(value, Some(255));
        let value = profile.resolve_binding(flag, &led, |_| Some(32768));
        assert_eq!(value, Some(127));
    }

    /// The mirroring lamp keeps its own `off`, so it can follow the backlight at
    /// night and still hold a daylight floor when the console knob is at zero.
    /// This is exactly the FLAG case that left the flap lamps invisible once.
    #[test]
    fn a_mirror_keeps_its_own_daylight_floor() {
        let led = lamp(LedKind::Dimmer, 255);
        let profile = mirror_profile(Some("Backlight"));
        let value = profile.resolve_binding(&profile.bindings[1], &led, |_| Some(0));
        assert_eq!(value, Some(255), "console off means daylight, so the flags stay readable");
    }

    /// A mirror reads no signal of its own, so the engine has to index it under
    /// the target's addresses or it would be written once and never follow.
    #[test]
    fn a_mirror_reports_the_signals_of_its_target() {
        let profile = mirror_profile(Some("Backlight"));
        assert_eq!(profile.sources_of(&profile.bindings[1]), vec!["DIM"]);
        // And without the mirror it reports nothing, because it reads nothing.
        let plain = mirror_profile(None);
        assert!(profile.sources_of(&plain.bindings[1]).is_empty());
    }

    /// The editor rewrites whole profiles, and these files are shipped and
    /// diffed by hand. A round trip that added a line to every lamp would make
    /// every future change unreviewable.
    #[test]
    fn saving_a_shipped_profile_does_not_pad_it() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/defaults");
        let mut checked = 0;
        for entry in std::fs::read_dir(&dir).expect("data/defaults should exist") {
            let path = entry.expect("readable entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let profile = Profile::load(&path).expect("shipped profile should parse");
            let text = serde_json::to_string_pretty(&profile).expect("should serialise");
            assert!(
                !text.contains("\"on\": null"),
                "{} gained an explicit null on save",
                path.display()
            );
            checked += 1;
        }
        assert!(checked > 0, "no shipped profiles were checked, so this proves nothing");
    }

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
