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

use dsc_bios::{BiosState, Write};
use dsc_config::{
    Binding, Catalogue, DeviceInventory, Display, DisplayCatalogue, Module, Pick, Profile, Screen,
    Transport, SEAT_SIGNAL,
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

/// One write to a display's buffer.
///
/// On a segment display this is one group, four bytes on the UFC. On a pixel
/// display it is a run of consecutive changed rows, starting at row `group`,
/// and the caller commits once the batch's writes to that part are out. On a
/// text grid it is the whole screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LcdWrite {
    pub device: String,
    pub part_id: u32,
    pub transport: Transport,
    /// Which display, a key in `data/displays`.
    pub display: String,
    pub group: u8,
    /// Where `bytes` start in the display's buffer.
    pub offset: usize,
    pub bytes: Vec<u8>,
    /// The font a text grid has to hold for these bytes to read right, as the
    /// display names it. The caller uploads it when the panel does not already
    /// have it. `None` on a blank screen, which needs no glyphs, and on every
    /// other kind of display.
    pub font: Option<String>,
}

/// Turn changed groups into writes. A pixel screen takes a write of any length,
/// so consecutive rows go out as one, which is what SimAppPro does too.
fn lcd_writes(
    device: &str,
    part_id: u32,
    map: &Display,
    font: Option<&str>,
    groups: Vec<(u8, Vec<u8>)>,
) -> Vec<LcdWrite> {
    let mut out: Vec<LcdWrite> = Vec::new();
    for (group, bytes) in groups {
        let offset = usize::from(group) * map.group_bytes;
        if map.transport == Transport::Pixel {
            if let Some(last) = out.last_mut() {
                if last.offset + last.bytes.len() == offset {
                    last.bytes.extend_from_slice(&bytes);
                    continue;
                }
            }
        }
        out.push(LcdWrite {
            device: device.to_string(),
            part_id,
            transport: map.transport,
            display: map.key.clone(),
            group,
            offset,
            bytes,
            font: font.map(str::to_string),
        });
    }
    out
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
    /// One screen repainted with another of its page slots.
    PageSwap,
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

/// One signal a [`Pick::Latest`] binding reads, and when its value last changed.
///
/// Tracked per signal rather than per word: DCS-BIOS packs several controls
/// into one word, and a neighbour moving must not count as this knob moving.
#[derive(Debug, Clone)]
struct Move {
    address: u16,
    mask: u16,
    shift: u8,
    /// The value last seen. The first one is a baseline, not a movement.
    last: Option<u16>,
    /// When it last changed, on the engine's move clock. `None` until it has.
    at: Option<u64>,
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
    /// Signals read by the active profile's `pick: latest` bindings, by id.
    moves: HashMap<String, Move>,
    /// Counts movements, so "which moved last" is a comparison of numbers.
    move_clock: u64,
}

/// Profiles as they run, with every device that follows another given that
/// device's rows. The one place a follower is resolved, so the engine never
/// has to know one exists.
fn running(profiles: Vec<Profile>) -> Vec<Profile> {
    profiles.iter().map(Profile::with_followers).collect()
}

impl Engine {
    pub fn new(devices: DeviceInventory, catalogue: Catalogue, profiles: Vec<Profile>) -> Self {
        Engine {
            devices,
            catalogue,
            profiles: running(profiles),
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
            moves: HashMap::new(),
            move_clock: 0,
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
        // Which page each screen shows is kept across a save in the editor,
        // where the slot is still in use, so saving does not throw the pilot
        // back to the start page. A new aircraft starts on `start` regardless.
        let shown: Option<(String, Vec<(String, usize)>)> = self.active_profile().map(|p| {
            (p.name.clone(), p.page_runs.iter().map(|(d, r)| (d.clone(), r.shown)).collect())
        });
        self.profiles = running(profiles);

        let Some(aircraft) = self.aircraft.clone() else {
            // Nothing is loaded, so there is nothing to sweep. The new profiles
            // are picked up when an aircraft is next detected.
            self.active = None;
            self.by_address.clear();
            return Batch::empty(Cause::ProfileReload);
        };
        self.select_profile(&aircraft);
        if let (Some(i), Some((name, shown))) = (self.active, shown) {
            if self.profiles[i].name == name {
                for (device, slot) in shown {
                    self.profiles[i].show_slot(&device, slot);
                }
            }
        }

        // Still waiting for the post-load flood to settle. That sweep is coming
        // anyway and will use the profiles we just installed.
        if self.pending.is_some() {
            return Batch::empty(Cause::ProfileReload);
        }

        let mut writes = self.sweep();
        let (lamps, mut lcd) = self.paint();
        writes.extend(lamps);
        let (released, blanked) = self.release_undriven();
        writes.extend(released);
        lcd.extend(blanked);
        Batch {
            cause: Cause::ProfileReload,
            writes,
            lcd,
        }
    }

    /// Show another of a device's page slots (from 0), as its page key asks.
    ///
    /// The slot's fields take the place of the page's on that device, and the
    /// paint that follows sends only what changed, which is that one screen.
    /// `None` when nothing changes: no profile, no such slot, a disabled
    /// one, or the one already showing. Which key means which slot is the
    /// caller's business; the engine knows only slots.
    pub fn show_slot(&mut self, device: &str, slot: usize) -> Option<Batch> {
        let i = self.active?;
        if !self.profiles[i].show_slot(device, slot) {
            return None;
        }
        // The settle sweep still to come paints the new page with the rest.
        if self.pending.is_some() {
            return Some(Batch::empty(Cause::PageSwap));
        }
        let (writes, lcd) = self.paint();
        Some(Batch { cause: Cause::PageSwap, writes, lcd })
    }

    /// Swap in a rebuilt catalogue, with the profiles checked against it.
    ///
    /// For DCS-BIOS changing under a running daemon: every address may have
    /// moved, so the catalogue and the profiles resolved through it go
    /// together, and the panels are resynced exactly as for a profile reload.
    pub fn set_catalogue(&mut self, catalogue: Catalogue, profiles: Vec<Profile>) -> Batch {
        self.catalogue = catalogue;
        self.set_profiles(profiles)
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
        self.moves.clear();
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

        // Still loading: the flood re-sends every signal, and none of that is
        // anyone turning a knob, so it only sets baselines.
        self.note_moves(&touched, self.pending.is_none());

        if let Some(p) = self.pending.as_mut() {
            if !writes.is_empty() {
                p.last_traffic = now;
            }
            let quiet_for = now.saturating_duration_since(p.last_traffic);
            let waited = now.saturating_duration_since(p.since);
            if quiet_for >= self.settle_quiet || waited >= self.settle_max {
                self.pending = None;
                let mut writes = self.sweep();
                let (lamps, mut lcd) = self.paint();
                writes.extend(lamps);
                let (released, blanked) = self.release_undriven();
                writes.extend(released);
                lcd.extend(blanked);
                return Batch {
                    cause: Cause::ModuleLoad,
                    writes,
                    lcd,
                };
            }
            return Batch::empty(Cause::ModuleLoad);
        }

        // The lamps only revisit bindings whose signals moved, but the glass
        // is repainted whole: a display field is cheap to rebuild and a torn
        // one is worse than a late one.
        let mut writes = self.incremental(&touched);
        let (lamps, lcd) = self.paint();
        writes.extend(lamps);
        Batch {
            cause: Cause::SignalChange,
            writes,
            lcd,
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

    /// Attach the display maps. Panels without glass need none, so this is
    /// opt-in rather than a constructor argument.
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
    ///
    /// Also returns writes for the lamps that light a display: as bound while
    /// the profile puts fields on it, 0 while it does not. Blanking on the way out
    /// needs nothing extra, because `shutdown` zeroes every lamp we lit.
    fn paint(&mut self) -> (Vec<LedWrite>, Vec<LcdWrite>) {
        let Some(profile) = self.active.map(|i| &self.profiles[i]) else {
            return (Vec::new(), Vec::new());
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
        let mut lamps = Vec::new();
        for device in &self.devices.devices {
            if !self.connected.iter().any(|k| k == &device.key) || !profile.drives(&device.key) {
                continue;
            }
            for (part, key) in device.displays() {
                let Some(map) = self.displays.get(key) else {
                    continue;
                };
                // A text grid draws from the font the aircraft's own CDU
                // matches, and one with none has nothing to draw with. The
                // profile check says so; here the screen is blanked, so the
                // last aircraft's page does not stay up under this one.
                // The aircraft's own font where it has one, and the font the
                // profile picked where it does not. An aircraft with a CDU
                // keeps its own either way: the glyphs are drawn to match what
                // the module sends, and overriding that puts the wrong symbol
                // on the glass rather than a differently shaped right one.
                let font = map.text.as_ref().and_then(|t| {
                    self.aircraft
                        .as_deref()
                        .and_then(|a| t.font_with(a, profile.font.as_deref()))
                });
                let drawable = map.text.is_none() || font.is_some();
                let used = drawable
                    && profile
                        .readouts
                        .iter()
                        .any(|r| r.device == device.key && r.display == key);
                for led in part.leds.iter().filter(|l| l.lights_display) {
                    let id = LedId {
                        device: device.key.clone(),
                        part_id: part.part_id,
                        index: led.index,
                    };
                    // Bound like any lamp, but only while there is something
                    // to see. Unbound, or bound to a signal not yet arrived,
                    // it is full: a drawn page nobody can read helps no one.
                    let bound = || {
                        let b = profile
                            .bindings
                            .iter()
                            .find(|b| b.device == device.key && b.led == led.name)?;
                        let module = self.catalogue.module(&profile.module)?;
                        profile.resolve_binding_with_moves(
                            b,
                            led,
                            |source| {
                                let output = module.signal(source)?.primary()?;
                                self.state
                                    .value(output.address, output.mask.unwrap_or(u16::MAX), output.shift)
                                    .map(u32::from)
                            },
                            |source| self.moves.get(source)?.at,
                        )
                    };
                    let value = if used { bound().unwrap_or(led.max_value()) } else { 0 };
                    if self.shadow.get(&id) != Some(&value) {
                        self.shadow.insert(id.clone(), value);
                        lamps.push(LedWrite { id, value });
                    }
                }
                let mut next = Screen::new(map);
                for r in profile
                    .readouts
                    .iter()
                    .filter(|r| drawable && r.device == device.key && r.display == key)
                {
                    // A field bound to a seat paints only from that seat, and
                    // not at all until the seat is known. Guessing would put
                    // the other station's reading on the glass, which is worse
                    // than a dark cell because it looks right.
                    if let Some(want) = r.seat {
                        if seat != Some(want) {
                            continue;
                        }
                    }
                    // What the field draws right now, one glyph per cell,
                    // composed from its spans. How a reading turns into
                    // characters is policy and lives in the config crate; all
                    // that happens here is looking up what a signal says.
                    //
                    // None means every signal it reads is still to arrive, so
                    // the cells are left alone rather than written blank. A
                    // chain that has some of its signals draws what it has: a
                    // label belongs on the glass before the reading beside it.
                    let module = self.catalogue.module(&profile.module);
                    let state = &self.state;
                    let Some(glyphs) = r.compose(|id| {
                        let output = module?.signal(id)?.primary()?;
                        if output.r#type == "string" {
                            return state
                                .text(output.address, output.max_length.unwrap_or(0))
                                .map(dsc_config::Reading::Text);
                        }
                        let mask = output.mask.unwrap_or(u16::MAX);
                        state
                            .value(output.address, mask, output.shift)
                            .map(|value| dsc_config::Reading::Number {
                                value,
                                max: output.number_max(),
                            })
                    }) else {
                        continue;
                    };
                    for (offset, cell) in r.cells.cells().enumerate() {
                        let Some(glyph) = glyphs.get(offset) else { continue };
                        if map.transport == Transport::Text {
                            let ch = glyph.text.chars().next().unwrap_or(' ');
                            let colour = glyph.colour.unwrap_or_default();
                            let _ =
                                next.draw_text(map, cell, ch, colour, glyph.small, glyph.inverse);
                        } else {
                            let _ = next.draw_styled(map, cell, &glyph.text, glyph.inverse);
                        }
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
                out.extend(lcd_writes(&device.key, part.part_id, map, font, groups));
                self.screens.insert(id, next);
            }
        }
        (lamps, out)
    }

    /// Blank every display we have driven, for shutdown and mission end.
    fn blank_displays(&mut self) -> Vec<LcdWrite> {
        self.blank_screens(|_| true)
    }

    /// Take back what we left on the panels the active profile does not
    /// drive, after a sweep.
    ///
    /// A disabled device is left alone as far as anyone else's settings go,
    /// but what the last aircraft put on it is ours. Left latched, it shows
    /// that aircraft's lamps and page under this one. So every lamp we lit
    /// there goes to zero and every screen we drew is blanked, as `shutdown`
    /// does for the lot. A device we never wrote is still never touched.
    ///
    /// With no profile for this aircraft nothing is driven, and the same goes
    /// for every panel: the sweep has already darkened the ordinary lamps, and
    /// this takes the screens and the lamps that light them.
    fn release_undriven(&mut self) -> (Vec<LedWrite>, Vec<LcdWrite>) {
        let profile = self.active.map(|i| &self.profiles[i]);
        let undriven: HashSet<String> = self
            .shadow
            .keys()
            .map(|id| &id.device)
            .chain(self.screens.keys().map(|(d, _)| d))
            .filter(|d| !profile.is_some_and(|p| p.drives(d)))
            .cloned()
            .collect();
        if undriven.is_empty() {
            return (Vec::new(), Vec::new());
        }

        let mut ids: Vec<LedId> = self
            .shadow
            .iter()
            .filter(|(id, v)| **v != 0 && undriven.contains(&id.device))
            .map(|(id, _)| id.clone())
            .collect();
        ids.sort_by(|a, b| {
            (&a.device, a.part_id, a.index).cmp(&(&b.device, b.part_id, b.index))
        });
        let mut writes = Vec::with_capacity(ids.len());
        for id in ids {
            self.shadow.insert(id.clone(), 0);
            writes.push(LedWrite { id, value: 0 });
        }
        let lcd = self.blank_screens(|d| undriven.contains(d));
        (writes, lcd)
    }

    /// Blank the displays we have driven on the devices `which` picks, and
    /// forget them, so driving one again paints it in full.
    fn blank_screens(&mut self, which: impl Fn(&str) -> bool) -> Vec<LcdWrite> {
        let mut out = Vec::new();
        let mut ids: Vec<(String, String)> =
            self.screens.keys().filter(|(d, _)| which(d)).cloned().collect();
        ids.sort();
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
            out.extend(lcd_writes(&id.0, part.part_id, map, None, groups));
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
        // A knob in the last aircraft says nothing about this one.
        self.moves.clear();
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
        // Every screen starts on its start page. The profile kept whichever
        // page it showed when this aircraft last flew, which is not saved.
        self.profiles[i].reset_pages();
        let profile = &self.profiles[i];
        let Some(module) = self.catalogue.module(&profile.module) else {
            return;
        };

        let mut index: HashMap<u16, Vec<usize>> = HashMap::new();
        for (bi, b) in profile.bindings.iter().enumerate() {
            // A panel this profile does not drive is never written, so its
            // bindings are kept out of the index rather than filtered on every
            // write. The sweep and the paint already leave it alone; this is
            // the incremental path's half of the same promise.
            if !profile.drives(&b.device) {
                continue;
            }
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

        // Kept across a profile reload, so saving in the editor does not
        // forget which knob was turned last. Only what is still read is kept.
        let mut old = std::mem::take(&mut self.moves);
        for b in profile.bindings.iter().filter(|b| b.pick == Pick::Latest) {
            for source in profile.sources_of(b) {
                if self.moves.contains_key(source) {
                    continue;
                }
                let track = old.remove(source).or_else(|| {
                    let out = module.signal(source)?.primary()?;
                    let mask = out.mask.unwrap_or(u16::MAX);
                    Some(Move {
                        address: out.address,
                        mask,
                        shift: out.shift,
                        last: self.state.value(out.address, mask, out.shift),
                        at: None,
                    })
                });
                if let Some(track) = track {
                    self.moves.insert(source.to_string(), track);
                }
            }
        }
    }

    /// Bring every tracked signal up to date with the words that just moved.
    ///
    /// `record` false only takes baselines, which is what the module-load
    /// flood is.
    fn note_moves(&mut self, touched: &[u16], record: bool) {
        if self.moves.is_empty() || touched.is_empty() {
            return;
        }
        for track in self.moves.values_mut() {
            if !touched.contains(&track.address) {
                continue;
            }
            let now = self.state.value(track.address, track.mask, track.shift);
            if now == track.last {
                continue;
            }
            if record && track.last.is_some() {
                self.move_clock += 1;
                track.at = Some(self.move_clock);
            }
            track.last = now;
        }
    }

    /// Every LED on every connected device, in a stable order.
    /// Every LED this profile is willing to drive.
    ///
    /// A disabled device contributes nothing, so the sweep does not zero it.
    /// That is the whole difference between disabling a device and binding
    /// nothing on it: one is left alone, the other is deliberately darkened.
    /// Only what we lit on it ourselves is taken back, by `release_undriven`.
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
            // A screen's lamp is written with its screen, by `paint`.
            for (part, led) in dev.leds().filter(|(_, l)| !l.lights_display) {
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
                    resolve(
                        &self.devices,
                        module,
                        &self.state,
                        &self.moves,
                        profile,
                        &profile.bindings[bi],
                    )
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
            if let Some((id, v)) = resolve(&self.devices, module, &self.state, &self.moves, profile, b) {
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
    moves: &HashMap<String, Move>,
    profile: &Profile,
    b: &Binding,
) -> Option<(LedId, u8)> {
    let device = devices.device(&b.device)?;
    let (part, led) = device.led(&b.led)?;
    // A screen's lamp is written with its screen, by `paint`, which gates it
    // on whether anything is drawn there.
    if led.lights_display {
        return None;
    }

    // Through the profile rather than the binding alone, because a lamp may
    // mirror another one and needs its sibling to resolve itself.
    let value = profile.resolve_binding_with_moves(
        b,
        led,
        |source| {
            let output = module.signal(source)?.primary()?;
            state
                .value(output.address, output.mask.unwrap_or(u16::MAX), output.shift)
                .map(u32::from)
        },
        |source| moves.get(source)?.at,
    )?;

    Some((
        LedId {
            device: b.device.clone(),
            part_id: part.part_id,
            index: led.index,
        },
        value,
    ))
}
