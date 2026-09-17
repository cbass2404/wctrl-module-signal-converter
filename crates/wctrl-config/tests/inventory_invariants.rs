//! Invariants the shipped `data/devices.json` has to hold.
//!
//! These check data, not code. The data is transcribed from captures and edited
//! by hand, which is exactly the kind of thing that drifts silently.

use std::collections::HashMap;
use std::path::Path;

use wctrl_config::{DeviceInventory, DisplayCatalogue};

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
        for (part, key) in device.displays() {
            let map = displays
                .get(key)
                .unwrap_or_else(|| panic!("{}: no display map named {key:?}", device.key));
            assert_eq!(
                map.part_id, part.part_id,
                "{}: display {key:?} is declared on part {:#06x} but its map is \
                 addressed to {:#06x}; a write would go to the wrong part",
                device.key, part.part_id, map.part_id
            );
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
