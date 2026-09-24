//! A panel a profile does not drive is never written, by any path, beyond
//! taking back what an earlier aircraft left on it.
//!
//! The ICP and UFC share a swing arm, so whichever is in use hides the other,
//! and a profile turns one off. Its lamps and fields stay in the profile, so
//! turning it back on restores them, which means the engine is the only thing
//! keeping them off the wire.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, Profile};
use dsc_engine::{Batch, Engine};

const UFC: &str = "CarrierAce_UFC";

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

fn engine(drive_ufc: bool) -> Engine {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut profile = Profile::load(&r("crates/dsc-engine/tests/fixtures/fa-18.json")).expect("hornet");
    if !drive_ufc {
        profile.disabled_devices.push(UFC.into());
    }
    let mut e = Engine::new(devices, cat, vec![profile]).with_displays(displays);
    e.set_connected(vec!["TAKEOFF_PLANEL_2".into(), UFC.into()]);
    e
}

/// Load the Hornet, then turn the consoles dimmer, which the shipped profile
/// binds to every backlight, the UFC's included.
fn fly(e: &mut Engine) -> Vec<Batch> {
    let dimmer = e
        .catalogue()
        .module("FA-18C_hornet")
        .and_then(|m| m.signal("CONSOLES_DIMMER"))
        .and_then(|s| s.primary())
        .expect("CONSOLES_DIMMER")
        .address;
    let t0 = Instant::now();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    writes.push(BiosWrite { address: dimmer, value: 10_000 });
    writes.extend(text_at(29746, "GRCV"));
    let mut out = vec![e.ingest(&writes, t0)];
    out.push(e.tick(t0 + Duration::from_secs(5)));
    let later = t0 + Duration::from_secs(6);
    out.push(e.ingest(&[BiosWrite { address: dimmer, value: 50_000 }], later));
    out.push(e.mission_ended());
    out
}

fn touches_ufc(batches: &[Batch]) -> bool {
    batches.iter().any(|b| {
        b.writes.iter().any(|w| w.id.device == UFC) || b.lcd.iter().any(|w| w.device == UFC)
    })
}

#[test]
fn a_driven_ufc_follows_the_dimmer() {
    // The control: the same flight with the UFC driven does write to it, so
    // the test below is not passing merely because nothing happened.
    let batches = fly(&mut engine(true));
    assert!(touches_ufc(&batches));
    assert!(
        batches[2].writes.iter().any(|w| w.id.device == UFC),
        "turning the dimmer mid-flight rewrites the UFC backlight"
    );
}

#[test]
fn a_disabled_ufc_is_never_written() {
    let batches = fly(&mut engine(false));
    assert!(
        !touches_ufc(&batches),
        "nothing may reach a panel the profile does not drive: {batches:?}"
    );
}

/// The Hornet's rows under another aircraft's name, with the UFC turned off:
/// the Viper's case, where the ICP swings in and the UFC swings out.
fn other() -> Profile {
    let mut p = Profile::load(&r("crates/dsc-engine/tests/fixtures/fa-18.json")).expect("hornet");
    p.name = "Other".into();
    p.aircraft = vec!["OTHER".into()];
    p.disabled_devices.push(UFC.into());
    p
}

fn ufc_lamps_lit(batch: &Batch) -> bool {
    batch.writes.iter().any(|w| w.id.device == UFC && w.value != 0)
}

#[test]
fn switching_to_an_aircraft_without_the_ufc_takes_back_what_the_last_one_lit() {
    let mut e = engine(true);
    let hornet = Profile::load(&r("crates/dsc-engine/tests/fixtures/fa-18.json")).expect("hornet");
    e.set_profiles(vec![hornet, other()]);
    let t0 = Instant::now();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(29746, "GRCV"));
    e.ingest(&writes, t0);
    let hornet = e.tick(t0 + Duration::from_secs(5));
    assert!(ufc_lamps_lit(&hornet) && hornet.lcd.iter().any(|w| w.device == UFC));

    let t1 = t0 + Duration::from_secs(10);
    e.ingest(&text_at(0, "OTHER\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0"), t1);
    let swap = e.tick(t1 + Duration::from_secs(5));
    let ufc: Vec<_> = swap.writes.iter().filter(|w| w.id.device == UFC).collect();
    assert!(!ufc.is_empty(), "the UFC's lamps are taken back: {swap:?}");
    assert!(ufc.iter().all(|w| w.value == 0), "and only ever to zero: {ufc:?}");
    assert!(
        swap.lcd.iter().any(|w| w.device == UFC && w.bytes.iter().all(|b| *b == 0)),
        "the Hornet's page is wiped off the glass"
    );

    // Taken back once, then left alone like any disabled panel.
    let later = e.ingest(&text_at(29746, "SQCH"), t1 + Duration::from_secs(6));
    assert!(!touches_ufc(&[later]));
}

#[test]
fn disabling_the_ufc_in_the_editor_takes_back_what_it_showed() {
    let mut e = engine(true);
    let batches = fly_without_ending(&mut e);
    assert!(touches_ufc(&batches));

    let mut hornet = e.active_profile().cloned().expect("hornet");
    hornet.disabled_devices.push(UFC.into());
    let reload = e.set_profiles(vec![hornet]);
    assert!(reload.writes.iter().any(|w| w.id.device == UFC));
    assert!(!ufc_lamps_lit(&reload), "{reload:?}");
    assert!(reload.lcd.iter().any(|w| w.device == UFC));
}

fn fly_without_ending(e: &mut Engine) -> Vec<Batch> {
    let t0 = Instant::now();
    let mut writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(29746, "GRCV"));
    vec![e.ingest(&writes, t0), e.tick(t0 + Duration::from_secs(5))]
}
