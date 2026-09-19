//! Building the signal catalogue from the DCS-BIOS installed on this machine.
//!
//! DCS-BIOS ships a JSON description of every control in every module it
//! supports. Each readable control carries an address, mask and shift, which is
//! exactly what decoding the export stream needs, so the catalogue is a
//! straight transform of those files with no Lua evaluation.
//!
//! It is rebuilt rather than shipped because DCS-BIOS allocates addresses in
//! the order controls are defined: one control added to a module moves every
//! later address in it. A catalogue from another release reads the wrong
//! addresses without any error, so it has to come from the release the user
//! actually has, and be built again whenever that changes.
//!
//! [`ensure`] is what both the daemon and the editor call at startup. It
//! compares the installed version, and a [`stamp`] of the files, with what the
//! catalogue was built from and rebuilds only when either differs, so
//! whichever starts second finds the work done. A lock keeps two builds from running at once, and a build goes
//! into a folder of its own that is renamed into place, so nothing ever reads
//! half a catalogue.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::ser::SerializeMap;
use serde::{Deserialize, Serialize};
use serde_json::ser::{Formatter, PrettyFormatter};
use serde_json::Value;

use crate::{Error, Result};

/// Files in `doc/json` that are not aircraft: they describe the stream itself
/// or are fragments shared between modules.
const NON_MODULE: [&str; 5] = ["AircraftAliases", "MetadataStart", "MetadataEnd", "CommonData", "NS430"];

/// The shared fragment that carries DCS-BIOS's own version, and the signal in
/// it. Recorded in the index so the daemon can ask the running DCS-BIOS which
/// release it is, rather than trusting the files on disk.
const COMMON_DATA: &str = "CommonData";
const VERSION_SIGNAL: &str = "VERSION";

/// A signal with no more than this many distinct values is offered as a
/// labelled dropdown; anything wider becomes a numeric range input.
const DISCRETE_LIMIT: u64 = 8;

/// What the version reads as when `BIOSConfig.lua` cannot be found or parsed.
pub const UNKNOWN_VERSION: &str = "unknown";

/// How long to wait for another process's build before giving up. A build
/// takes well under a second, so this is only reached if something is wrong.
const LOCK_WAIT: Duration = Duration::from_secs(20);

/// A lock older than this was left by a process that died mid-build.
const LOCK_STALE: Duration = Duration::from_secs(60);

/// Where DCS-BIOS puts its module descriptions in a default install.
pub fn default_bios_json() -> PathBuf {
    let home = std::env::var_os("USERPROFILE").map(PathBuf::from).unwrap_or_default();
    home.join("Saved Games")
        .join("DCS")
        .join("Scripts")
        .join("DCS-BIOS")
        .join("doc")
        .join("json")
}

/// The `doc/json` folder to build from: the one given, else the one the
/// current catalogue was built from, else the default install.
///
/// Remembering the last source means a user with DCS-BIOS somewhere unusual
/// says so once, on the first build, and never again.
pub fn locate_bios_json(catalogue: &Path, given: Option<&Path>) -> PathBuf {
    if let Some(dir) = given {
        return dir.to_path_buf();
    }
    if let Some(source) = read_index(catalogue).and_then(|i| i.source) {
        let source = PathBuf::from(source);
        if source.is_dir() {
            return source;
        }
    }
    default_bios_json()
}

/// The installed DCS-BIOS version, from `BIOSConfig.lua` two levels above
/// `doc/json`. `None` when the file is missing or has no version in it.
pub fn installed_version(bios_json: &Path) -> Option<String> {
    let config = bios_json.join("..").join("..").join("BIOSConfig.lua");
    let text = fs::read_to_string(config).ok()?;
    parse_version(&text)
}

/// The first `version = "..."` in a Lua file.
fn parse_version(text: &str) -> Option<String> {
    for (at, _) in text.match_indices("version") {
        let rest = text[at + "version".len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('"') else { continue };
        let Some(end) = rest.find('"') else { continue };
        if end > 0 {
            return Some(rest[..end].to_string());
        }
    }
    None
}

/// The version a catalogue was built from, from its `index.json`.
pub fn catalogue_version(catalogue: &Path) -> Option<String> {
    read_index(catalogue).and_then(|i| i.bios_version)
}

/// Where the running DCS-BIOS reports its own version in the export stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionSignal {
    pub address: u16,
    pub max_length: u16,
}

