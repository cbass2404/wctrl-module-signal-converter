//! A device that follows another is driven exactly as that one is.
//!
//! WinWing sells the MCDU as Captain, Co-Pilot and Observer, one PID each, and
//! the hardware is the same. A profile points one at another rather than
//! writing every lamp and field three times, so the engine is what has to put
//! the same thing on both.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, Error, Profile};
use dsc_engine::{Batch, Engine};

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

fn hornet() -> Profile {
    Profile::load(&r("data/defaults/fa-18.json")).expect("hornet")
}

fn fly(profile: Profile) -> Vec<Batch> {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let mut e = Engine::new(devices, cat, vec![profile]).with_displays(displays);
    e.set_connected(vec![CAPTAIN.into(), COPILOT.into()]);
    let t0 = Instant::now();
    let writes = text_at(0, "FA-18C_hornet\0\0\0\0\0\0\0\0\0\0\0");
    vec![e.ingest(&writes, t0), e.tick(t0 + Duration::from_secs(5))]
}

/// What one device was sent, with the device name taken off so two can be
/// compared.
fn sent(batches: &[Batch], device: &str) -> (Vec<(u8, u8)>, Vec<(String, u8, usize, Vec<u8>)>) {
    let mut leds: Vec<_> = batches
        .iter()
        .flat_map(|b| b.writes.iter())
        .filter(|w| w.id.device == device)
        .map(|w| (w.id.index, w.value))
        .collect();
    let mut lcd: Vec<_> = batches
        .iter()
        .flat_map(|b| b.lcd.iter())
        .filter(|w| w.device == device)
        .map(|w| (w.display.clone(), w.group, w.offset, w.bytes.clone()))
        .collect();
    leds.sort();
    lcd.sort();
    (leds, lcd)
}

#[test]
fn a_follower_is_sent_what_it_follows() {
    let mut p = hornet();
    p.follows.insert(COPILOT.into(), CAPTAIN.into());
    let batches = fly(p);
    let captain = sent(&batches, CAPTAIN);
    assert!(!captain.1.is_empty(), "the Hornet puts fields on the Captain's screen");
    assert_eq!(captain, sent(&batches, COPILOT));
}

#[test]
fn without_following_the_two_differ() {
    // The control: the shipped Hornet puts its fields on the Captain only, so
    // the test above is not passing because the two were already the same.
    let batches = fly(hornet());
    assert_ne!(sent(&batches, CAPTAIN).1, sent(&batches, COPILOT).1);
}

fn problems(p: &Profile) -> Vec<Error> {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let cat = Catalogue::load_dir(&r("data/catalogue")).expect("catalogue");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let module = cat.module(&p.module).expect("the module");
    p.problems(module, &devices, &displays)
}

#[test]
fn only_the_same_hardware_can_be_followed() {
    let mut p = hornet();
    p.follows.insert(COPILOT.into(), "TAKEOFF_PLANEL_2".into());
    assert!(
        problems(&p).iter().any(|e| matches!(e, Error::FollowsDifferentHardware(..))),
        "an MCDU cannot take a PTO2's lamps"
    );
}

#[test]
fn a_follower_cannot_be_followed() {
    let mut p = hornet();
    p.follows.insert(COPILOT.into(), CAPTAIN.into());
    p.follows.insert("MCDU_Observer".into(), COPILOT.into());
    assert!(problems(&p).iter().any(|e| matches!(e, Error::FollowChain(..))));
}

#[test]
fn the_mfds_are_one_device_under_three_names() {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let d = |k: &str| devices.device(k).expect(k);
    assert!(d("CarrierAce_MFD_L").same_hardware(d("CarrierAce_MFD_R")));
    assert!(d("CarrierAce_MFD_L").same_hardware(d("CarrierAce_MFD_C")));
    assert!(d(CAPTAIN).same_hardware(d("MCDU_Observer")));
    assert!(!d(CAPTAIN).same_hardware(d("CarrierAce_MFD_L")));
}
