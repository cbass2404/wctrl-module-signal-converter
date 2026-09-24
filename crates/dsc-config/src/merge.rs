//! Taking some of one profile's setup into another.
//!
//! An import is otherwise all or nothing: the profile arrives whole, as a
//! profile of its own. Often only part of it is wanted, a panel's lamps or a
//! few lines of a screen, and often the profile that wants them already
//! exists. The F-14 and F-14BU read one module and ship apart, so a change
//! worth having in both was made twice by hand.
//!
//! What can be taken is lamps, one at a time or a panel at once, and screen
//! fields, a line at a time. A line is a region of the display map, since that is the unit the
//! editor already offers for placing a field, and a field belongs to the
//! region its first cell is in. Only between profiles on one module, because
//! signals are named by id and an id means something only in its own
//! catalogue.
//!
//! A screen holds no fields of its own, only page slots, so it is merged a
//! slot at a time instead: slot n of the source replaces slot n of the target.
//! Bringing the page into the library, when it comes from a file, is the
//! caller's; see `bundle::bring_in`.
//!
//! Nothing else moves. The name, the aircraft, the font, disabled panels and
//! which panel follows which are the target's and stay so.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{CellRange, DeviceInventory, DisplayCatalogue, PageSlots, Profile, Readout};

/// Where a field sits when no region of its display holds its first cell.
pub const OTHER_CELLS: &str = "Other cells";

/// One lamp that can be taken.
#[derive(Debug, Clone, Serialize)]
pub struct LampPart {
    pub led: String,
    pub label: String,
}

/// A panel whose lamps can be taken.
#[derive(Debug, Clone, Serialize)]
pub struct LightPart {
    pub device: String,
    pub label: String,
    /// Lamps the source assigns on it, in the panel's order. An unassigned
    /// lamp is not offered.
    pub lamps: Vec<LampPart>,
}

/// One line of a screen whose fields can be taken.
#[derive(Debug, Clone, Serialize)]
pub struct LinePart {
    pub device: String,
    pub display: String,
    /// The panel and screen, for grouping lines under one heading.
    pub screen: String,
    pub line: String,
    pub fields: usize,
}

/// One filled page slot that can be taken.
#[derive(Debug, Clone, Serialize)]
pub struct SlotPart {
    pub device: String,
    /// The panel, the heading its slots are grouped under.
    pub screen: String,
    /// Counting from 1.
    pub slot: usize,
    /// The page's name, or its id where the page cannot be found. Empty for
    /// a blank slot.
    pub page: String,
    /// Whether the slot shows a blank screen rather than a page.
    pub blank: bool,
    /// Whether this is the slot the source starts on.
    pub start: bool,
}

/// Everything a profile has to offer another.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Parts {
    pub lights: Vec<LightPart>,
    pub lines: Vec<LinePart>,
    pub slots: Vec<SlotPart>,
}

/// A lamp picked for merging.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LampPick {
    pub device: String,
    pub led: String,
}

/// A line picked for merging.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LinePick {
    pub device: String,
    pub display: String,
    pub line: String,
}

/// A page slot picked for merging.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SlotPick {
    pub device: String,
    /// Counting from 1.
    pub slot: usize,
}

/// What the user ticked.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Pick {
    #[serde(default)]
    pub lights: Vec<LampPick>,
    #[serde(default)]
    pub lines: Vec<LinePick>,
    #[serde(default)]
    pub slots: Vec<SlotPick>,
}

/// What merging did to one panel's lamps or one screen line.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    /// The panel, or the panel and screen line.
    pub label: String,
    /// Lamps or fields the target did not have.
    pub added: usize,
    /// Lamps the target set up differently, now the source's.
    pub replaced: usize,
    /// Fields on the line the source does not have, gone.
    pub removed: usize,
    /// Already the same in both.
    pub unchanged: usize,
    /// Whether this counts fields rather than lamps.
    pub fields: bool,
    /// Whether this is a page slot rather than lamps or fields.
    pub pages: bool,
}

impl Change {
    pub fn changes_anything(&self) -> bool {
        self.added + self.replaced + self.removed > 0
    }
}

/// The target with the picked parts taken in, and what that did.
#[derive(Debug, Clone)]
pub struct Merged {
    pub profile: Profile,
    pub changes: Vec<Change>,
    /// Things that make a merge do less than it looks like it does: a panel
    /// the target follows another with, or has turned off.
    pub notes: Vec<String>,
}