/// The parts of `index.json` read back. The module list is not needed here.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Index {
    #[serde(default)]
    pub bios_version: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub version_signal: Option<VersionSignal>,
    #[serde(default)]
    pub stamp: Option<String>,
}

pub(crate) fn read_index(catalogue: &Path) -> Option<Index> {
    let text = fs::read_to_string(catalogue.join("index.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// A cheap record of the `doc/json` files: every name, size and modified time.
///
/// The version alone is not enough. A build that ran while an install was
/// still copying files in read the new `BIOSConfig.lua` beside the old module
/// files, and recorded the new version over the old contents; a nightly can
/// also change its files without changing its version. Either way the version
/// matched and nothing ever rebuilt it. Any copy changes sizes and times, so
/// the next start sees the difference. Reading them costs a directory listing.
///
/// A build takes it before reading anything, so files that change while it
/// runs leave a stamp that no longer matches, and the next start builds again.
pub fn stamp(bios_json: &Path) -> Option<String> {
    let mut files: Vec<(String, u64, u128)> = fs::read_dir(bios_json)
        .ok()?
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".json") {
                return None;
            }
            let meta = e.metadata().ok()?;
            let at = meta.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?;
            Some((name, meta.len(), at.as_nanos()))
        })
        .collect();
    files.sort();
    // FNV-1a, because the standard hasher is free to change between Rust
    // releases, and a new compiler should not mean a rebuild.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, len, at) in &files {
        let bytes = name.bytes().chain([0]).chain(len.to_le_bytes()).chain(at.to_le_bytes());
        for b in bytes {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    Some(format!("{hash:016x}"))
}

/// What [`ensure`] found and did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Freshness {
    /// The catalogue was built from the installed version. Nothing was done.
    Current { version: String },
    /// The catalogue was missing, built from another version, or built from
    /// files that have changed since, and has been rebuilt. `was` is `None`
    /// when there was no catalogue, and equal to `now` when only the files
    /// changed.
    Built { was: Option<String>, now: String, modules: usize },
    /// No DCS-BIOS at `bios_json`. `have_catalogue` says whether an old one is
    /// still there to fall back on.
    NoBios { bios_json: PathBuf, have_catalogue: bool },
}

impl std::fmt::Display for Freshness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Freshness::Current { version } => write!(f, "catalogue matches DCS-BIOS {version}"),
            Freshness::Built { was: None, now, modules } => {
                write!(f, "catalogue built from DCS-BIOS {now}: {modules} modules")
            }
            Freshness::Built { was: Some(was), now, modules } if was == now => write!(
                f,
                "catalogue rebuilt: the DCS-BIOS {now} files changed since the last build: {modules} modules"
            ),
            Freshness::Built { was: Some(was), now, modules } => write!(
                f,
                "catalogue rebuilt for DCS-BIOS {now} (was {was}): {modules} modules"
            ),
            Freshness::NoBios { bios_json, have_catalogue: true } => write!(
                f,
                "DCS-BIOS not found at {}; using the catalogue already built, which may not match what DCS runs",
                bios_json.display()
            ),
            Freshness::NoBios { bios_json, have_catalogue: false } => write!(
                f,
                "DCS-BIOS not found at {}, and there is no catalogue. Install DCS-BIOS, then start again.",
                bios_json.display()
            ),
        }
    }
}

/// Make the catalogue at `out` match the DCS-BIOS at `bios_json`, rebuilding
/// it only if the version or the files differ.
pub fn ensure(bios_json: &Path, out: &Path) -> Result<Freshness> {
    if !bios_json.is_dir() {
        return Ok(Freshness::NoBios {
            bios_json: bios_json.to_path_buf(),
            have_catalogue: out.join("index.json").is_file(),
        });
    }
    let installed = installed_version(bios_json).unwrap_or_else(|| UNKNOWN_VERSION.to_string());
    let files = stamp(bios_json);
    if is_current(out, &installed, files.as_deref()) {
        return Ok(Freshness::Current { version: installed });
    }

    let _lock = BuildLock::take(out)?;
    // Asked again under the lock: the other app may have been building this
    // very version while we waited, and then there is nothing left to do.
    if is_current(out, &installed, files.as_deref()) {
        return Ok(Freshness::Current { version: installed });
    }
    let was = catalogue_version(out);
    let modules = build_and_swap(bios_json, out, &installed)?;
    Ok(Freshness::Built { was, now: installed, modules })
}

