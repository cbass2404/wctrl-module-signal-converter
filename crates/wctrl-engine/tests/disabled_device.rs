//! A panel a profile does not drive is never written, by any path.
//!
//! The ICP and UFC share a swing arm, so whichever is in use hides the other,
//! and a profile turns one off. Its lamps and fields stay in the profile, so
//! turning it back on restores them, which means the engine is the only thing
//! keeping them off the wire.

use std::path::Path;
use std::time::{Duration, Instant};

use wctrl_bios::Write as BiosWrite;
use wctrl_config::{Catalogue, DeviceInventory, DisplayCatalogue, Profile};
use wctrl_engine::{Batch, Engine};

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
    let mut profile = Profile::load(&r("data/defaults/fa-18c-hornet.json")).expect("hornet");
    if !drive_ufc {
        profile.disabled_devices.push(UFC.into());
    }
    let mut e = Engine::new(devices, cat, vec![profile]).with_displays(displays);
    e.set_connected(vec!["TAKEOFF_PLANEL_2".into(), UFC.into()]);
    e
}

/// Load the Hornet, then turn the instrument panel dimmer, which the shipped
/// profile binds to the UFC's panel backlight.
fn fly(e: &mut Engine) -> Vec<Batch> {
    let dimmer = e
        .catalogue()
        .module("FA-18C_hornet")
        .and_then(|m| m.signal("INST_PNL_DIMMER"))
        .and_then(|s| s.primary())
        .expect("INST_PNL_DIMMER")
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
