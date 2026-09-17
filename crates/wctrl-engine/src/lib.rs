//! Turns a DCS-BIOS export stream into LED writes.
//!
//! The engine is deliberately free of I/O. It consumes decoded [`Write`]s and
//! returns [`LedWrite`]s for a caller to put on the wire, which is what lets
//! the whole module-load sequence be tested without hardware or DCS.
//!
//! Two behaviours here are driven by measured hardware facts rather than
//! preference, and both are documented in `docs/PROTOCOL.md`:
//!
//! * **LED state latches in the device.** There is no host watchdog, so the
//!   engine writes only on change and must clear LEDs on the way out.
//! * **A governing dimmer is an ordinary LED.** Per-LED values latch beneath
//!   the governor, so raising `SL` or `Backlight` re-lights whatever was set
//!   underneath it without the engine rewriting anything.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use wctrl_bios::{BiosState, Write};
use wctrl_config::{
    Binding, Catalogue, DeviceInventory, DisplayCatalogue, Module, Profile, Screen,
    SEAT_SIGNAL,
};

pub mod learn;

pub use learn::{Change, Watcher};

/// `_ACFT_NAME` sits at the bottom of the address space and is 24 bytes wide.
/// DCS-BIOS writes it on every aircraft change, which is how we notice one.
pub const ACFT_NAME_ADDRESS: u16 = 0;
pub const ACFT_NAME_LEN: u16 = 24;

/// How long the stream must go quiet before a module-load sweep is considered
/// safe. DCS-BIOS re-floods the whole address space after `clearValues()`, and
/// sweeping mid-flood would latch values that are about to be superseded.
pub const DEFAULT_SETTLE_QUIET: Duration = Duration::from_millis(250);

/// Upper bound on waiting for that quiet. A busy cockpit may never go quiet;
/// a late sweep is better than none, and the incremental path corrects it.
pub const DEFAULT_SETTLE_MAX: Duration = Duration::from_millis(2500);

/// Identifies one physical LED. `part_id` is carried because a single USB
/// device can front several parts, so the index alone is ambiguous across them.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LedId {
    pub device: String,
    pub part_id: u32,
    pub index: u8,
}

/// One write to a segment display's buffer, four bytes at a time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LcdWrite {
    pub device: String,
    pub part_id: u32,
    pub group: u8,
    pub bytes: Vec<u8>,
}

/// One LED write for the caller to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedWrite {
    pub id: LedId,
    pub value: u8,
}

/// Why the engine emitted a batch. Useful for logging, and for tests that care
/// about the difference between a sweep and an incremental update.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// A full sweep after an aircraft change.
    ModuleLoad,
    /// Individual LEDs whose source values moved.
    SignalChange,
    /// Everything owned, driven to zero.
    Shutdown,
    /// A full sweep after the profiles were edited while running.
    ProfileReload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub cause: Cause,
    pub writes: Vec<LedWrite>,
    /// Display groups to send, if any panel has glass. Separate from `writes`
    /// because the two go out through different commands and only one of them
    /// is acknowledged.
    #[allow(clippy::struct_field_names)]
    pub lcd: Vec<LcdWrite>,
}

impl Batch {
    fn empty(cause: Cause) -> Self {
        Batch {
            cause,
            writes: Vec::new(),
            lcd: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.lcd.is_empty()
    }
}

/// A pending module-load sweep, waiting for the post-load flood to settle.
#[derive(Debug, Clone, Copy)]
struct Pending {
    since: Instant,
    last_traffic: Instant,
}

pub struct Engine {
    devices: DeviceInventory,
    catalogue: Catalogue,
    profiles: Vec<Profile>,
    /// Device keys physically present. Nothing is written for a device that is
    /// absent, and a shared profile may legitimately name hardware we lack.
    connected: Vec<String>,
    state: BiosState,
    aircraft: Option<String>,
    active: Option<usize>,
    /// Source address to indices into the active profile's bindings. Rebuilt on
    /// every profile change; the stream is far too hot to scan bindings per write.
    by_address: HashMap<u16, Vec<usize>>,
    /// Last value written per LED, so we only write on change.
    shadow: HashMap<LedId, u8>,
    /// Cell and glyph maps for panels with glass.
    displays: DisplayCatalogue,
    /// What we believe is on each display, keyed by device and display. The
    /// device cannot be asked, and a display write is never acknowledged, so
    /// this is the only record of it.
    screens: HashMap<(String, String), Screen>,
    pending: Option<Pending>,
    settle_quiet: Duration,
    settle_max: Duration,
}

impl Engine {
    pub fn new(devices: DeviceInventory, catalogue: Catalogue, profiles: Vec<Profile>) -> Self {
        Engine {
            devices,
            catalogue,
            profiles,
            connected: Vec::new(),
            state: BiosState::new(),
            aircraft: None,
            active: None,
            by_address: HashMap::new(),
            shadow: HashMap::new(),
            displays: DisplayCatalogue::default(),
            screens: HashMap::new(),
            pending: None,
            settle_quiet: DEFAULT_SETTLE_QUIET,
            settle_max: DEFAULT_SETTLE_MAX,
        }
    }

