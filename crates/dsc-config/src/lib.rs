//! Catalogue, device inventory and profile types.
//!
//! The catalogue is generated from DCS-BIOS by [`catalogue_build`]; the
//! device inventory is `data/devices.json`. Profiles are authored by the user,
//! keyed by LED rather than by signal  see docs/CONFIG.md.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub mod catalogue_build;
pub mod daemon;
pub mod display;
pub mod log;
pub mod mcdu_font;
pub mod nightly_only;
pub mod paths;

/// The release, exactly as `VERSION.md` states it. The manifests hold a semver
/// form of it (see `tools/version.py`); this is the one to show people.
pub fn version() -> &'static str {
    include_str!("../../../VERSION.md").trim()
}

/// The commit this was built from. The release pipeline sets `DSC_COMMIT`, so
/// an installed copy names the exact source it was compiled from; any other
/// build has none.
pub fn commit() -> Option<&'static str> {
    option_env!("DSC_COMMIT").filter(|c| !c.is_empty())
}

/// The version with its commit, for `--version` and the daemon log, so a
/// report from a user leads back to the source that built it.
pub fn build_label() -> &'static str {
    static LABEL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    LABEL.get_or_init(|| format!("{} ({})", version(), commit().unwrap_or("local build")))
}

pub use display::{
    divider_rule, divider_text, min_divider_cells, text_cells, AliasDraw, Align, Cell, CellRange,
    Colour, ColourSource, Display, DisplayCatalogue, Glyph, Grid, Readout, Reading, Region, Round,
    RuleCell, Screen, Span, TextCell, TextGrid, Transport, ValueBand, SEAT_SIGNAL,
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json in {1}: {0}")]
    Json(serde_json::Error, String),
    #[error("no catalogue entry for aircraft {0:?}")]
    UnknownAircraft(String),
    #[error("LED {0:?} has a condition with no signal chosen yet; pick one or delete the condition")]
    UnfinishedCondition(String),
    #[error("the field on {1} of display {0:?} has a piece with nothing in it; give it characters or a signal, or take the piece out")]
    UnfinishedField(String, String),
    #[error("profile references unknown LED {0:?} on device {1:?}")]
    UnknownLed(String, String),
    #[error("LED {0:?} was given on={1}, above its maximum of {2}")]
    OutOfRange(String, u8, u8),
    #[error("no shipped default named {0:?} to reset from")]
    NoDefault(String),
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
    #[error("LED {0:?} mirrors {1:?}, which has no row on device {2:?}")]
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
    #[error("a piece of cells {1} on display {0:?} has both characters to draw and the signal {2:?} to read; it can have one")]
    SpanReadsAndWrites(String, String, String),
    #[error("a gap on cells {1} of display {0:?} also has something to draw; a gap is the blank space left over and draws nothing of its own")]
    GapHasContent(String, String),
    #[error("cells {1} of display {0:?} hold nothing but gaps, which would draw an empty run")]
    NothingButGaps(String, String),
    #[error("a colour code is one character; {0:?} is not")]
    ColourCodeNotOneChar(String),
    #[error("cells {1} of display {0:?} are given a colour or size, which only a text grid draws")]
    StyleNotDrawn(String, String),
    #[error("cells {1} of display {0:?} are given a divider, which only a text grid draws")]
    DividerNotDrawn(String, String),
    #[error("the divider on {1} of display {0:?} also names a signal {2:?}; a divider draws a fixed rule and reads nothing")]
    DividerReadsSignal(String, String, String),
    #[error("the label {4:?} on the divider on {1} of display {0:?} does not fit: it has {2} cells and needs {3}, a dash and a blank each side of the label")]
    DividerLabelTooWide(String, String, usize, usize, String),
    #[error("a rule on {1} of display {0:?} is written on a piece that draws its own content; only a gap can be a rule")]
    RuleNotOnGap(String, String),
    #[error("the label {2:?} on a rule on {1} of display {0:?} has no fixed width; an elastic rule is as wide as the rest of the line leaves, so a label on one would come and go as the readings beside it change width")]
    RuleLabelNeedsWidth(String, String, String),
    #[error("the label {4:?} on a rule on {1} of display {0:?} does not fit: the rule is {2} cells wide and needs {3}, a dash and a blank each side of the label")]
    RuleLabelTooWide(String, String, usize, usize, String),
    #[error("a label {2:?} on {1} of display {0:?} is written on a piece that is not a rule; only a rule sets a label into itself")]
    LabelNotOnRule(String, String, String),
    #[error("a fixed width of {2} on {1} of display {0:?} is wider than the {3} cells the field has")]
    SpanWiderThanField(String, String, usize, usize),

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
    #[error("{0:?} already reports characters, so a range would mean nothing")]
    RangeOnText(String),
    #[error("{0:?} already reports characters, so aliases for its values would mean nothing")]
    AliasesOnText(String),
    #[error("aliases {0} and {1} on {2:?} both claim the same reading; the lower one draws it")]
    AliasBandsOverlap(String, String, String),
    #[error("alias {0} on {1:?} is outside everything the face reads, {2} to {3}, so nothing would ever draw it")]
    AliasBandUnreachable(String, String, String, String),
    #[error("profile disables device {0:?}, which is not a device we know")]
    DisablesUnknownDevice(String),
    #[error("{0:?} is set to follow {1:?}, and {2:?} is not a device we know")]
    FollowsUnknownDevice(String, String, String),
    #[error("{0:?} is set to follow itself")]
    FollowsItself(String),
    #[error("{0:?} follows {1:?}, which follows another device in turn; point it at that one instead")]
    FollowChain(String, String),
    #[error("{0:?} cannot follow {1:?}: they are different hardware, so the lamps and screens would not line up")]
    FollowsDifferentHardware(String, String),
    #[error("a field is set to seat {0}, but module {1:?} does not report {2}, so nothing would ever be painted there")]
    SeatNotReported(u32, String, &'static str),
    #[error("seat {0} is not one this module has; {1} reports 0 to {2}")]
    NoSuchSeat(u32, &'static str, u32),
    #[error("display {0:?} cannot draw inverse characters, so the format signal on {1} would do nothing")]
    FormatNotDrawn(String, String),
    #[error("{0:?} is a number; a format signal has to be characters, one per cell")]
    FormatNotText(String),
    #[error("DCS-BIOS not found at {0}, so there is nothing to build a catalogue from")]
    NoBios(PathBuf),
    #[error("another DCS Signal Converter process is building the catalogue and has not finished; if none is running, delete {0}")]
    CatalogueBusy(PathBuf),
}

impl Error {
    /// Whether this is a caution rather than a reason to refuse a profile.
    ///
    /// These hang on DCS-BIOS saying a signal is text or a number. It is right
    /// nearly always and wrong sometimes, and when it is wrong only the user,
    /// looking at the panel, can tell. The worst a wrong one does is a setting
    /// that changes nothing.
    ///
    /// A character the font lacks is not one of these. The font is ours, it is
    /// what the panel is sent, and a character missing from it is a blank cell
    /// for certain.
    /// Both band faults are these. A band nothing can reach is dead
    /// configuration, and the likeliest way to write one is to band a
    /// converted face in the raw counts DCS-BIOS sends.
    ///
    /// Two bands claiming one reading is a caution rather than a refusal even
    /// though it is ambiguous, because it is not undefined: bands are held in
    /// order of where they start, so the lower one draws, every time. Unlike
    /// two fields claiming a cell, nothing here is unresolvable, and the cost
    /// of being strict is the wrong way round. A profile is refused whole, so
    /// refusing this one would take every lamp and every screen dark over one
    /// row drawing the first of two words the user wrote. A row the user owns
    /// is never rewritten by an update, so a profile that started loading
    /// could not be repaired for them either.
    pub fn is_advisory(&self) -> bool {
        matches!(
            self,
            Error::RangeOnText(_)
                | Error::AliasesOnText(_)
                | Error::FormatNotText(_)
                | Error::AliasBandUnreachable(..)
                | Error::AliasBandsOverlap(..)
        )
    }
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

impl Output {
    /// The largest number this output can report, as a display field reads it.
    ///
    /// DCS-BIOS leaves `max_value` off some outputs, and the value is a single
    /// 16 bit word whatever it says, so both fall back to the word's own top.
    pub fn number_max(&self) -> u16 {
        self.max_value
            .unwrap_or(u32::from(u16::MAX))
            .min(u32::from(u16::MAX)) as u16
    }
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
    /// The DCS-BIOS release this was built from, from `index.json`.
    bios_version: Option<String>,
    /// Where the running DCS-BIOS reports its release in the stream.
    version_signal: Option<catalogue_build::VersionSignal>,
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
        if let Some(index) = catalogue_build::read_index(dir) {
            cat.bios_version = index.bios_version;
            cat.version_signal = index.version_signal;
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

    /// The DCS-BIOS release this catalogue was built from. `None` for one
    /// built in memory, or read from a folder with no index.
    pub fn bios_version(&self) -> Option<&str> {
        self.bios_version.as_deref()
    }

    /// Where DCS-BIOS reports its own release in the export stream, as an
    /// address and a length, when the catalogue recorded it.
    pub fn version_signal(&self) -> Option<(u16, u16)> {
        self.version_signal.map(|v| (v.address, v.max_length))
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

    /// Whether `other` is this device under another name: the same lamps at
    /// the same indices and the same screens, whatever its part ids and USB id
    /// say.
    ///
    /// WinWing sells one panel as several products, one per seat or position,
    /// each with its own PID so that more than one can sit on a desk. The
    /// MCDU is Captain, Co-Pilot and Observer, and the MFD is L, C and R. A
    /// profile can point one at another rather than setting both up, and this
    /// is what decides it may. Worked out from the hardware rather than listed,
    /// so a variant added to the inventory is one without anything else said.
    pub fn same_hardware(&self, other: &DeviceSpec) -> bool {
        let shape = |d: &DeviceSpec| {
            d.parts
                .iter()
                .map(|p| {
                    let leds: Vec<_> = p.leds.iter().map(|l| (l.index, l.name.clone(), l.max)).collect();
                    (p.display.clone(), leds)
                })
                .collect::<Vec<_>>()
        };
        shape(self) == shape(other)
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
    /// The highest source value this condition tests or scales against.
    pub fn highest(&self) -> Option<u32> {
        match self {
            OnWhen::Equals(v) | OnWhen::Gte(v) | OnWhen::Lte(v) => Some(*v),
            OnWhen::In(vs) => vs.iter().max().copied(),
            OnWhen::Between([lo, hi]) | OnWhen::Scale([lo, hi]) => Some((*lo).max(*hi)),
        }
    }

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
    /// Mirror another lamp, by name, on this device unless
    /// [`same_as_device`](Self::same_as_device) names another.
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
    /// The device holding the lamp `same_as` names, when it is not this one.
    ///
    /// So one panel's backlight can follow another's: an MFD bezel set to
    /// match the throttle's is one knob to change instead of two. Left out, the
    /// lamp is on this device, which is every profile written before this
    /// existed. A device that follows another is read through the one it
    /// follows, since its own rows are not in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub same_as_device: Option<String>,
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

/// Why a condition or display field cannot be trusted with the installed
/// DCS-BIOS.
///
/// Either way the source is not what the profile was written for, and a lamp
/// reading it could light when nobody meant it to. So the chain it belongs to
/// is turned off rather than run on a guess.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Unsound {
    /// The catalogue has no signal by that name.
    Missing,
    /// The condition tests or scales against `value`, above the signal's
    /// highest, `max`.
    AboveRange { value: u32, max: u32 },
}

/// Where in a profile a flagged source sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "at", rename_all = "snake_case")]
pub enum Place {
    /// `bindings[binding].conditions[index]`: the whole lamp is off.
    Condition { binding: usize, index: usize },
    /// `bindings[binding].any_of[branch].conditions[index]`: that alternative
    /// is dropped, and the others still work.
    Branch { binding: usize, branch: usize, index: usize },
    /// `readouts[readout]`, through its text, format or colours: the field is
    /// left blank.
    Field { readout: usize },
}

/// One condition or display field that reads something the installed
/// DCS-BIOS does not give it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Flag {
    pub device: String,
    /// The lamp, or for a display field, its display and cells.
    pub target: String,
    pub place: Place,
    pub source: String,
    pub why: Unsound,
}

/// Whether one condition can be trusted. Unfinished conditions, with no
/// signal chosen, are `problems` to finish rather than flags.
fn unsound(module: &Module, c: &Condition) -> Option<Unsound> {
    if c.source.is_empty() {
        return None;
    }
    let Some(signal) = module.signal(&c.source) else {
        return Some(Unsound::Missing);
    };
    let output = signal.primary()?;
    if output.r#type != "integer" {
        return None;
    }
    let max = output.max_value?;
    let value = c.on_when.highest()?;
    (value > max).then_some(Unsound::AboveRange { value, max })
}

/// How much room a field's content needs, against how much it has.
///
/// A run of cells is a fixed width and nothing on the panel says when content
/// ran past it: the write goes out looking healthy and the tail is simply not
/// there, cut from whichever end the alignment anchors away from. So this is
/// worked out ahead of a single frame arriving, and the editor says how many
/// characters will be lost rather than warning that some might be.
///
/// It is a warning and never a refusal. Getting the length right is the user's
/// to judge: they know which readings their aircraft actually shows, and a
/// range wide enough to overflow in theory may never do it in the air.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Width {
    /// The most cells this content can ever need.
    pub widest: usize,
    /// How many cells it has.
    pub cells: usize,
    /// A piece reads a string DCS-BIOS gives no length for, so nothing
    /// bounds how wide it gets and `widest` is a floor rather than a maximum.
    pub unbounded: bool,
}

impl Width {
    /// Whether the content is known to run past its cells.
    pub fn overflows(&self) -> bool {
        self.widest > self.cells
    }

    /// How many characters would be dropped, where that is known.
    pub fn dropped(&self) -> usize {
        self.widest.saturating_sub(self.cells)
    }

    /// Cells the content cannot fill, where every piece is bounded.
    pub fn spare(&self) -> usize {
        self.cells.saturating_sub(self.widest)
    }
}

impl Readout {
    /// What this field needs against what it has.
    ///
    /// A string signal is bounded by the `max_length` DCS-BIOS declares for
    /// it, and a number by the range the user converted it to, or by its own
    /// maximum when it is shown as sent. A string with no declared length is
    /// the one thing nothing bounds, and it is reported rather than guessed at.
    pub fn width(&self, module: &Module) -> Width {
        let cells = self.cells.len();
        if self.divider {
            return Width { widest: cells, cells, unbounded: false };
        }
        // A run of one cell takes its whole value as a single glyph, however
        // many characters that is. That is not a shortcut: a two character
        // field really does occupy one cell on this hardware, the comm windows
        // carry `width: 2` and a Hornet scratchpad mark arrives as `" G"`.
        // `compose` hands the value over whole and `fit` joins rather than
        // crops, so nothing can be cut off the end of one cell and there is
        // nothing here to measure. Counted by character instead, every one of
        // these read as a field about to lose its last character.
        if cells == 1 {
            return Width { widest: 1, cells, unbounded: false };
        }
        let mut widest = 0;
        let mut unbounded = false;
        for span in &self.content {
            if !span.is_signal() {
                widest += span.text.chars().count();
                continue;
            }
            let Some(output) = module.signal(&span.source).and_then(|s| s.primary()) else {
                // A source this DCS-BIOS does not have draws nothing at all,
                // and is already flagged as its own problem.
                continue;
            };
            let number_max = (output.r#type != "string").then(|| output.number_max());
            let max_length = output.max_length.map(usize::from);
            match span.widest(max_length, number_max) {
                Some(n) => widest += n,
                None => unbounded = true,
            }
        }
        Width { widest, cells, unbounded }
    }
}

/// Every source a display field reads that this module lacks.
fn missing_in_field<'a>(module: &Module, r: &'a Readout) -> Vec<&'a str> {
    r.sources()
        .into_iter()
        .filter(|s| !s.is_empty() && module.signal(s).is_none())
        .collect()
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
            same_as_device: None,
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
    /// The text grid font to upload, for an aircraft whose own CDU is not one
    /// DCS-BIOS exports.
    ///
    /// An aircraft with a CDU of its own has a font drawn to match what the
    /// module sends for it, named in the display's `native_fonts`, and that
    /// font is the aircraft's rather than the user's: the choice would only be
    /// a way to get it wrong. This is for every other aircraft, where the
    /// screen holds whatever the user decided to put there and nothing has an
    /// opinion about which glyphs it should be drawn with.
    ///
    /// Named by font file, relative to the display, so it means the same thing
    /// as a `native_fonts` value. Ignored for an aircraft that has a native
    /// font, which keeps its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
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
    /// Devices that take another device's setup instead of holding their own,
    /// keyed by the one that follows.
    ///
    /// For variants of one panel, which WinWing sells under a name per seat:
    /// an MCDU set up as Captain can drive the Co-Pilot and Observer units
    /// too, rather than every lamp and field being written three times and
    /// kept in step by hand. Only between the same hardware, see
    /// [`DeviceSpec::same_hardware`], and one step deep: a device that follows
    /// cannot be followed, so there is always exactly one place to edit.
    ///
    /// The follower's own rows are kept and ignored while it follows, the way
    /// a disabled device's are, so stopping gives back what was there.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub follows: BTreeMap<String, String>,
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

    /// This profile as it runs: every device that follows another given a
    /// copy of that device's lamps and fields in place of its own.
    ///
    /// Done once, when the engine takes the profile, so nothing downstream has
    /// a second kind of device to know about. Each follower is still driven,
    /// disabled or not, under its own name, so disabling the one it follows
    /// leaves it running.
    pub fn with_followers(&self) -> Profile {
        let mut p = self.clone();
        if self.follows.is_empty() {
            return p;
        }
        p.bindings.retain(|b| !self.follows.contains_key(&b.device));
        p.readouts.retain(|r| !self.follows.contains_key(&r.device));
        for (follower, source) in &self.follows {
            // A chain is refused by `problems`. Should one be loaded anyway,
            // copying from a follower would copy the rows just set aside.
            if self.follows.contains_key(source) {
                continue;
            }
            p.bindings.extend(self.bindings.iter().filter(|b| &b.device == source).map(|b| {
                let mut b = b.clone();
                b.device = follower.clone();
                b
            }));
            p.readouts.extend(self.readouts.iter().filter(|r| &r.device == source).map(|r| {
                let mut r = r.clone();
                r.device = follower.clone();
                r
            }));
        }
        p
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
            font: None,
            bindings,
            // A stub lists hardware, and a display field is not hardware: it is
            // a decision about what to show. The editor shows a row for every
            // region of every screen regardless, the way it shows every lamp,
            // so an empty field needs no record here to be offered.
            readouts: Vec::new(),
            disabled_devices: Vec::new(),
            follows: BTreeMap::new(),
        }
    }

    /// Write the profile, replacing any existing file in one step.
    ///
    /// Written to a temporary file and renamed rather than written in place,
    /// because the running daemon watches this directory and a plain write is
    /// visible while it is still half finished. A reader would see truncated
    /// JSON, and the profile would appear to vanish for as long as it took to
    /// finish writing.
    ///
    /// CRLF, because these are Windows files that people open and read. A
    /// profile is meant to be looked at and hand edited, and `to_string_pretty`
    /// writes LF, which leaves the file as one long line in anything that still
    /// wants the pair. The shipped defaults are CRLF and end without a trailing
    /// newline, so a save gives back the same bytes the profile arrived as
    /// rather than a whole file of changed endings around one edited row.
    pub fn save(&self, path: &Path) -> Result<()> {
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Json(e, path.display().to_string()))?
            .replace('\n', "\r\n");
        let temp = path.with_extension("json.saving");
        // Rename replaces an existing file on Windows as well as on Unix.
        let written = std::fs::write(&temp, text).and_then(|()| std::fs::rename(&temp, path));
        if written.is_err() {
            // A half-written temporary is no use to anyone and the daemon
            // would list it on every poll.
            let _ = std::fs::remove_file(&temp);
        }
        written?;
        Ok(())
    }

    /// The device and lamp a mirroring lamp points at, if it mirrors one.
    ///
    /// A device that follows another is read as the one it follows, whose rows
    /// are the ones in use; after [`with_followers`](Self::with_followers) the
    /// two hold the same rows, so either reading gives the same value.
    fn mirror_target<'a>(&'a self, b: &'a Binding) -> Option<(&'a str, &'a str)> {
        let led = b.same_as.as_deref()?;
        let device = b.same_as_device.as_deref().unwrap_or(&b.device);
        let device = self.follows.get(device).map_or(device, String::as_str);
        Some((device, led))
    }

    /// The binding a mirroring lamp points at, if any.
    fn mirrored<'a>(&'a self, b: &'a Binding) -> Option<&'a Binding> {
        let (device, led) = self.mirror_target(b)?;
        self.bindings.iter().find(|o| o.device == device && o.led == led)
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

    /// Every signal this profile names, once each: in lamp conditions, and in
    /// display fields as the text, its format and its colours. Unfinished
    /// conditions and fields, which name nothing yet, are left out.
    pub fn signals_read(&self) -> Vec<&str> {
        let lamps = self.bindings.iter().flat_map(|b| b.sources());
        let fields = self.readouts.iter().flat_map(Readout::sources);
        let mut out: Vec<&str> = lamps.chain(fields).filter(|s| !s.is_empty()).collect();
        out.sort_unstable();
        out.dedup();
        out
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
            if let Some((target_device, target)) = self.mirror_target(b) {
                match self.mirrored(b) {
                    None => out.push(Error::UnknownMirror(
                        b.led.clone(),
                        target.to_string(),
                        target_device.to_string(),
                    )),
                    Some(other) => {
                        if other.same_as.is_some() {
                            out.push(Error::MirrorChain(b.led.clone(), target.to_string()));
                        }
                        // Only lamps that dim, on both ends. An indicator takes
                        // 0 or 1, so it has no level to follow and none to
                        // offer: mirroring one either way would be a setting
                        // that cannot mean what it says.
                        let dims = |device: &str, name: &str| {
                            devices
                                .device(device)
                                .and_then(|d| d.led(name))
                                .is_some_and(|(_, led)| led.is_dimmable())
                        };
                        if devices.device(&b.device).is_some()
                            && (!dims(&b.device, &b.led) || !dims(target_device, target))
                        {
                            out.push(Error::MirrorNotDimmable(b.led.clone(), target.to_string()));
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
                }
                // A signal this DCS-BIOS lacks is not a fault in the profile,
                // which may be written for another release: `flags` reports it
                // and `runnable` turns off what depends on it.
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
        self.readout_problems(module, devices, displays, &mut out, &mut Vec::new());
        out.retain(|e| !e.is_advisory());
        out
    }

    /// Display fields that lean on what DCS-BIOS says a signal is, which only
    /// the user can check.
    ///
    /// Never a refusal. DCS-BIOS metadata is not right for every module, so it
    /// can be wrong about a field that works. The profile loads and draws what
    /// it can, and the user is told rather than stopped from trying.
    ///
    /// Each comes with the index of the field it is about. Every field records
    /// where its findings start in the list, which places them without the
    /// checks themselves having to say.
    pub fn text_cautions(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
    ) -> Vec<(usize, String)> {
        let mut all = Vec::new();
        let mut starts = Vec::new();
        self.readout_problems(module, devices, displays, &mut all, &mut starts);
        let owner = |at: usize| -> Option<usize> {
            starts.iter().rev().find(|(_, from)| *from <= at).map(|(i, _)| *i)
        };
        all.into_iter()
            .enumerate()
            .filter(|(_, e)| e.is_advisory())
            .filter_map(|(at, e)| {
                owner(at).map(|i| {
                    (i, format!("{e}. It will load anyway, in case DCS-BIOS is wrong about it."))
                })
            })
            .collect()
    }

    /// Every condition and display field that reads what this DCS-BIOS does
    /// not give it: a signal it lacks, or a value above the signal's range.
    ///
    /// Not a reason to refuse the profile. It may be written for another
    /// release, and everything else in it still works. [`runnable`] is what
    /// runs instead, and this is what to tell the user about it.
    ///
    /// [`runnable`]: Self::runnable
    pub fn flags(&self, module: &Module) -> Vec<Flag> {
        let mut out = Vec::new();
        for (bi, b) in self.bindings.iter().enumerate() {
            let flag = |place, c: &Condition, why| Flag {
                device: b.device.clone(),
                target: b.led.clone(),
                place,
                source: c.source.clone(),
                why,
            };
            for (ci, c) in b.conditions.iter().enumerate() {
                if let Some(why) = unsound(module, c) {
                    out.push(flag(Place::Condition { binding: bi, index: ci }, c, why));
                }
            }
            for (ri, branch) in b.any_of.iter().enumerate() {
                for (ci, c) in branch.conditions.iter().enumerate() {
                    if let Some(why) = unsound(module, c) {
                        let place = Place::Branch { binding: bi, branch: ri, index: ci };
                        out.push(flag(place, c, why));
                    }
                }
            }
        }
        for (ri, r) in self.readouts.iter().enumerate() {
            for source in missing_in_field(module, r) {
                out.push(Flag {
                    device: r.device.clone(),
                    target: format!("{} cells {}", r.display, r.cells),
                    place: Place::Field { readout: ri },
                    source: source.to_string(),
                    why: Unsound::Missing,
                });
            }
        }
        out
    }

    /// The profile as it can safely run on this DCS-BIOS.
    ///
    /// A flagged condition takes its whole AND chain with it: running the rest
    /// of the chain without it could light the lamp under circumstances the
    /// chain was written to exclude. So a lamp's own conditions are cleared,
    /// leaving it unset and swept dark, and in an `any_of` only the branch
    /// holding the flag goes, since the other alternatives stand on their own.
    /// A display field reading a missing signal is left out, so its cells
    /// stay blank.
    ///
    /// Only this copy changes. The file keeps every row, so they work again
    /// once the source is fixed, the same rule the merge follows.
    pub fn runnable(&self, module: &Module) -> Profile {
        let bad = |conditions: &[Condition]| conditions.iter().any(|c| unsound(module, c).is_some());
        let mut p = self.clone();
        for b in &mut p.bindings {
            if bad(&b.conditions) {
                b.conditions.clear();
            }
            if !b.any_of.is_empty() {
                b.any_of.retain(|branch| !bad(&branch.conditions));
                if b.any_of.is_empty() {
                    // Nothing left to pick between.
                    b.pick = Pick::Brightest;
                }
            }
        }
        p.readouts.retain(|r| missing_in_field(module, r).is_empty());
        p
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
                    "{device} is disabled in this profile, so {lamps} lamp binding(s) and {fields} display field(s) on it do nothing"
                ));
            }
        }
        for (device, source) in &self.follows {
            let lamps = self.bindings.iter().filter(|b| &b.device == device && !b.is_placeholder()).count();
            let fields = self.readouts.iter().filter(|r| &r.device == device).count();
            if lamps + fields > 0 {
                out.push(format!(
                    "{device} follows {source} in this profile, so {lamps} lamp binding(s) and {fields} display field(s) of its own are kept but not used"
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

    /// Fields whose content will not fit the cells they were given.
    ///
    /// Never a refusal. A run is a fixed width and the tail is simply cut,
    /// from whichever end the alignment anchors away from, with nothing on the
    /// panel to say it happened. So it is said here instead, with the count,
    /// because a field that silently loses its last two digits reads as a
    /// working field showing the wrong number.
    ///
    /// Only what is known. A string with no declared length is unbounded and
    /// gets the shorter warning, since the widest it can draw depends on what
    /// the module sends rather than on anything written down.
    pub fn width_cautions(&self, module: &Module) -> Vec<String> {
        self.width_findings(module)
            .into_iter()
            .map(|(i, text)| {
                let r = &self.readouts[i];
                format!("{} cells {} {text}", r.display, r.cells)
            })
            .collect()
    }

    /// Every caution about what a display field will draw, each with the index
    /// of the field it is about, worded to sit beside that field.
    ///
    /// Two kinds, both about text output and neither a refusal: content that
    /// may not fit its cells, and settings that lean on what DCS-BIOS says a
    /// signal is. Kept apart from the profile's own cautions because a line
    /// naming cells in a list at the top of the page makes the user go and
    /// find the field, where one on the field is already there.
    pub fn field_cautions(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
    ) -> Vec<(usize, String)> {
        let mut out: Vec<(usize, String)> = self
            .width_findings(module)
            .into_iter()
            .map(|(i, text)| (i, format!("This field {text}")))
            .collect();
        out.extend(self.text_cautions(module, devices, displays));
        out.sort_by_key(|(i, _)| *i);
        out
    }

    /// What `width_cautions` says, without saying where.
    fn width_findings(&self, module: &Module) -> Vec<(usize, String)> {
        let mut out = Vec::new();
        for (i, r) in self.readouts.iter().enumerate() {
            if r.divider {
                continue;
            }
            let width = r.width(module);
            if width.overflows() {
                let n = width.dropped();
                let end = match r.align {
                    Align::Right => "first",
                    Align::Left => "last",
                    // Centred content is cropped at both ends, the odd one
                    // coming off the front, the way its padding is added.
                    Align::Centre => "outermost",
                };
                out.push((i, format!(
                    "needs up to {} cells and has {}, so the {end} {n} character{} would be dropped with nothing shown on the panel to say so.",
                    width.widest,
                    width.cells,
                    if n == 1 { "" } else { "s" }
                )));
            } else if width.unbounded {
                out.push((i, format!(
                    "reads text DCS-BIOS gives no length for, so how wide it draws is not known ahead of time and it may run past its {} cells.",
                    width.cells
                )));
            }
        }
        out
    }

    /// Check the display fields: that they name real glass, sit inside it, do
    /// not fight over cells, and read a source that can actually fill them.
    ///
    /// Appends rather than returning, for the reason given on
    /// [`problems`](Self::problems): one fault must not hide another.
    ///
    /// `starts` gets each field's index and where its findings begin in `out`,
    /// for a caller that shows them on the field rather than in a list.
    fn readout_problems(
        &self,
        module: &Module,
        devices: &DeviceInventory,
        displays: &DisplayCatalogue,
        out: &mut Vec<Error>,
        starts: &mut Vec<(usize, usize)>,
    ) {
        for name in &self.disabled_devices {
            if devices.device(name).is_none() {
                out.push(Error::DisablesUnknownDevice(name.clone()));
            }
        }
        for (follower, source) in &self.follows {
            if follower == source {
                out.push(Error::FollowsItself(follower.clone()));
                continue;
            }
            let (Some(a), Some(b)) = (devices.device(follower), devices.device(source)) else {
                let unknown = if devices.device(follower).is_none() { follower } else { source };
                out.push(Error::FollowsUnknownDevice(
                    follower.clone(),
                    source.clone(),
                    unknown.clone(),
                ));
                continue;
            };
            if self.follows.contains_key(source) {
                out.push(Error::FollowChain(follower.clone(), source.clone()));
            }
            if !a.same_hardware(b) {
                out.push(Error::FollowsDifferentHardware(follower.clone(), source.clone()));
            }
        }
        for (i, r) in self.readouts.iter().enumerate() {
            starts.push((i, out.len()));
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

            // A divider reads nothing, so every check below it is about a
            // source it does not have. What it can get wrong is its own: glass
            // that cannot draw it, a signal named anyway, or a run with no room
            // for a dash between two margins.
            if r.divider {
                if !display.is_text_grid() {
                    out.push(Error::DividerNotDrawn(
                        r.display.clone(),
                        r.cells.to_string(),
                    ));
                }
                if let Some(source) = r.sources().first() {
                    out.push(Error::DividerReadsSignal(
                        r.display.clone(),
                        r.cells.to_string(),
                        (*source).to_string(),
                    ));
                }
                // There is no minimum for the rule itself: it runs corner
                // to corner of its cells, and one cell is one dash. A label is
                // the only thing here that needs room.
                if !r.label.is_empty() && r.cells.len() < min_divider_cells(&r.label) {
                    // A label with no room is left off the rule rather than
                    // crowding it, so without this the rule would quietly draw
                    // plain and nothing would say where the label went.
                    out.push(Error::DividerLabelTooWide(
                        r.display.clone(),
                        r.cells.to_string(),
                        r.cells.len(),
                        min_divider_cells(&r.label),
                        r.label.clone(),
                    ));
                }
                self.text_problems(r, display, out);
                continue;
            }

            // A field with nothing in it is unfinished work rather than a
            // mistake, but it still stops the profile loading, so it is said
            // plainly and in those terms. A chain with an empty span in the
            // middle is the same thing: a piece somebody started and left.
            if r.content.is_empty() || r.content.iter().any(Span::is_empty) {
                out.push(Error::UnfinishedField(r.display.clone(), r.cells.to_string()));
                continue;
            }

            for span in &r.content {
                // A box wider than the run it sits in cannot be drawn: the
                // field crops what will not fit, so the piece would take the
                // whole run and whatever shares it would be the part that
                // goes. Refused rather than cautioned, unlike an overflow,
                // because this one is certain before a single frame arrives.
                if span.width > r.cells.len() {
                    out.push(Error::SpanWiderThanField(
                        r.display.clone(),
                        r.cells.to_string(),
                        span.width,
                        r.cells.len(),
                    ));
                }
                // A rule fills room it was given rather than drawing anything
                // of its own, which is what a gap is. On a piece that has its
                // own content there is nowhere to put it.
                if span.rule {
                    if !span.gap {
                        out.push(Error::RuleNotOnGap(
                            r.display.clone(),
                            r.cells.to_string(),
                        ));
                    }
                    if !display.is_text_grid() {
                        out.push(Error::DividerNotDrawn(
                            r.display.clone(),
                            r.cells.to_string(),
                        ));
                    }
                }
                if !span.label.is_empty() {
                    if !span.rule {
                        out.push(Error::LabelNotOnRule(
                            r.display.clone(),
                            r.cells.to_string(),
                            span.label.clone(),
                        ));
                    } else if span.width == 0 {
                        // An elastic rule is as wide as the chain leaves it,
                        // which changes with every reading beside it, so there
                        // is no width to check a label against. `divider_rule`
                        // drops a label it cannot fit, which on a rule that
                        // keeps changing width means a label appearing and
                        // vanishing on the glass with nothing to say why.
                        out.push(Error::RuleLabelNeedsWidth(
                            r.display.clone(),
                            r.cells.to_string(),
                            span.label.clone(),
                        ));
                    } else if span.width < min_divider_cells(&span.label) {
                        out.push(Error::RuleLabelTooWide(
                            r.display.clone(),
                            r.cells.to_string(),
                            span.width,
                            min_divider_cells(&span.label),
                            span.label.clone(),
                        ));
                    }
                }
                // A gap draws nothing and measures itself from what is left,
                // so anything written on one is something that will never be
                // seen. Refused rather than ignored, for the same reason a
                // signal on a divider is.
                if span.gap {
                    if !span.text.is_empty() || span.is_signal() {
                        out.push(Error::GapHasContent(
                            r.display.clone(),
                            r.cells.to_string(),
                        ));
                    }
                    continue;
                }
                // Characters and a signal are two different answers to what
                // this span draws, so a span holding both is one somebody
                // half changed rather than one that means anything.
                if !span.text.is_empty() && span.is_signal() {
                    out.push(Error::SpanReadsAndWrites(
                        r.display.clone(),
                        r.cells.to_string(),
                        span.source.clone(),
                    ));
                    continue;
                }
                // What a span writes is only shaped by what it reads, so
                // nothing below applies to characters the user typed. Their
                // one rule, that the font can draw them, is in text_problems
                // where the font is known.
                if !span.is_signal() {
                    if span.shapes_a_number() {
                        out.push(Error::RangeOnText(span.text.clone()));
                    }
                    if !span.value_aliases.is_empty() {
                        out.push(Error::AliasesOnText(span.text.clone()));
                    }
                    continue;
                }
                // Flagged rather than refused, as for a lamp condition.
                let Some(output) = module.signal(&span.source).and_then(|s| s.primary()) else {
                    continue;
                };
                // A number shown as sent, converted or as words is the user's
                // choice. Raw 0 to 65535 is a strange thing to put on a
                // screen, but the editor says so where the choice is made,
                // and it draws exactly what it says it will.
                if output.r#type == "string" {
                    if span.reads.is_some()
                        || span.wrap.is_some()
                        || span.round != Round::Nearest
                        || span.abs
                    {
                        out.push(Error::RangeOnText(span.source.clone()));
                    }
                    if !span.value_aliases.is_empty() {
                        out.push(Error::AliasesOnText(span.source.clone()));
                    }
                } else {
                    band_problems(span, output.number_max(), out);
                }

                if let Some(format) = &span.format {
                    if !display.draws_inverse() {
                        out.push(Error::FormatNotDrawn(r.display.clone(), r.cells.to_string()));
                    }
                    if let Some(o) = module.signal(format).and_then(|s| s.primary()) {
                        if o.r#type != "string" {
                            out.push(Error::FormatNotText(format.clone()));
                        }
                    }
                }

                if let Some(colours) = &span.colours {
                    if let Some(o) = module.signal(&colours.source).and_then(|s| s.primary()) {
                        if o.r#type != "string" {
                            out.push(Error::FormatNotText(colours.source.clone()));
                        }
                    }
                    for code in colours.codes.keys() {
                        if code.chars().count() != 1 {
                            out.push(Error::ColourCodeNotOneChar(code.clone()));
                        }
                    }
                }

                for (from, to) in &span.replace {
                    if from.chars().count() != 1 || to.chars().count() != 1 {
                        out.push(Error::ReplaceNotOneChar(from.clone(), to.clone()));
                    }
                }
            }

            // Gaps space out what is around them. With nothing around them
            // they are an elaborate way of writing blanks, which is what an
            // empty run already does. A rule is not blanks: a field that is
            // nothing but one is a divider written the long way, and drawing
            // it is the right answer rather than a fault.
            if !r.content.is_empty() && r.content.iter().all(|s| s.gap && !s.rule) {
                out.push(Error::NothingButGaps(
                    r.display.clone(),
                    r.cells.to_string(),
                ));
            }

            // Inverse is the one piece of styling a span can ask for on glass
            // that is not a text grid, so it is checked against what the
            // display can actually do rather than lumped in with colour.
            if !display.draws_inverse() && r.content.iter().any(|s| s.inverse) {
                out.push(Error::FormatNotDrawn(r.display.clone(), r.cells.to_string()));
            }

            self.text_problems(r, display, out);
        }
    }

    /// What only a text grid can take, and what a text grid needs.
    ///
    /// An aircraft with a CDU of its own takes its font from the aircraft:
    /// the glyphs are drawn to match what DCS-BIOS sends for that module, so
    /// the choice would only be a way to get it wrong. An aircraft without one
    /// takes the profile's `font`, because nothing else has an opinion about
    /// what its screen should look like, and until one is picked there is no
    /// alphabet to check anything against.
    ///
    /// Every character the field puts on the glass has to be one that font
    /// draws, whether the user typed it or a `replace` rewrote a signal into
    /// it. A character the font lacks is a blank cell on the panel with
    /// nothing to say why.
    fn text_problems(&self, r: &Readout, display: &Display, out: &mut Vec<Error>) {
        let Some(text) = &display.text else {
            let styled = r.content.iter().any(|s| {
                s.colour.is_some()
                    || s.colours.is_some()
                    || s.small
                    || s.label_colour.is_some()
                    // A band's colour is styling like any other, and a band
                    // that asks for one on glass with no colours is a setting
                    // nothing draws.
                    || s.value_aliases.values().any(|a| a.colour.is_some())
            });
            let ruled = r.divider && (r.colour.is_some() || r.label_colour.is_some());
            if styled || ruled {
                out.push(Error::StyleNotDrawn(r.display.clone(), r.cells.to_string()));
            }
            return;
        };
        let mut fonts = Vec::new();
        for aircraft in &self.aircraft {
            match text.font_with(aircraft, self.font.as_deref()) {
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
            // A rule is drawn from the font like any other character, so a font
            // without a dash would rule the line in blanks and look broken.
            if r.divider {
                // The label is drawn from the same font as the rule, at full
                // size, so a character it lacks is a blank cell in the middle
                // of the line with nothing to say why. The blank is only drawn
                // where there is a label to set apart from the line.
                let spaced = if r.label.is_empty() { None } else { Some(' ') };
                for c in std::iter::once('-').chain(spaced).chain(r.label.chars()) {
                    if !chars.large.contains(&c) {
                        out.push(Error::NotInFont(c, file.to_string(), r.cells.to_string()));
                    }
                }
                continue;
            }
            for span in &r.content {
                // Each size has its own alphabet, and small is the smaller one
                // in every font here, so a span marked small can lose a
                // character that was fine at full size.
                let set = if span.small { &chars.small } else { &chars.large };
                // A gap is drawn as blank cells like any other character, so a
                // font without a space would leave a hole rather than a gap. A
                // rule draws dashes instead, and its label is drawn from the
                // same alphabet as the rule, with a blank each side of it.
                if span.gap {
                    let (fill, spaced) = if span.rule {
                        ('-', if span.label.is_empty() { None } else { Some(' ') })
                    } else {
                        (' ', None)
                    };
                    let written = std::iter::once(fill)
                        .chain(spaced)
                        .chain(span.label.chars());
                    for c in written {
                        if !set.contains(&c) {
                            out.push(Error::NotInFont(c, file.to_string(), r.cells.to_string()));
                        }
                    }
                    continue;
                }
                let written = span
                    .text
                    .chars()
                    .chain(span.value_aliases.values().flat_map(|a| a.text.chars()))
                    .chain(span.replace.values().filter_map(|to| to.chars().next()));
                for c in written {
                    if !set.contains(&c) {
                        out.push(Error::NotInFont(c, file.to_string(), r.cells.to_string()));
                    }
                }
            }
        }
    }
}

/// What a reading's alias bands can be wrong about.
///
/// Overlaps are refused. Two bands claiming one reading would make what gets
/// drawn depend on the order they happen to be held in, and a reading draws
/// one thing, the way a cell has one field.
///
/// A band outside everything the face reads is only a caution: it draws
/// nothing rather than drawing something wrong. It is worth saying because the
/// likeliest way to write one is to band a converted face in the raw counts
/// DCS-BIOS sends, and that mistake is otherwise silent.
fn band_problems(span: &Span, max: u16, out: &mut Vec<Error>) {
    if span.value_aliases.is_empty() {
        return;
    }
    let tol = span.tolerance();
    let bands: Vec<&ValueBand> = span.value_aliases.keys().collect();
    for (i, a) in bands.iter().enumerate() {
        for b in &bands[i + 1..] {
            if a.overlaps(b, tol) {
                out.push(Error::AliasBandsOverlap(
                    a.to_string(),
                    b.to_string(),
                    span.source.clone(),
                ));
            }
        }
    }
    let [low, high] = span.reads.unwrap_or([0.0, f64::from(max)]);
    let (lo, hi) = (low.min(high), low.max(high));
    for band in bands {
        if band.highest() < lo - tol || band.lowest() > hi + tol {
            out.push(Error::AliasBandUnreachable(
                band.to_string(),
                span.source.clone(),
                low.to_string(),
                high.to_string(),
            ));
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
/// Seeding only ever adds, and a profile the user has is theirs. An update may
/// still correct a field they never touched, which is what `previous` is for: a
/// copy of the defaults as the last release shipped them, so a field can be
/// told apart from one somebody edited. Everything else is left alone, and
/// [`reset_to_default`] remains the only path that overwrites wholesale.
pub struct Profiles {
    pub defaults: PathBuf,
    /// The defaults as the last release shipped them, or empty for none.
    ///
    /// Empty, missing or stale all mean the same thing and are all safe: no
    /// field matches anything, so nothing is corrected and nothing is removed.
    pub previous: PathBuf,
    pub active: PathBuf,
}

/// The file in the active folder naming the version that last reconciled it.
///
/// Not `.json`, so every path that walks the folder for profiles skips it.
const UPDATED: &str = ".updated";

/// What reconciling one profile against a new release came to.
#[derive(Default)]
struct FieldWork {
    added: usize,
    updated: usize,
    removed: usize,
    lamps: usize,
    settings: usize,
}

impl FieldWork {
    fn nothing(&self) -> bool {
        self.added == 0
            && self.updated == 0
            && self.removed == 0
            && self.lamps == 0
            && self.settings == 0
    }
}

/// The snapshot folder for a defaults folder: the same name, `-previous`.
///
/// One rule rather than one per layout, because the daemon lets `--defaults`
/// point anywhere and a snapshot that did not follow it would be read from the
/// install while the defaults came from somewhere else. Empty for a path with
/// no file name, which is how callers that only read profiles pass no defaults
/// at all.
fn snapshot_beside(defaults: &Path) -> PathBuf {
    match defaults.file_name().and_then(|n| n.to_str()) {
        Some(name) => defaults.with_file_name(format!("{name}-previous")),
        None => PathBuf::new(),
    }
}

/// What identifies a field across two versions of a default.
///
/// The cells are part of it, so a field that moved reads as the old one gone
/// and a new one arriving. That is what stops a moved field being drawn twice.
fn field_key(r: &Readout) -> (String, String, String) {
    (r.device.clone(), r.display.clone(), r.cells.to_string())
}

fn at<'a>(list: &'a [Readout], key: &(String, String, String)) -> Option<&'a Readout> {
    list.iter().find(|r| &field_key(r) == key)
}

/// Whether two fields say the same thing.
///
/// Compared as values rather than text: `replace` and `aliases` are hash maps
/// and their key order on disk is arbitrary, and a field of one piece is
/// written flat while a chain is written as an array. Both sides go through
/// the same types first, so neither difference is mistaken for an edit.
fn same_field(a: &Readout, b: &Readout) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Bring one profile's fields up to what the default now ships, keeping
/// anything the user has made their own.
///
/// `was` is what the last release shipped and is the whole basis for telling
/// those apart. Five cases, and the last one is the reason this needs a
/// snapshot at all:
///
/// * still what we shipped, and still shipped: take the new one
/// * still what we shipped, no longer shipped: take it out
/// * changed from what we shipped: leave it, it is theirs
/// * missing, and we never shipped it: add it
/// * missing, and we did ship it: leave it missing, they deleted it
///
/// With no snapshot the first, second and last collapse into "add what does
/// not clash", which is where this started and is still the safe answer.
fn reconcile_fields(profile: &mut Profile, shipped: &[Readout], was: &[Readout]) -> FieldWork {
    let mut work = FieldWork::default();

    let mut kept: Vec<Readout> = Vec::with_capacity(profile.readouts.len());
    for r in std::mem::take(&mut profile.readouts) {
        let key = field_key(&r);
        let Some(before) = at(was, &key) else {
            kept.push(r);
            continue;
        };
        if !same_field(&r, before) {
            // Theirs, and it stays theirs, whole. A setting introduced after
            // they edited it needs nothing written here: every key on a field
            // is optional and absent means its default, so a row from before
            // a setting existed already reads as that setting turned off.
            // That is the only sensible value for it, because the row was
            // built without it and looked right.
            //
            // Filling the key in instead would mean deciding, from the JSON
            // alone, which keys are new to a field and which ones the default
            // has merely started setting. Those look identical, so a colour or
            // a rounding the new default chose would land on a row the user
            // owns, which is the one thing this promises not to do.
            kept.push(r);
            continue;
        }
        match at(shipped, &key) {
            Some(now) => {
                if !same_field(now, before) {
                    work.updated += 1;
                }
                kept.push(now.clone());
            }
            None => work.removed += 1,
        }
    }
    profile.readouts = kept;

    for r in shipped {
        // Shipped before and not here now is a field the user took out, and
        // taking one out is as much a decision as editing one.
        if at(was, &field_key(r)).is_some() {
            continue;
        }
        // Otherwise it is new, and lands only where it cannot collide: the
        // user may have claimed those cells, and a suggestion does not
        // outrank that.
        let clash = profile.readouts.iter().any(|o| {
            o.device == r.device && o.display == r.display && o.cells.overlaps(&r.cells)
        });
        if clash {
            continue;
        }
        profile.readouts.push(r.clone());
        work.added += 1;
    }

    work
}

/// Whether two values say the same thing, compared as JSON for the same
/// reason [`same_field`] is.
fn same_value<T: Serialize>(a: &T, b: &T) -> bool {
    match (serde_json::to_value(a), serde_json::to_value(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Bring lamp rows still as the last release shipped them up to the new one.
///
/// The field rule, narrowed to what lamps can do: a lamp is hardware, so its
/// row is never added or removed here (new hardware is added on every start
/// regardless). A row identical to what `was` held is ours to replace; one
/// that differs, or one `was` never had, is the user's and stays.
fn reconcile_lamps(profile: &mut Profile, shipped: &[Binding], was: &[Binding]) -> usize {
    let key = |b: &Binding| (b.device.clone(), b.led.clone());
    let was: HashMap<_, _> = was.iter().map(|b| (key(b), b)).collect();
    let now: HashMap<_, _> = shipped.iter().map(|b| (key(b), b)).collect();
    let mut changed = 0;
    for b in &mut profile.bindings {
        let k = key(b);
        let (Some(before), Some(after)) = (was.get(&k), now.get(&k)) else {
            continue;
        };
        if same_value(&*b, *before) && !same_value(*after, *before) {
            *b = (*after).clone();
            changed += 1;
        }
    }
    changed
}

/// Bring the profile's own settings up to the new release, one entry at a
/// time: the font, each device's `follows`, and whether each device is
/// disabled.
///
/// Each is the user's once it differs from what `was` said, and ours while it
/// does not. Taken entry by entry rather than as a whole map, so pointing one
/// MFD at another does not freeze the MCDU's entry the release changed.
///
/// These travel with the rows and fields reconciled beside them: a release
/// that adds MCDU fields to an aircraft with no CDU of its own also names the
/// font they are drawn in, and fields merged without it leave a profile the
/// daemon refuses to load at all.
fn reconcile_settings(profile: &mut Profile, shipped: &Profile, was: &Profile) -> usize {
    let mut changed = 0;

    if profile.font == was.font && shipped.font != was.font {
        profile.font = shipped.font.clone();
        changed += 1;
    }

    let followers: BTreeSet<&String> = was.follows.keys().chain(shipped.follows.keys()).collect();
    for device in followers {
        let (before, after) = (was.follows.get(device), shipped.follows.get(device));
        if profile.follows.get(device) != before || after == before {
            continue;
        }
        match after {
            Some(source) => profile.follows.insert(device.clone(), source.clone()),
            None => profile.follows.remove(device),
        };
        changed += 1;
    }

    let has = |list: &[String], d: &String| list.contains(d);
    let mut devices: Vec<&String> = Vec::new();
    for d in was.disabled_devices.iter().chain(&shipped.disabled_devices) {
        if !devices.contains(&d) {
            devices.push(d);
        }
    }
    for device in devices {
        let (before, after) = (has(&was.disabled_devices, device), has(&shipped.disabled_devices, device));
        if has(&profile.disabled_devices, device) != before || after == before {
            continue;
        }
        if after {
            profile.disabled_devices.push(device.clone());
        } else {
            profile.disabled_devices.retain(|d| d != device);
        }
        changed += 1;
    }

    changed
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

/// Aircraft grouped by the shipped default that lists them.
///
/// Two aircraft in one family can share a profile: one of them can be handed
/// to a profile flying the other. An aircraft no default lists is grouped by
/// its module, since nothing has said its module's aircraft differ.
pub struct Families(HashMap<String, String>);

impl Families {
    /// The family `aircraft` belongs to, on a profile reading `module`.
    pub fn of(&self, aircraft: &str, module: &str) -> String {
        match self.0.get(aircraft) {
            Some(file) => file.clone(),
            None => format!("module {module}"),
        }
    }

    /// Whether `aircraft` could join `target`: it reads the same module and
    /// already flies an aircraft of the same family.
    pub fn fits(&self, aircraft: &str, module: &str, target: &Profile) -> bool {
        let family = self.of(aircraft, module);
        target.module == module && target.aircraft.iter().any(|a| self.of(a, &target.module) == family)
    }
}

impl Profiles {
    pub fn new(defaults: impl Into<PathBuf>, active: impl Into<PathBuf>) -> Self {
        let defaults = defaults.into();
        Profiles {
            previous: snapshot_beside(&defaults),
            defaults,
            active: active.into(),
        }
    }

    /// Point somewhere else for the defaults as the last release shipped them.
    ///
    /// Only tests need this. Everything else takes the folder beside the
    /// defaults, which is where it ships and where `--defaults` keeps it.
    pub fn with_previous(mut self, previous: impl Into<PathBuf>) -> Self {
        self.previous = previous.into();
        self
    }

    /// The version that last reconciled the active folder, if it says.
    fn last_update(&self) -> Option<String> {
        std::fs::read_to_string(self.active.join(UPDATED))
            .ok()
            .map(|s| s.trim().to_string())
    }

    /// What the last release shipped for this profile, or nothing.
    fn previously_shipped(&self, name: &str) -> Option<Profile> {
        if self.previous.as_os_str().is_empty() {
            return None;
        }
        Profile::load(&self.previous.join(name)).ok()
    }

    /// Every aircraft an active profile already claims, with the name of the
    /// profile that claims it.
    ///
    /// Two profiles for one aircraft is the conflict worth preventing, not two
    /// on one module: the F/A-18C and F/A-18E profiles share a module and
    /// claim different aircraft, which is fine. A profile that will not parse
    /// claims nothing, since it cannot be loaded to fly either.
    pub fn claimed_aircraft(&self) -> HashMap<String, String> {
        self.claimed_except(None)
    }

    /// Whether an active profile other than `file` is already called `name`.
    ///
    /// The name is the only thing that identifies a profile to the user: the
    /// file name is never shown, and never changes once written. Two profiles
    /// reading the same in the list cannot be told apart, and the one wanted
    /// is a guess. Compared without case or surrounding space, because that is
    /// how a user reads two names as the same.
    pub fn name_taken(&self, file: &str, name: &str) -> Option<String> {
        self.name_taken_except(&[file], name)
    }

    /// [`name_taken`](Self::name_taken), leaving out every file in `skip`: the
    /// ones about to be deleted, whose names are free to reuse.
    pub fn name_taken_except(&self, skip: &[&str], name: &str) -> Option<String> {
        let wanted = name.trim().to_lowercase();
        let entries = std::fs::read_dir(&self.active).ok()?;
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter(|p| p.file_name().and_then(|n| n.to_str()).is_none_or(|n| !skip.contains(&n)))
            .collect();
        paths.sort();
        paths.into_iter().find_map(|path| {
            let p = Profile::load(&path).ok()?;
            (p.name.trim().to_lowercase() == wanted).then_some(p.name)
        })
    }

    /// [`claimed_aircraft`](Self::claimed_aircraft), leaving out the claims of
    /// one file, for asking what a profile could take back without counting
    /// its own.
    pub fn claimed_except(&self, file: Option<&str>) -> HashMap<String, String> {
        let mut out = HashMap::new();
        let Ok(entries) = std::fs::read_dir(&self.active) else {
            return out;
        };
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .filter(|p| file.is_none() || p.file_name().and_then(|n| n.to_str()) != file)
            .collect();
        // In file name order, which is the order the daemon loads them in, so
        // an aircraft claimed twice is named after the profile that wins.
        paths.sort();
        for path in paths {
            if let Ok(p) = Profile::load(&path) {
                for a in &p.aircraft {
                    out.entry(a.clone()).or_insert_with(|| p.name.clone());
                }
            }
        }
        out
    }

    /// Copy in every default the active folder does not already have, returning
    /// a line for each one copied. Creates the active folder if it is missing.
    ///
    /// Safe to run on every start, which is the point: install, update and a
    /// user who deleted the folder all take the same path.
    ///
    /// A default comes in only for the aircraft no profile already claims. The
    /// file name is not the claim, the aircraft is: a user who deleted a
    /// shipped profile after moving its aircraft elsewhere, or an update that
    /// ships a default for an aircraft the user already set up themselves,
    /// would otherwise end with two profiles for one aircraft and the daemon
    /// flying whichever sorts first. One with nothing left to claim is skipped.
    pub fn seed(&self) -> Result<Vec<String>> {
        if !self.defaults.is_dir() {
            return Ok(Vec::new());
        }
        std::fs::create_dir_all(&self.active)?;
        let mut claimed = self.claimed_aircraft();
        let mut defaults: Vec<PathBuf> = std::fs::read_dir(&self.defaults)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
            .collect();
        defaults.sort();
        let mut copied = Vec::new();
        for from in defaults {
            let Some(name) = from.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            let to = self.active.join(&name);
            if to.exists() {
                continue;
            }
            // A default that will not parse is our fault, not the user's, and
            // copying it is how the daemon gets to say what is wrong with it.
            let Ok(mut profile) = Profile::load(&from) else {
                std::fs::copy(&from, &to)?;
                copied.push(name);
                continue;
            };
            let (free, taken): (Vec<String>, Vec<String>) =
                profile.aircraft.iter().cloned().partition(|a| !claimed.contains_key(a));
            if free.is_empty() {
                continue;
            }
            if taken.is_empty() {
                std::fs::copy(&from, &to)?;
                copied.push(name);
            } else {
                profile.aircraft = free;
                profile.save(&to)?;
                copied.push(format!("{name} without {}, which another profile has", taken.join(", ")));
            }
            for a in &profile.aircraft {
                claimed.insert(a.clone(), profile.name.clone());
            }
        }
        Ok(copied)
    }

    /// Which aircraft belong together, as the shipped defaults group them.
    ///
    /// Sharing a module is not enough to share a profile. The F-14 and F-14BU
    /// read one module and ship apart because their screens differ, and "No
    /// aircraft" rides on FC3 without being an FC3 aircraft. Each shipped file
    /// is that decision already made, so it is the grouping; see [`Families`].
    pub fn families(&self) -> Families {
        let mut out = HashMap::new();
        if let Ok(entries) = std::fs::read_dir(&self.defaults) {
            let mut paths: Vec<PathBuf> = entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .collect();
            paths.sort();
            for path in paths {
                let Ok(p) = Profile::load(&path) else { continue };
                let file = path.file_name().unwrap_or_default().to_string_lossy().into_owned();
                for a in p.aircraft {
                    out.entry(a).or_insert_with(|| file.clone());
                }
            }
        }
        Families(out)
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
    /// Lamp rows are never touched, in either direction: not rewritten, not
    /// removed, not reordered relative to what they say. A profile whose device
    /// has been unplugged keeps its rows, because unplugging a panel for an
    /// evening is not a decision to discard its configuration.
    ///
    /// Display fields are the one exception, and only against [`previous`]:
    /// a field still identical to what the last release shipped is ours to
    /// correct or retire, and anything else is the user's. See
    /// [`reconcile_fields`] for the five cases.
    ///
    /// That runs once per `version`, recorded in the active folder, rather than
    /// every start. Otherwise a user who puts a field back the way they liked
    /// it would have it taken away again at the next launch, and every launch
    /// after that. It is skipped entirely where the defaults and the active
    /// folder are one folder, which is a development checkout: there is nothing
    /// to reconcile against and the files are tracked.
    ///
    /// The cost of keeping one snapshot rather than all of them: somebody two
    /// releases behind has fields matching a snapshot we no longer hold, so
    /// they read as edited and stay as they are. A frozen field still works,
    /// and the alternative is overwriting someone who deliberately went back to
    /// an older layout.
    ///
    /// [`previous`]: Self::previous
    pub fn merge_new(&self, devices: &DeviceInventory, version: &str) -> Result<Vec<String>> {
        if !self.active.is_dir() {
            return Ok(Vec::new());
        }
        // Same folder means a development checkout, where the default and the
        // profile being flown are the same file and correcting one against
        // itself is noise at best.
        let upgrading =
            self.defaults != self.active && self.last_update().as_deref() != Some(version);

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
            let mut work = FieldWork::default();

            if let Ok(shipped) = Profile::load(&self.defaults.join(&name)) {
                for b in &shipped.bindings {
                    if have.insert((b.device.clone(), b.led.clone())) {
                        profile.bindings.push(b.clone());
                        from_default += 1;
                    }
                }
                if upgrading {
                    // With no snapshot there is nothing to tell an untouched
                    // row from an edited one, so only fields go on, adding
                    // what does not clash, and lamps and settings stay put.
                    let was = self.previously_shipped(&name);
                    let fields = was.as_ref().map_or(&[][..], |w| &w.readouts[..]);
                    work = reconcile_fields(&mut profile, &shipped.readouts, fields);
                    if let Some(was) = &was {
                        work.lamps = reconcile_lamps(&mut profile, &shipped.bindings, &was.bindings);
                        work.settings = reconcile_settings(&mut profile, &shipped, was);
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
            // Field work is counted rather than measured as a change in length:
            // a release that retires one field and adds another nets to zero
            // and would save nothing, and a release that retires two would
            // underflow the subtraction it used to be.
            if added == 0 && work.nothing() && !order_changed {
                continue;
            }
            profile.save(&path)?;
            let mut what = Vec::new();
            if added > 0 {
                what.push(format!(
                    "added {added} row(s), {from_default} from the shipped default"
                ));
            }
            if work.added > 0 {
                what.push(format!(
                    "added {} display field(s) from the shipped default",
                    work.added
                ));
            }
            if work.updated > 0 {
                what.push(format!(
                    "updated {} unchanged display field(s) to the new default",
                    work.updated
                ));
            }
            if work.removed > 0 {
                what.push(format!(
                    "removed {} display field(s) the default no longer ships",
                    work.removed
                ));
            }
            if work.lamps > 0 {
                what.push(format!(
                    "updated {} unchanged lamp row(s) to the new default",
                    work.lamps
                ));
            }
            if work.settings > 0 {
                what.push(format!(
                    "updated {} unchanged profile setting(s) to the new default",
                    work.settings
                ));
            }
            match what.is_empty() {
                true => notes.push(format!("{name}: reordered")),
                false => notes.push(format!("{name}: {}", what.join(", "))),
            }
        }
        // Written whether or not anything changed: the question it answers is
        // "has this version had its turn", not "did it find work".
        if upgrading {
            std::fs::write(self.active.join(UPDATED), format!("{version}\r\n"))?;
        }
        Ok(notes)
    }

    /// Delete a profile, shipped or made by the user.
    ///
    /// A shipped one stays deleted only while other profiles claim all of its
    /// aircraft, because [`seed`](Self::seed) brings a default back for any
    /// aircraft nothing claims. That is the intent: an aircraft is never left
    /// without a profile by accident, and the editor says so before deleting.
    pub fn delete(&self, file: &str) -> Result<()> {
        // A bare file name, so nothing outside the active folder is reachable.
        if Path::new(file).file_name().and_then(|n| n.to_str()) != Some(file) {
            return Err(Error::NotAProfileFile(file.to_string()));
        }
        std::fs::remove_file(self.active.join(file))?;
        Ok(())
    }

    /// Overwrite one active profile with its shipped default.
    ///
    /// Destroys user work, so it is never reached except by someone clicking
    /// reset. The lamps go back to how they shipped; the aircraft only where
    /// no other profile has taken them since, so a profile split with Copy
    /// to... is not claimed twice by resetting the half that shipped. If every
    /// shipped aircraft is taken, it keeps the ones it has.
    pub fn reset_to_default(&self, file: &str) -> Result<()> {
        let from = self.defaults.join(file);
        if !from.is_file() {
            return Err(Error::NoDefault(file.to_string()));
        }
        std::fs::create_dir_all(&self.active)?;
        let to = self.active.join(file);
        let Ok(mut shipped) = Profile::load(&from) else {
            std::fs::copy(&from, &to)?;
            return Ok(());
        };
        let others = self.claimed_except(Some(file));
        let free: Vec<String> =
            shipped.aircraft.iter().filter(|a| !others.contains_key(*a)).cloned().collect();
        if free.len() == shipped.aircraft.len() {
            std::fs::copy(&from, &to)?;
            return Ok(());
        }
        shipped.aircraft = if free.is_empty() {
            Profile::load(&to).map(|p| p.aircraft).unwrap_or_default()
        } else {
            free
        };
        shipped.save(&to)?;
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
            same_as_device: None,
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
            same_as_device: None,
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
            same_as_device: None,
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
            font: None,
            bindings: vec![
                Binding {
                    device: "D".into(),
                    led: "Backlight".into(),
                    conditions: vec![cond("DIM", OnWhen::Scale([0, 65535]))],
                    always: false,
                    any_of: Vec::new(),
                    pick: Pick::default(),
                    same_as: None,
                    same_as_device: None,
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
                    same_as_device: None,
                    on: None,
                    off: 255,
                    note: String::new(),
                },
            ],
            readouts: Vec::new(),
            disabled_devices: Vec::new(),
            follows: BTreeMap::new(),
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

    /// A mirror can name a lamp on another device, and follows it the same way.
    #[test]
    fn a_mirror_follows_a_lamp_on_another_device() {
        let led = lamp(LedKind::Dimmer, 255);
        let mut profile = mirror_profile(Some("Backlight"));
        profile.bindings[1].device = "E".into();
        profile.bindings[1].same_as_device = Some("D".into());
        let flag = &profile.bindings[1];

        assert_eq!(profile.resolve_binding(flag, &led, |_| Some(32768)), Some(127));
        assert_eq!(profile.sources_of(flag), vec!["DIM"]);
        // Without the device it looks on its own, which holds no `Backlight`.
        let mut local = profile.clone();
        local.bindings[1].same_as_device = None;
        assert!(local.sources_of(&local.bindings[1]).is_empty());
    }

    /// Pointing at a device that follows another reads the one it follows,
    /// whose rows are the ones in use.
    #[test]
    fn a_mirror_on_a_follower_reads_what_it_follows() {
        let led = lamp(LedKind::Dimmer, 255);
        let mut profile = mirror_profile(Some("Backlight"));
        profile.bindings[1].device = "E".into();
        profile.bindings[1].same_as_device = Some("F".into());
        profile.follows.insert("F".into(), "D".into());
        let flag = &profile.bindings[1];
        assert_eq!(profile.resolve_binding(flag, &led, |_| Some(65535)), Some(255));
        let running = profile.with_followers();
        let flag = running.bindings.iter().find(|b| b.device == "E").unwrap();
        assert_eq!(running.sources_of(flag), vec!["DIM"]);
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