/// Built from `installed`, from files still as they were, and by a builder
/// that recorded where the stream reports its version. A catalogue from the
/// old Python builder has the right version and neither record, so it is
/// rebuilt once to gain them.
fn is_current(out: &Path, installed: &str, files: Option<&str>) -> bool {
    read_index(out).is_some_and(|i| {
        i.bios_version.as_deref() == Some(installed)
            && i.version_signal.is_some()
            && files.is_some()
            && i.stamp.as_deref() == files
    })
}

/// Rebuild whatever the versions say, for when the user asks.
pub fn rebuild(bios_json: &Path, out: &Path) -> Result<Freshness> {
    if !bios_json.is_dir() {
        return Err(Error::NoBios(bios_json.to_path_buf()));
    }
    let installed = installed_version(bios_json).unwrap_or_else(|| UNKNOWN_VERSION.to_string());
    let _lock = BuildLock::take(out)?;
    let was = catalogue_version(out);
    let modules = build_and_swap(bios_json, out, &installed)?;
    Ok(Freshness::Built { was, now: installed, modules })
}

fn build_and_swap(bios_json: &Path, out: &Path, version: &str) -> Result<usize> {
    let fresh = sibling(out, "building");
    if fresh.exists() {
        fs::remove_dir_all(&fresh)?;
    }
    let modules = match build(bios_json, &fresh, version) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_dir_all(&fresh);
            return Err(e);
        }
    };
    swap(&fresh, out)?;
    Ok(modules)
}

/// `data/catalogue` to `data/catalogue.<suffix>`, beside it so a rename is a
/// move within one folder.
fn sibling(out: &Path, suffix: &str) -> PathBuf {
    let name = out
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "catalogue".to_string());
    out.with_file_name(format!("{name}.{suffix}"))
}

/// Put a finished build where the old catalogue was.
///
/// Two renames rather than a copy, so the only moment the catalogue is absent
/// is between them. Retried because Windows refuses to rename a folder while
/// another process has a file in it open, and a reader holds one only briefly.
fn swap(fresh: &Path, out: &Path) -> Result<()> {
    let old = sibling(out, "old");
    if old.exists() {
        fs::remove_dir_all(&old)?;
    }
    if out.exists() {
        retry(|| fs::rename(out, &old))?;
    }
    if let Err(e) = retry(|| fs::rename(fresh, out)) {
        if old.exists() {
            let _ = fs::rename(&old, out);
        }
        return Err(e.into());
    }
    let _ = fs::remove_dir_all(&old);
    Ok(())
}