    /// Swap in a reloaded set of profiles and resync the panels.
    ///
    /// The signal state is deliberately kept. DCS-BIOS has already said what
    /// the cockpit looks like, and dropping it would blank the panel for a
    /// second and wait for the next re-export to fill it back in.
    ///
    /// A full sweep follows rather than incremental writes, because a binding
    /// may have changed for a lamp whose signals have not moved, and a lamp
    /// that just became unbound has to be driven off. Nothing else would
    /// notice either case.
    pub fn set_profiles(&mut self, profiles: Vec<Profile>) -> Batch {
        self.profiles = profiles;

        let Some(aircraft) = self.aircraft.clone() else {
            // Nothing is loaded, so there is nothing to sweep. The new profiles
            // are picked up when an aircraft is next detected.
            self.active = None;
            self.by_address.clear();
            return Batch::empty(Cause::ProfileReload);
        };
        self.select_profile(&aircraft);

        // Still waiting for the post-load flood to settle. That sweep is coming
        // anyway and will use the profiles we just installed.
        if self.pending.is_some() {
            return Batch::empty(Cause::ProfileReload);
        }

        Batch {
            cause: Cause::ProfileReload,
            writes: self.sweep(),
            lcd: self.paint(),
        }
    }

    /// The mission has ended, but DCS is still running.
    ///
    /// Clears the panels and forgets the cockpit entirely, so the next mission
    /// is detected and swept from scratch.
    ///
    /// Forgetting the aircraft is the part that is easy to leave out and hard
    /// to notice: `shutdown` alone would clear the lamps but leave the name
    /// set, so loading the *same* aircraft again would look like no change at
    /// all, no sweep would run, and the panels would simply stay dark.
    pub fn mission_ended(&mut self) -> Batch {
        let batch = self.shutdown();
        self.state.clear();
        self.aircraft = None;
        self.active = None;
        self.by_address.clear();
        self.pending = None;
        batch
    }

    /// Declare which devices are present. Call after HID enumeration.
    pub fn set_connected(&mut self, keys: Vec<String>) {
        self.connected = keys;
    }

    pub fn set_settle(&mut self, quiet: Duration, max: Duration) {
        self.settle_quiet = quiet;
        self.settle_max = max;
    }

    pub fn aircraft(&self) -> Option<&str> {
        self.aircraft.as_deref()
    }

    pub fn active_profile(&self) -> Option<&Profile> {
        self.active.map(|i| &self.profiles[i])
    }

    pub fn state(&self) -> &BiosState {
        &self.state
    }

    pub fn catalogue(&self) -> &Catalogue {
        &self.catalogue
    }

    pub fn devices(&self) -> &DeviceInventory {
        &self.devices
    }

    /// Feed decoded writes and get back whatever should go to the hardware.
    ///
    /// `now` is passed in rather than read from the clock so the settle window
    /// can be exercised deterministically in tests.
    pub fn ingest(&mut self, writes: &[Write], now: Instant) -> Batch {
        // Only addresses whose value moved. The whole map is re-exported on a
        // cycle, so without this every bound lamp would be re-resolved several
        // times a second with nothing happening in the cockpit. The output was
        // always the same either way, because the shadow suppresses a write
        // that would not change the lamp; this skips the work, not the write.
        let mut touched: Vec<u16> = Vec::new();
        for w in writes {
            if self.state.apply(*w) {
                touched.push(w.address);
            }
        }

        if self.detect_aircraft() {
            // Hold everything until the post-load flood settles.
            self.pending = Some(Pending {
                since: now,
                last_traffic: now,
            });
            return Batch::empty(Cause::ModuleLoad);
        }

        if let Some(p) = self.pending.as_mut() {
            if !writes.is_empty() {
                p.last_traffic = now;
            }
            let quiet_for = now.saturating_duration_since(p.last_traffic);
            let waited = now.saturating_duration_since(p.since);
            if quiet_for >= self.settle_quiet || waited >= self.settle_max {
                self.pending = None;
                return Batch {
                    cause: Cause::ModuleLoad,
                    writes: self.sweep(),
                    lcd: self.paint(),
                };
            }
            return Batch::empty(Cause::ModuleLoad);
        }

        // The lamps only revisit bindings whose signals moved, but the glass
        // is repainted whole: a display field is cheap to rebuild and a torn
        // one is worse than a late one.
        Batch {
            cause: Cause::SignalChange,
            writes: self.incremental(&touched),
            lcd: self.paint(),
        }
    }

