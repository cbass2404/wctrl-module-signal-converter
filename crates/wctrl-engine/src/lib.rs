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
use wctrl_config::{Binding, Catalogue, DeviceInventory, Module, Profile};

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    pub cause: Cause,
    pub writes: Vec<LedWrite>,
}

impl Batch {
    fn empty(cause: Cause) -> Self {
        Batch {
            cause,
            writes: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.writes.is_empty()
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
            pending: None,
            settle_quiet: DEFAULT_SETTLE_QUIET,
            settle_max: DEFAULT_SETTLE_MAX,
        }
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
        let mut touched: Vec<u16> = Vec::with_capacity(writes.len());
        for w in writes {
            self.state.apply(*w);
            touched.push(w.address);
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
                };
            }
            return Batch::empty(Cause::ModuleLoad);
        }

        Batch {
            cause: Cause::SignalChange,
            writes: self.incremental(&touched),
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
        Batch {
            cause: Cause::Shutdown,
            writes,
        }
    }

    // ------------------------------------------------------------- internals

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
    fn all_leds(&self) -> Vec<LedId> {
        let mut out = Vec::new();
        for key in &self.connected {
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