fn retry(mut op: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match op() {
            Ok(()) => return Ok(()),
            Err(_) if tries < 40 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Held while building. Removed on drop, including when the build fails.
struct BuildLock(PathBuf);

impl BuildLock {
    fn take(out: &Path) -> Result<Self> {
        let path = sibling(out, "lock");
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        let deadline = Instant::now() + LOCK_WAIT;
        loop {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    let _ = write!(file, "{}", std::process::id());
                    return Ok(BuildLock(path));
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    if is_stale(&path) {
                        let _ = fs::remove_file(&path);
                        continue;
                    }
                    if Instant::now() >= deadline {
                        return Err(Error::CatalogueBusy(path));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn is_stale(path: &Path) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| SystemTime::now().duration_since(t).ok())
        .is_some_and(|age| age > LOCK_STALE)
}

// ------------------------------------------------------------------- build

/// Write a catalogue for the DCS-BIOS at `bios_json` into `out`, returning the
/// number of modules written. `out` is created if missing.
pub fn build(bios_json: &Path, out: &Path, version: &str) -> Result<usize> {
    // Before anything is read; see `stamp`.
    let files = stamp(bios_json);
    let aliases = read_value(&bios_json.join("AircraftAliases.json"))?;
    let mut names: Vec<String> = fs::read_dir(bios_json)?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    names.sort();

    fs::create_dir_all(out)?;
    let mut index: Vec<(String, IndexEntry)> = Vec::new();
    for name in &names {
        let key = &name[..name.len() - ".json".len()];
        if NON_MODULE.contains(&key) {
            continue;
        }
        let raw = read_value(&bios_json.join(name))?;
        let signals = convert_module(&raw);
        if signals.is_empty() {
            continue;
        }
        let mut aircraft = aircraft_for(&aliases, key);
        aircraft.sort();
        let lamp_count = signals
            .iter()
            .filter(|s| s.control_type.as_str() == Some("led"))
            .count();
        let record = Record {
            module: key,
            bios_version: version,
            aircraft: &aircraft,
            signal_count: signals.len(),
            lamp_count,
            signals: &signals,
        };
        write_json(&out.join(name), &record)?;
        index.push((
            key.to_string(),
            IndexEntry {
                aircraft,
                signals: signals.len(),
                lamps: lamp_count,
            },
        ));
    }

    let source = std::path::absolute(bios_json).unwrap_or_else(|_| bios_json.to_path_buf());
    let common = bios_json.join(format!("{COMMON_DATA}.json"));
    let version_signal = read_value(&common).ok().and_then(|raw| version_signal(&raw));
    let modules = index.len();
    write_json(
        &out.join("index.json"),
        &IndexOut {
            bios_version: version,
            source: &source.to_string_lossy(),
            modules: &index,
            version_signal,
            stamp: files.as_deref(),
        },
    )?;
    Ok(modules)
}

fn read_value(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str(&text).map_err(|e| Error::Json(e, path.display().to_string()))
}

/// Every runtime aircraft name `AircraftAliases.json` maps onto `key`, once
/// for each time it does.
fn aircraft_for(aliases: &Value, key: &str) -> Vec<String> {
    let Some(map) = aliases.as_object() else { return Vec::new() };
    let mut out = Vec::new();
    for (aircraft, modules) in map {
        if aircraft.is_empty() {
            continue;
        }
        let hits = modules
            .as_array()
            .map_or(0, |m| m.iter().filter(|m| m.as_str() == Some(key)).count());
        out.extend(std::iter::repeat_n(aircraft.clone(), hits));
    }
    out
}

/// Where `CommonData` puts the `VERSION` string.
fn version_signal(raw: &Value) -> Option<VersionSignal> {
    for category in raw.as_object()?.values() {
        let Some(control) = category.get(VERSION_SIGNAL) else { continue };
        let out = control.get("outputs")?.as_array()?.first()?;
        return Some(VersionSignal {
            address: u16::try_from(out.get("address")?.as_u64()?).ok()?,
            max_length: u16::try_from(out.get("max_length")?.as_u64()?).ok()?,
        });
    }
    None
}

#[derive(Serialize)]
struct Record<'a> {
    module: &'a str,
    bios_version: &'a str,
    aircraft: &'a [String],
    signal_count: usize,
    lamp_count: usize,
    signals: &'a [SignalOut],
}

#[derive(Serialize)]
struct IndexEntry {
    aircraft: Vec<String>,
    signals: usize,
    lamps: usize,
}

struct IndexOut<'a> {
    bios_version: &'a str,
    source: &'a str,
    /// In file order, which is not the order a sorted map would give: a
    /// `.json` suffix sorts differently from the bare key.
    modules: &'a [(String, IndexEntry)],
    version_signal: Option<VersionSignal>,
    stamp: Option<&'a str>,
}

struct Modules<'a>(&'a [(String, IndexEntry)]);

impl Serialize for Modules<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(Some(self.0.len()))?;
        for (key, entry) in self.0 {
            map.serialize_entry(key, entry)?;
        }
        map.end()
    }
}

impl Serialize for IndexOut<'_> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        let mut map = s.serialize_map(None)?;
        map.serialize_entry("bios_version", self.bios_version)?;
        map.serialize_entry("source", self.source)?;
        map.serialize_entry("modules", &Modules(self.modules))?;
        if let Some(signal) = &self.version_signal {
            map.serialize_entry("version_signal", signal)?;
        }
        if let Some(stamp) = self.stamp {
            map.serialize_entry("stamp", stamp)?;
        }
        map.end()
    }
}

#[derive(Serialize)]
struct SignalOut {
    id: String,
    category: String,
    description: Value,
    control_type: Value,
    outputs: Vec<OutputOut>,
}

#[derive(Serialize)]
struct OutputOut {
    address: Value,
    mask: Value,
    shift: Value,
    max_value: Value,
    #[serde(rename = "type")]
    kind: Value,
    description: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_length: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    values: Option<Vec<LabelOut>>,
    discrete: bool,
}

#[derive(Serialize)]
struct LabelOut {
    value: u64,
    label: String,
}