    /// Nudge a pending sweep when no datagrams are arriving.
    ///
    /// Needed because the settle window is defined by *silence*, and a silent
    /// stream produces no `ingest` calls in which to notice it.
    pub fn tick(&mut self, now: Instant) -> Batch {
        if self.pending.is_none() {
            return Batch::empty(Cause::SignalChange);
        }
        self.ingest(&[], now)
    }

    /// Every LED *we lit* driven to zero. Call on exit and on mission end: the
    /// device latches, so a process that dies quietly leaves the panel frozen.
    ///
    /// Scoped to LEDs the engine actually wrote, not to every LED on the
    /// hardware. Anything we never drove belongs to whoever did, typically
    /// SimAppPro's stored backlight, and blanking it on exit would be us
    /// clobbering a setting we do not own.
    pub fn shutdown(&mut self) -> Batch {
        let mut ids: Vec<LedId> = self
            .shadow
            .iter()
            .filter(|(_, v)| **v != 0)
            .map(|(id, _)| id.clone())
            .collect();
        // HashMap order is not stable; callers and tests want a fixed sequence.
        ids.sort_by(|a, b| {
            (&a.device, a.part_id, a.index).cmp(&(&b.device, b.part_id, b.index))
        });

        let mut writes = Vec::with_capacity(ids.len());
        for id in ids {
            self.shadow.insert(id.clone(), 0);
            writes.push(LedWrite { id, value: 0 });
        }
        let lcd = self.blank_displays();
        Batch {
            cause: Cause::Shutdown,
            writes,
            lcd,
        }
    }

    // ------------------------------------------------------------- internals

    /// Attach the segment display maps. Panels without glass need none, so
    /// this is opt-in rather than a constructor argument.
    pub fn with_displays(mut self, displays: DisplayCatalogue) -> Self {
        self.displays = displays;
        self
    }

