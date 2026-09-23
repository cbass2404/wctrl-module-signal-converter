//! A page key puts another of a screen's slots on it, and only on it.
//!
//! The engine knows slots, not keys: the daemon turns a key into a slot from
//! the device's own `page_keys`. What is pinned here is what a slot does once
//! asked for: a page repaints its one screen, a disabled slot and the slot
//! already showing do nothing, a blank one takes the screen dark, a follower
//! swaps on its own, and a new aircraft load starts on `start`.
//! See docs/CONFIG.md "Swapping".

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, Page, PageLibrary, PageSlots, Profile, Slot};
use dsc_engine::{Batch, Cause, Engine};

const CAPTAIN: &str = "MCDU_Captain";
const COPILOT: &str = "MCDU_CoPilot";

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn text_at(address: u16, s: &str) -> Vec<BiosWrite> {
    let b = s.as_bytes();
    (0..b.len().div_ceil(2))
        .map(|i| BiosWrite {
            address: address + (i as u16) * 2,
            value: u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&0)) << 8),
        })
        .collect()
}

/// The Hornet fixture with its MCDU fields moved onto two pages: "All",
/// every field, and "One", the first field alone. The Captain's slots are
/// All, One, Blank, then three disabled, starting on All.
fn hornet(follow: bool) -> Profile {
    let mut p = Profile::load(&r("crates/dsc-engine/tests/fixtures/fa-18.json")).expect("hornet");
    let mut fields: Vec<_> = p.readouts.iter().filter(|f| f.display == "MCDU" && f.device == CAPTAIN).cloned().collect();
    assert!(fields.len() > 1, "the fixture has more than one field on the Captain's screen");
    for f in &mut fields {
        f.device.clear();
    }
    p.readouts.retain(|f| f.display != "MCDU");
    let page = |id: &str, fields: Vec<_>| Page { id: id.into(), name: id.into(), display: "MCDU".into(), fields };
    let lib = PageLibrary::of(&p.module.clone(), vec![page("All", fields.clone()), page("One", vec![fields[0].clone()])]);
    let mut slots = PageSlots::empty(6);
    slots.slots[0] = Some(Slot::new("All"));
    slots.slots[1] = Some(Slot::new("One"));
    slots.slots[2] = Some(Slot::blank());
    slots.start = Some(1);
    p.screens.insert(CAPTAIN.into(), slots);
    if follow {
        p.follows.insert(COPILOT.into(), CAPTAIN.into());
    }
    p.with_pages(&lib)
}

fn engine(p: Profile) -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![p]).with_displays(displays);
    e.set_connected(vec![CAPTAIN.into(), COPILOT.into()]);
    e
}

/// Load the Hornet and let the flood settle, so the screens are painted.
fn fly(e: &mut Engine, t0: Instant) {
    e.ingest(&text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0"), t0);
    e.tick(t0 + Duration::from_secs(5));
}

fn screens_of(batch: &Batch) -> Vec<&str> {
    let mut d: Vec<&str> = batch.lcd.iter().map(|w| w.device.as_str()).collect();
    d.dedup();
    d
}

fn shown(e: &Engine, device: &str) -> usize {
    e.active_profile().unwrap().page_runs[device].shown
}

#[test]
fn a_page_slot_repaints_its_one_screen() {
    let mut e = engine(hornet(true));
    fly(&mut e, Instant::now());
    assert_eq!(shown(&e, CAPTAIN), 0);

    let batch = e.show_slot(CAPTAIN, 1).expect("slot 2 holds a page");
    assert_eq!(batch.cause, Cause::PageSwap);
    assert!(!batch.lcd.is_empty(), "the fields that went are blanked");
    assert_eq!(screens_of(&batch), vec![CAPTAIN], "and nothing but the Captain's screen is sent");
    assert_eq!(shown(&e, CAPTAIN), 1);
}

#[test]
fn a_disabled_slot_and_the_one_showing_do_nothing() {
    let mut e = engine(hornet(false));
    fly(&mut e, Instant::now());
    assert!(e.show_slot(CAPTAIN, 0).is_none(), "slot 1 is already up");
    assert!(e.show_slot(CAPTAIN, 3).is_none(), "slot 4 is disabled");
    assert!(e.show_slot(CAPTAIN, 6).is_none(), "there is no slot 7");
    assert!(e.show_slot("PTO2", 0).is_none(), "a device with no slots");
    assert_eq!(shown(&e, CAPTAIN), 0, "the page shown stays");
}

#[test]
fn a_blank_slot_takes_the_screen_dark() {
    let mut e = engine(hornet(false));
    fly(&mut e, Instant::now());
    let batch = e.show_slot(CAPTAIN, 2).expect("slot 3 is blank");
    assert!(!batch.lcd.is_empty());
    assert!(
        e.active_profile().unwrap().readouts.iter().all(|f| f.device != CAPTAIN || f.display != "MCDU"),
        "nothing is left to draw on it"
    );
    // Its backlight goes with it, as on any screen with nothing to show.
    assert!(batch.writes.iter().any(|w| w.id.device == CAPTAIN && w.value == 0));
}

#[test]
fn a_follower_starts_alike_and_swaps_on_its_own() {
    let mut e = engine(hornet(true));
    fly(&mut e, Instant::now());
    assert_eq!(shown(&e, COPILOT), 0, "it starts on the Captain's start page");

    let batch = e.show_slot(COPILOT, 1).expect("the follower has the Captain's slots");
    assert_eq!(screens_of(&batch), vec![COPILOT]);
    assert_eq!(shown(&e, CAPTAIN), 0, "the Captain's screen is its own");
    assert_eq!(shown(&e, COPILOT), 1);
}

#[test]
fn a_saved_profile_keeps_the_page_and_a_new_flight_starts_over() {
    let mut e = engine(hornet(false));
    let t0 = Instant::now();
    fly(&mut e, t0);
    e.show_slot(CAPTAIN, 1).unwrap();

    // Saved in the editor while flying: the page stays up.
    e.set_profiles(vec![hornet(false)]);
    assert_eq!(shown(&e, CAPTAIN), 1);

    // The mission ends and the same aircraft is flown again: start over.
    e.mission_ended();
    fly(&mut e, t0 + Duration::from_secs(10));
    assert_eq!(shown(&e, CAPTAIN), 0);
}