/// One DCS-BIOS module file to our flat signal list, sorted by category then
/// id. Input-only controls, with nothing to read, are left out.
fn convert_module(raw: &Value) -> Vec<SignalOut> {
    let mut signals = Vec::new();
    let Some(categories) = raw.as_object() else { return signals };
    for (category, controls) in categories {
        let Some(controls) = controls.as_object() else { continue };
        for (id, control) in controls {
            let mut outputs = Vec::new();
            let raw_outputs = control.get("outputs").and_then(Value::as_array);
            for out in raw_outputs.into_iter().flatten() {
                let Some(address) = out.get("address") else { continue };
                let values = value_labels(control, out);
                outputs.push(OutputOut {
                    address: address.clone(),
                    mask: out.get("mask").cloned().unwrap_or(Value::Null),
                    shift: out.get("shift_by").cloned().unwrap_or(Value::from(0)),
                    max_value: out.get("max_value").cloned().unwrap_or(Value::Null),
                    kind: out.get("type").cloned().unwrap_or_else(|| Value::from("integer")),
                    description: out.get("description").cloned().unwrap_or_else(|| Value::from("")),
                    // String outputs are sized by max_length, not max_value.
                    // Without it the decoder cannot tell how many bytes to
                    // read, so every display signal would be unreadable.
                    max_length: out.get("max_length").filter(|v| !v.is_null()).cloned(),
                    discrete: values.is_some(),
                    values,
                });
            }
            if outputs.is_empty() {
                continue;
            }
            let category = match control.get("category") {
                Some(Value::String(c)) => c.clone(),
                Some(other) => other.to_string(),
                None => category.clone(),
            };
            signals.push(SignalOut {
                id: id.clone(),
                category,
                description: control.get("description").cloned().unwrap_or_else(|| Value::from("")),
                control_type: control.get("control_type").cloned().unwrap_or_else(|| Value::from("")),
                outputs,
            });
        }
    }
    signals.sort_by(|a, b| (&a.category, &a.id).cmp(&(&b.category, &b.id)));
    signals
}

/// The value and label of every position of a discrete signal, or `None` for
/// one that is continuous. The editor limits a condition to exactly these, so
/// a user can never pick something the signal cannot report.
fn value_labels(control: &Value, out: &Value) -> Option<Vec<LabelOut>> {
    if out.get("type").and_then(Value::as_str) != Some("integer") {
        return None;
    }
    let max_value = out.get("max_value")?.as_i64()?;
    if max_value < 1 || max_value as u64 + 1 > DISCRETE_LIMIT {
        return None;
    }
    let max_value = max_value as u64;

    // `positions` is authoritative when present: index == reported value.
    let positions = control
        .get("positions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let inline = inline_labels(out.get("description").and_then(Value::as_str).unwrap_or(""));

    let mut values = Vec::new();
    for value in 0..=max_value {
        let mut label = positions.get(value as usize).and_then(position_label);
        // The last label given for a value wins, as it would in a dict.
        if let Some((_, text)) = inline.iter().rev().find(|(v, _)| *v == value) {
            label = Some(match label {
                Some(l) => format!("{l} ({text})"),
                None => text.clone(),
            });
        }
        let label = label.filter(|l| !l.is_empty()).unwrap_or_else(|| value.to_string());
        values.push(LabelOut { value, label });
    }
    Some(values)
}

/// A position's name, when it has one worth showing.
fn position_label(p: &Value) -> Option<String> {
    match p {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) if n.as_f64() != Some(0.0) => Some(n.to_string()),
        Value::Bool(true) => Some("True".to_string()),
        _ => None,
    }
}

/// Labels written into a description, as in
/// `"switch position -- 0 = Down, 1 = Mid,  2 = Up"`: each run of digits,
/// `=`, and the text up to the next comma or semicolon, trimmed.
fn inline_labels(text: &str) -> Vec<(u64, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match inline_label_at(&chars, i) {
            Some((value, label, end)) => {
                if let Some(value) = value {
                    found.push((value, label));
                }
                i = end;
            }
            None => i += 1,
        }
    }
    found
}

/// One `digits = text` starting exactly at `start`, with where it ends. The
/// value is `None` when the digits overflow, which no discrete signal reaches.
fn inline_label_at(chars: &[char], start: usize) -> Option<(Option<u64>, String, usize)> {
    let mut i = start;
    while i < chars.len() && chars[i].is_ascii_digit() {
        i += 1;
    }
    if i == start {
        return None;
    }
    let value = chars[start..i].iter().collect::<String>().parse::<u64>().ok();
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if chars.get(i) != Some(&'=') {
        return None;
    }
    i += 1;
    let spaces = i;
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    let text_start = i;
    while i < chars.len() && chars[i] != ',' && chars[i] != ';' {
        i += 1;
    }
    if i > text_start {
        let text: String = chars[text_start..i].iter().collect();
        return Some((value, text.trim().to_string(), i));
    }
    // Nothing after the spaces: the last space is the text, as a backtracking
    // regex would take it, and it trims to nothing.
    if text_start > spaces {
        return Some((value, String::new(), text_start));
    }
    None
}