    /// Repaint every display from the active profile, and report the groups
    /// that moved.
    ///
    /// The whole screen is rebuilt rather than patched. A field spans several
    /// words and DCS-BIOS delivers them across separate writes, so a partly
    /// arrived field is a state that was never in the cockpit. Painting from
    /// scratch each time the engine emits a batch means the glass only ever
    /// shows a settled reading, and 36 cells is far too cheap to optimise.
    ///
    /// A value the glyph table cannot draw leaves its cell blank rather than
    /// failing the batch. A profile is user-authored and the stream is live:
    /// one unexpected character must not stop the other 35 cells updating.
    fn paint(&mut self) -> Vec<LcdWrite> {
        let Some(profile) = self.active.map(|i| &self.profiles[i]) else {
            return Vec::new();
        };

        // Which crew station the player is in, where the module says. Read once
        // per paint rather than per field, and left as None on a module that
        // does not report it, which is most of them.
        let seat = self
            .catalogue
            .module(&profile.module)
            .and_then(|m| m.signal(SEAT_SIGNAL))
            .and_then(|s| s.primary())
            .and_then(|o| {
                self.state
                    .value(o.address, o.mask.unwrap_or(u16::MAX), o.shift)
            })
            // Widened to match a readout's seat, which is sized like every
            // other value the catalogue reports rather than like a word.
            .map(u32::from);

        let mut out = Vec::new();
        for device in &self.devices.devices {
            if !self.connected.iter().any(|k| k == &device.key) || !profile.drives(&device.key) {
                continue;
            }
            for (part, key) in device.displays() {
                let Some(map) = self.displays.get(key) else {
                    continue;
                };
                let mut next = Screen::new(map);
                for r in profile.readouts.iter().filter(|r| {
                    r.device == device.key && r.display == key
                }) {
                    // A field bound to a seat paints only from that seat, and
                    // not at all until the seat is known. Guessing would put
                    // the other station's reading on the glass, which is worse
                    // than a dark cell because it looks right.
                    if let Some(want) = r.seat {
                        if seat != Some(want) {
                            continue;
                        }
                    }
                    let Some(signal) = self
                        .catalogue
                        .module(&profile.module)
                        .and_then(|m| m.signal(&r.source))
                    else {
                        continue;
                    };
                    let Some(output) = signal.primary() else {
                        continue;
                    };
                    let text = if output.r#type == "string" {
                        match self.state.text(output.address, output.max_length.unwrap_or(0)) {
                            Some(t) => t,
                            None => continue, // not arrived yet; leave it blank
                        }
                    } else {
                        let mask = output.mask.unwrap_or(u16::MAX);
                        match self.state.value(output.address, mask, output.shift) {
                            Some(v) => r.format_number(
                                v,
                                output.max_value.unwrap_or(u32::from(u16::MAX)).min(u32::from(u16::MAX)) as u16,
                            ),
                            None => continue,
                        }
                    };
                    for (offset, cell) in r.cells.cells().enumerate() {
                        let value = r.lay_out(&text);
                        let Some(glyph) = value.get(offset) else { continue };
                        let _ = next.draw(map, cell, r.alias(glyph));
                    }
                }

                let id = (device.key.clone(), key.to_string());
                // A display we have not driven before is painted in full. Its
                // buffer latches, so whatever a previous run or SimAppPro left
                // on it is still there, and diffing against a blank we never
                // sent would leave that showing.
                let groups = match self.screens.get(&id) {
                    Some(previous) => next.changes_from(previous),
                    None => next.all_groups(),
                };
                for (group, bytes) in groups {
                    out.push(LcdWrite {
                        device: device.key.clone(),
                        part_id: part.part_id,
                        group,
                        bytes,
                    });
                }
                self.screens.insert(id, next);
            }
        }
        out
    }

    /// Blank every display we have driven, for shutdown and mission end.
    fn blank_displays(&mut self) -> Vec<LcdWrite> {
        let mut out = Vec::new();
        let ids: Vec<(String, String)> = self.screens.keys().cloned().collect();
        for id in ids {
            let Some(map) = self.displays.get(&id.1) else {
                continue;
            };
            let Some(part) = self
                .devices
                .device(&id.0)
                .and_then(|d| d.part_with_display(&id.1))
            else {
                continue;
            };
            let blank = Screen::new(map);
            let previous = self.screens.remove(&id);
            let groups = match previous {
                Some(p) => blank.changes_from(&p),
                None => blank.all_groups(),
            };
            for (group, bytes) in groups {
                out.push(LcdWrite {
                    device: id.0.clone(),
                    part_id: part.part_id,
                    group,
                    bytes,
                });
            }
        }
        out
    }

    /// Returns true when the aircraft name changed, which is the trigger for a
    /// module-load sweep.
    fn detect_aircraft(&mut self) -> bool {
        let raw = match self.state.string(ACFT_NAME_ADDRESS, ACFT_NAME_LEN) {
            Some(s) => s,
            None => return false,
        };
        let name = raw.trim_matches('\0').trim().to_string();
        if name.is_empty() || self.aircraft.as_deref() == Some(name.as_str()) {
            return false;
        }
        self.aircraft = Some(name.clone());
        self.select_profile(&name);
        true
    }

    fn select_profile(&mut self, aircraft: &str) {
        self.active = self
            .profiles
            .iter()
            .position(|p| p.aircraft.iter().any(|a| a == aircraft));
        self.by_address.clear();

        let Some(i) = self.active else { return };
        let profile = &self.profiles[i];
        let Some(module) = self.catalogue.module(&profile.module) else {
            return;
        };

        let mut index: HashMap<u16, Vec<usize>> = HashMap::new();
        for (bi, b) in profile.bindings.iter().enumerate() {
            // A binding is re-evaluated when *any* signal it reads moves, so it
            // is indexed under every address it reads, wherever in the binding
            // that signal is named. Rows that read nothing appear nowhere: a
            // placeholder is swept off like any unbound lamp, and an always-on
            // lamp is written by the sweep and never needs revisiting.
            for source in profile.sources_of(b) {
                if let Some(addr) = module
                    .signal(source)
                    .and_then(|s| s.primary())
                    .map(|o| o.address)
                {
                    let slot = index.entry(addr).or_insert_with(Vec::new);
                    // Two conditions on one address must not queue the binding
                    // twice.
                    if !slot.contains(&bi) {
                        slot.push(bi);
                    }
                }
            }
        }
        self.by_address = index;
    }

