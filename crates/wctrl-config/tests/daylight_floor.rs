//! A gate that follows the cockpit lighting down to 0 hides its lamps by day.
//!
//! SL gates every PTO2 indicator and FLAG gates the seven flag lamps. Tied to
//! a console knob with no `off`, either one goes to 0 whenever the knob is
//! down, which in daylight is always. The profile loads and does exactly what
//! it says, so this is a caution rather than a problem: the editor shows it
//! without withholding Save, and the daemon logs it without skipping the file.

use std::path::Path;

use wctrl_config::{DeviceInventory, Profile};

fn devices() -> DeviceInventory {
    DeviceInventory::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json"))
        .expect("devices")
}

fn profile(bindings: &str) -> Profile {
    serde_json::from_str(&format!(
        r#"{{"name": "T", "aircraft": ["TEST"], "module": "TEST", "bindings": [{bindings}]}}"#
    ))
    .expect("the fixture profile parses")
}

const SCALED: &str = r#""conditions": [{"source": "DIM", "on_when": {"scale": [0, 65535]}}]"#;

#[test]
fn a_gate_that_follows_the_console_to_zero_is_cautioned() {
    let p = profile(&format!(
        r#"{{"device": "TAKEOFF_PLANEL_2", "led": "SL", "off": 0, {SCALED}}}"#
    ));
    let cautions = p.cautions(&devices());
    assert_eq!(cautions.len(), 1, "{cautions:?}");
    assert!(cautions[0].contains("SL"), "{:?}", cautions[0]);
    assert!(cautions[0].contains("14 lamps"), "{:?}", cautions[0]);
}

#[test]
fn a_gate_with_a_daylight_floor_is_not() {
    let p = profile(&format!(
        r#"{{"device": "TAKEOFF_PLANEL_2", "led": "SL", "off": 255, {SCALED}}}"#
    ));
    assert!(p.cautions(&devices()).is_empty());
}

#[test]
fn a_mirror_is_judged_by_its_own_floor() {
    // The A-10C case: SL matched the backlight and carried no floor of its
    // own. FLAG, matching the same lamp, did, and stays quiet.
    let p = profile(&format!(
        r#"{{"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0, {SCALED}}},
           {{"device": "TAKEOFF_PLANEL_2", "led": "SL", "off": 0, "conditions": [], "same_as": "Backlight"}},
           {{"device": "TAKEOFF_PLANEL_2", "led": "FLAG", "off": 255, "conditions": [], "same_as": "Backlight"}}"#
    ));
    let cautions = p.cautions(&devices());
    assert_eq!(cautions.len(), 1, "{cautions:?}");
    assert!(cautions[0].starts_with("SL "), "{:?}", cautions[0]);
}

#[test]
fn a_backlight_may_go_dark() {
    // Unlit panel labels in daylight are correct. The backlight governs no
    // lamp, so following the console to 0 is exactly what it should do.
    let p = profile(&format!(
        r#"{{"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0, {SCALED}}}"#
    ));
    assert!(p.cautions(&devices()).is_empty());
}

#[test]
fn every_governed_lamp_exists() {
    // A misspelt name here would still count toward the number the caution
    // quotes, and quietly overstate what the gate hides.
    let devices = devices();
    for device in &devices.devices {
        for part in &device.parts {
            for led in &part.leds {
                for name in &led.governs {
                    assert!(
                        device.led(name).is_some(),
                        "{} {} governs {name}, which is not on the device",
                        device.key,
                        led.name
                    );
                }
            }
        }
    }
}