/// The line a field is filed under: the first region of its display that
/// holds its first cell.
pub fn line_of(readout: &Readout, displays: &DisplayCatalogue) -> String {
    displays
        .get(&readout.display)
        .and_then(|d| {
            d.regions.iter().find(|r| {
                r.cells
                    .parse::<CellRange>()
                    .is_ok_and(|range| range.contains(readout.cells.first))
            })
        })
        .map(|r| r.name.clone())
        .unwrap_or_else(|| OTHER_CELLS.to_string())
}

fn device_label(devices: &DeviceInventory, device: &str) -> String {
    devices
        .device(device)
        .map(|d| d.display_name.clone())
        .unwrap_or_else(|| device.to_string())
}

/// A screen's heading: the panel's name, and the display's too when the panel
/// has more than one.
fn screen_label(devices: &DeviceInventory, device: &str, display: &str) -> String {
    let panel = device_label(devices, device);
    match devices.device(device) {
        Some(d) if d.displays().count() > 1 => format!("{panel} {display}"),
        _ => panel,
    }
}

/// Whether two rows would drive the panel alike, compared as they are written.
fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// What `source` could give another profile: every lamp it assigns, by panel,
/// and every screen line with a field on it.
///
/// A panel the source has following another is left out, since its own rows
/// are not what flies there.
pub fn parts(source: &Profile, devices: &DeviceInventory, displays: &DisplayCatalogue) -> Parts {
    let mut out = Parts::default();
    for spec in &devices.devices {
        if source.follows.contains_key(&spec.key) {
            continue;
        }
        let lamps: Vec<LampPart> = spec
            .leds()
            .filter(|(_, l)| {
                source
                    .bindings
                    .iter()
                    .any(|b| b.device == spec.key && b.led == l.name && !b.is_placeholder())
            })
            .map(|(_, l)| LampPart {
                led: l.name.clone(),
                label: if l.label.is_empty() { l.name.clone() } else { l.label.clone() },
            })
            .collect();
        if !lamps.is_empty() {
            out.lights.push(LightPart {
                device: spec.key.clone(),
                label: spec.display_name.clone(),
                lamps,
            });
        }
        for (_, display) in spec.displays() {
            // In region order, so the lines read top to bottom.
            let mut counts: BTreeMap<usize, (String, usize)> = BTreeMap::new();
            let order = |line: &str| {
                displays
                    .get(display)
                    .and_then(|d| d.regions.iter().position(|r| r.name == line))
                    .unwrap_or(usize::MAX)
            };
            for r in source.readouts.iter().filter(|r| r.device == spec.key && r.display == display) {
                let line = line_of(r, displays);
                counts.entry(order(&line)).or_insert_with(|| (line, 0)).1 += 1;
            }
            for (line, fields) in counts.into_values() {
                out.lines.push(LinePart {
                    device: spec.key.clone(),
                    display: display.to_string(),
                    screen: screen_label(devices, &spec.key, display),
                    line,
                    fields,
                });
            }
        }
    }
    out
}

/// Every filled page slot `source` has, by panel, named with `name_of`.
///
/// A panel the source has following another is left out, as for `parts`.
pub fn slot_parts(source: &Profile, devices: &DeviceInventory, name_of: impl Fn(&str) -> Option<String>) -> Vec<SlotPart> {
    let mut out = Vec::new();
    for spec in &devices.devices {
        if source.follows.contains_key(&spec.key) {
            continue;
        }
        let Some(slots) = source.screens.get(&spec.key) else { continue };
        for (i, slot) in slots.filled() {
            out.push(SlotPart {
                device: spec.key.clone(),
                screen: device_label(devices, &spec.key),
                slot: i + 1,
                page: slot.page.as_ref().map_or(String::new(), |id| name_of(id).unwrap_or_else(|| id.clone())),
                blank: slot.page.is_none(),
                start: slots.start == Some(i + 1),
            });
        }
    }
    out
}