    /// Every LED on every connected device, in a stable order.
    /// Every LED this profile is willing to drive.
    ///
    /// A disabled device contributes nothing, so the sweep does not zero it.
    /// That is the whole difference between disabling a device and binding
    /// nothing on it: one is left alone, the other is deliberately darkened.
    fn all_leds(&self) -> Vec<LedId> {
        let mut out = Vec::new();
        let profile = self.active.map(|i| &self.profiles[i]);
        for key in &self.connected {
            if profile.is_some_and(|p| !p.drives(key)) {
                continue;
            }
            let Some(dev) = self.devices.device(key) else {
                continue;
            };
            for (part, led) in dev.leds() {
                out.push(LedId {
                    device: key.clone(),
                    part_id: part.part_id,
                    index: led.index,
                });
            }
        }
        out
    }

    /// One pass over every LED: bound ones take their resolved value, unbound
    /// ones take zero. Deliberately *not* a reset followed by a sync, which
    /// would write every LED twice and visibly flash the panel on every load.
    fn sweep(&mut self) -> Vec<LedWrite> {
        let bound = self.resolve_all();
        let mut writes = Vec::new();
        for id in self.all_leds() {
            let value = bound.get(&id).copied().unwrap_or(0);
            self.shadow.insert(id.clone(), value);
            writes.push(LedWrite { id, value });
        }
        writes
    }

    fn incremental(&mut self, touched: &[u16]) -> Vec<LedWrite> {
        let Some(active) = self.active else {
            return Vec::new();
        };
        if touched.is_empty() {
            return Vec::new();
        }

        let mut hit: Vec<usize> = Vec::new();
        let mut seen = HashSet::new();
        for addr in touched {
            for &bi in self.by_address.get(addr).into_iter().flatten() {
                if seen.insert(bi) {
                    hit.push(bi);
                }
            }
        }
        if hit.is_empty() {
            return Vec::new();
        }

        let resolved: Vec<(LedId, u8)> = {
            let profile = &self.profiles[active];
            let Some(module) = self.catalogue.module(&profile.module) else {
                return Vec::new();
            };
            hit.iter()
                .filter_map(|&bi| {
                    resolve(&self.devices, module, &self.state, profile, &profile.bindings[bi])
                })
                .collect()
        };

        let mut writes = Vec::new();
        for (id, value) in resolved {
            if self.shadow.get(&id).copied() == Some(value) {
                continue;
            }
            self.shadow.insert(id.clone(), value);
            writes.push(LedWrite { id, value });
        }
        writes
    }

    fn resolve_all(&self) -> HashMap<LedId, u8> {
        let mut out = HashMap::new();
        let Some(i) = self.active else { return out };
        let profile = &self.profiles[i];
        let Some(module) = self.catalogue.module(&profile.module) else {
            return out;
        };
        for b in &profile.bindings {
            if let Some((id, v)) = resolve(&self.devices, module, &self.state, profile, b) {
                out.insert(id, v);
            }
        }
        out
    }
}

/// Resolve one binding against current state.
///
/// Returns `None` when any signal it reads has not been seen yet, which is
/// normal early in a mission. An unresolved binding leaves the LED at its swept
/// value rather than forcing it to zero, so lamps do not flicker as the
/// post-load flood arrives.
fn resolve(
    devices: &DeviceInventory,
    module: &Module,
    state: &BiosState,
    profile: &Profile,
    b: &Binding,
) -> Option<(LedId, u8)> {
    let device = devices.device(&b.device)?;
    let (part, led) = device.led(&b.led)?;

    // Through the profile rather than the binding alone, because a lamp may
    // mirror another one and needs its sibling to resolve itself.
    let value = profile.resolve_binding(b, led, |source| {
        let output = module.signal(source)?.primary()?;
        state
            .value(output.address, output.mask.unwrap_or(u16::MAX), output.shift)
            .map(u32::from)
    })?;

    Some((
        LedId {
            device: b.device.clone(),
            part_id: part.part_id,
            index: led.index,
        },
        value,
    ))
}