/// Pretty printed one space deep with every non-ASCII character escaped, so
/// the files match what the Python builder wrote byte for byte.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut bytes = Vec::new();
    let formatter = AsciiPretty(PrettyFormatter::with_indent(b" "));
    let mut ser = serde_json::Serializer::with_formatter(&mut bytes, formatter);
    value
        .serialize(&mut ser)
        .map_err(|e| Error::Json(e, path.display().to_string()))?;
    fs::write(path, bytes)?;
    Ok(())
}

struct AsciiPretty<'a>(PrettyFormatter<'a>);

impl Formatter for AsciiPretty<'_> {
    fn begin_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_array(w)
    }
    fn end_array<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array(w)
    }
    fn begin_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.0.begin_array_value(w, first)
    }
    fn end_array_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_array_value(w)
    }
    fn begin_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object(w)
    }
    fn end_object<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object(w)
    }
    fn begin_object_key<W: ?Sized + io::Write>(&mut self, w: &mut W, first: bool) -> io::Result<()> {
        self.0.begin_object_key(w, first)
    }
    fn begin_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.begin_object_value(w)
    }
    fn end_object_value<W: ?Sized + io::Write>(&mut self, w: &mut W) -> io::Result<()> {
        self.0.end_object_value(w)
    }
    fn write_string_fragment<W: ?Sized + io::Write>(&mut self, w: &mut W, fragment: &str) -> io::Result<()> {
        if fragment.bytes().all(|b| b < 0x7f) {
            return w.write_all(fragment.as_bytes());
        }
        let mut units = [0u16; 2];
        for c in fragment.chars() {
            if (c as u32) < 0x7f {
                w.write_all(&[c as u8])?;
            } else {
                for unit in c.encode_utf16(&mut units) {
                    write!(w, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_version_comes_from_the_assignment_not_a_comment() {
        let lua = "--- @field version string the current dcs-bios version\n\
                   BIOSConfig = {\n\tversion = \"2026.09.18-nightly\", -- set automatically\n}";
        assert_eq!(parse_version(lua).as_deref(), Some("2026.09.18-nightly"));
        assert_eq!(parse_version("nothing here"), None);
    }

    #[test]
    fn inline_labels_read_like_the_description_says() {
        let got = inline_labels("switch position -- 0 = Down, 1 = Mid,  2 = Up");
        assert_eq!(got, vec![(0, "Down".into()), (1, "Mid".into()), (2, "Up".into())]);
        // A value with nothing after it still counts, as nothing.
        assert_eq!(inline_labels("0 = , 1 = On"), vec![(0, String::new()), (1, "On".into())]);
        // Digits with no `=` are not a label.
        assert!(inline_labels("rated 28 V; 400 Hz").is_empty());
    }

    #[test]
    fn positions_and_inline_labels_combine() {
        let control = serde_json::json!({"positions": ["OFF", "", "ON"]});
        let out = serde_json::json!({
            "type": "integer", "max_value": 2, "description": "0 = Safe, 1 = Mid"
        });
        let labels: Vec<String> = value_labels(&control, &out)
            .unwrap()
            .into_iter()
            .map(|l| l.label)
            .collect();
        assert_eq!(labels, ["OFF (Safe)", "Mid", "ON"]);
    }

    #[test]
    fn wide_or_textual_outputs_are_not_discrete() {
        let control = serde_json::json!({});
        let wide = serde_json::json!({"type": "integer", "max_value": 65535});
        let text = serde_json::json!({"type": "string", "max_length": 6});
        assert!(value_labels(&control, &wide).is_none());
        assert!(value_labels(&control, &text).is_none());
    }

    #[test]
    fn non_ascii_is_escaped_the_way_python_writes_it() {
        let dir = std::env::temp_dir().join(format!("wctrl-ascii-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.json");
        write_json(&path, &serde_json::json!({"a": "\u{b0}\u{bb}\u{1f600}", "b": []})).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        let _ = fs::remove_dir_all(&dir);
        assert_eq!(text, "{\n \"a\": \"\\u00b0\\u00bb\\ud83d\\ude00\",\n \"b\": []\n}");
    }
}
