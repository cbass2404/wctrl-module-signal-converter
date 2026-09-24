//! Invariants the shipped `data/devices.json` has to hold.
//!
//! These check data, not code. The data is transcribed from captures and edited
//! by hand, which is exactly the kind of thing that drifts silently.

use std::collections::HashMap;
use std::path::Path;

use dsc_config::{DeviceInventory, DisplayCatalogue};

fn root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn inventory() -> DeviceInventory {
    DeviceInventory::load(&root().join("../../data/devices.json")).expect("devices.json loads")
}

#[test]
fn every_lamp_name_is_unique_within_its_device() {
    // A profile addresses a lamp by device and name, so a duplicate name makes
    // one of the two unreachable with no error anywhere. The CarrierAce UFC hit
    // this: the vendor calls a lamp INST_PNL_Backlight on both the UFC part and
    // the HUD part, and the merge that adds new rows to old profiles quietly
    // added one row where two were due.
    for device in &inventory().devices {
        let mut seen: HashMap<&str, u32> = HashMap::new();
        for (part, led) in device.leds() {
            if let Some(other) = seen.insert(&led.name, part.part_id) {
                panic!(
                    "{}: lamp {:?} is on both part {:#06x} and part {:#06x}; \
                     a profile cannot say which it means",
                    device.key, led.name, other, part.part_id
                );
            }
        }
    }
}

#[test]
fn every_declared_display_has_a_map() {
    let displays =
        DisplayCatalogue::load_dir(&root().join("../../data/displays")).expect("displays load");
    for device in &inventory().devices {
        for (_, key) in device.displays() {
            assert!(displays.get(key).is_some(), "{}: no display map named {key:?}", device.key);
        }
    }
}

#[test]
fn a_dimmer_and_an_indicator_are_told_apart_by_their_range() {
    // Not cosmetic: an indicator that is sent 255 acks and lights nothing on the
    // PTO2, which cost a debugging session. The kind is what stops us doing it.
    for device in &inventory().devices {
        for (_, led) in device.leds() {
            if led.is_dimmable() {
                assert_eq!(led.max_value(), 255, "{}: {} dims", device.key, led.name);
            } else {
                assert!(
                    led.max_value() <= 1,
                    "{}: {} is an indicator but claims a maximum of {}",
                    device.key,
                    led.name,
                    led.max_value()
                );
            }
        }
    }
}

#[test]
fn the_cdus_share_a_screen_and_nothing_else() {
    // The MCDU and the three PFPs have the same glass, so a page made for one
    // shows on all of them. Their lamps and keys differ, so one may follow
    // another of its own model under another seat name, never a different
    // model: a PFP taking the MCDU's lamp rows would write lamps it lacks.
    let inventory = inventory();
    let families = ["MCDU", "PFP3N", "PFP7", "PFP4"];
    let names = |family: &str| -> Vec<&dsc_config::DeviceSpec> {
        ["Captain", "CoPilot", "Observer"]
            .iter()
            .map(|seat| {
                let key = format!("{family}_{seat}");
                inventory.device(&key).unwrap_or_else(|| panic!("{key} is in devices.json"))
            })
            .collect()
    };
    for family in families {
        for a in names(family) {
            assert_eq!(a.part_with_display("MCDU").map(|_| ()), Some(()), "{} has the MCDU screen", a.key);
            for other in families {
                for b in names(other) {
                    assert_eq!(
                        a.same_hardware(b),
                        family == other,
                        "{} and {} should {}be variants",
                        a.key,
                        b.key,
                        if family == other { "" } else { "not " }
                    );
                }
            }
        }
    }
}