/// `target` with the lamps and lines in `pick` taken from `source`.
///
/// A picked lamp takes the source's row in place of the target's; a lamp the
/// source leaves unassigned keeps the target's row, and so does every lamp not
/// picked. What happened is told a panel at a time.
///
/// A picked line becomes exactly the source's line: its fields arrive and the
/// target's other fields on it go, since two fields on one line would fight
/// over its cells.
///
/// Refused when the two read different modules. Whether the result would load
/// is not decided here; the caller checks it the way a save is checked.
pub fn merge(
    target: &Profile,
    source: &Profile,
    pick: &Pick,
    devices: &DeviceInventory,
    displays: &DisplayCatalogue,
) -> Result<Merged, String> {
    if source.module != target.module {
        return Err(format!(
            "{} reads {} and {} reads {}, so nothing in one means anything in the other",
            source.name, source.module, target.name, target.module
        ));
    }
    let mut profile = target.clone();
    let mut changes = Vec::new();
    let mut touched: Vec<&str> = Vec::new();

    let mut panels: Vec<&str> = Vec::new();
    for lamp in &pick.lights {
        if !panels.contains(&lamp.device.as_str()) {
            panels.push(&lamp.device);
        }
    }
    for device in panels {
        let mut change = Change {
            label: device_label(devices, device),
            added: 0,
            replaced: 0,
            removed: 0,
            unchanged: 0,
            fields: false,
            pages: false,
        };
        let picked = |b: &&crate::Binding| {
            b.device == device
                && !b.is_placeholder()
                && pick.lights.iter().any(|l| l.device == device && l.led == b.led)
        };
        for b in source.bindings.iter().filter(picked) {
            match profile.bindings.iter_mut().find(|t| t.device == b.device && t.led == b.led) {
                Some(t) if same(t, b) => change.unchanged += 1,
                Some(t) => {
                    *t = b.clone();
                    change.replaced += 1;
                }
                None => {
                    profile.bindings.push(b.clone());
                    change.added += 1;
                }
            }
        }
        touched.push(device);
        changes.push(change);
    }

    for line in &pick.lines {
        let on_line = |r: &Readout| {
            r.device == line.device && r.display == line.display && line_of(r, displays) == line.line
        };
        let incoming: Vec<&Readout> = source.readouts.iter().filter(|r| on_line(r)).collect();
        let mut change = Change {
            label: format!("{} {}", screen_label(devices, &line.device, &line.display), line.line),
            added: 0,
            replaced: 0,
            removed: 0,
            unchanged: 0,
            fields: true,
            pages: false,
        };
        // Where the line's first field was, so a merged line lands where the
        // old one sat in the file rather than at the end. Nothing before the
        // first field on the line is removed, so the index still holds after.
        let at = profile.readouts.iter().position(|r| on_line(r));
        profile.readouts.retain(|r| {
            if !on_line(r) {
                return true;
            }
            if incoming.iter().any(|i| same(*i, r)) {
                change.unchanged += 1;
            } else {
                change.removed += 1;
            }
            false
        });
        let at = at.unwrap_or(profile.readouts.len());
        change.added = incoming.len() - change.unchanged;
        for (n, r) in incoming.into_iter().enumerate() {
            profile.readouts.insert(at + n, r.clone());
        }
        touched.push(&line.device);
        changes.push(change);
    }

    for pick in &pick.slots {
        let i = pick.slot.saturating_sub(1);
        let incoming = source.screens.get(&pick.device).and_then(|s| s.slots.get(i)).cloned().flatten();
        let mut slots = profile.screens.get(&pick.device).cloned().unwrap_or_default();
        let count = devices.device(&pick.device).map_or(1, |d| d.slot_count());
        slots.slots.resize(count.max(slots.slots.len()).max(i + 1), None);
        let mut change = Change {
            label: format!("{} slot {}", device_label(devices, &pick.device), pick.slot),
            added: 0,
            replaced: 0,
            removed: 0,
            unchanged: 0,
            fields: false,
            pages: true,
        };
        match (&slots.slots[i], &incoming) {
            (a, b) if a == b => change.unchanged += 1,
            (None, Some(_)) => change.added += 1,
            (Some(_), Some(_)) => change.replaced += 1,
            (Some(_), None) => change.removed += 1,
            (None, None) => {}
        }
        slots.slots[i] = incoming;
        // The target's own start stands, unless the merge emptied it or it
        // had none; then the first filled slot starts.
        slots.settle_start();
        store(&mut profile, &pick.device, slots);
        touched.push(&pick.device);
        changes.push(change);
    }

    let mut notes = Vec::new();
    touched.sort();
    touched.dedup();
    for device in touched {
        let label = device_label(devices, device);
        if let Some(leader) = target.follows.get(device) {
            notes.push(format!(
                "{label} follows {} in {}, so what is merged onto it is kept but not used until it stops following.",
                device_label(devices, leader),
                target.name
            ));
        }
        if !target.drives(device) {
            notes.push(format!(
                "{label} is turned off in {}, so what is merged onto it does nothing until it is turned back on.",
                target.name
            ));
        }
    }
    Ok(Merged { profile, changes, notes })
}

/// Put a device's slots on `profile`, leaving no entry for six empty ones.
fn store(profile: &mut Profile, device: &str, slots: PageSlots) {
    if slots.filled().next().is_none() {
        profile.screens.remove(device);
    } else {
        profile.screens.insert(device.to_string(), slots);
    }
}
