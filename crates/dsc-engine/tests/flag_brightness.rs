//! The PTO2's flag lamps have their own dimmer, and it has to be driven.
//!
//! Index 3 was missing from the vendor table we transcribed, so nothing wrote
//! it and it kept whatever SimAppPro last stored. On 2026-09-16 that cost a
//! full debugging session: the engine resolved the flap lamps correctly, the
//! device acked every write, and the lamps were invisible.
//!
//! Measured the same day, with all 14 indicators held at 1:
//!
//! * FLAG at 0 hides NOSE, LEFT, RIGHT, FLAPS, HALF, FULL and HOOK.
//! * CAUTION, JETT, CTR, LI, LO, RO and RI are unaffected by it.
//! * Raising FLAG to 255 brings all seven back with no rewrite of the lamps.
//!
//! These drive a fixture wired the way a knob-following profile is: every
//! backlight on the console dimmer, with the gates given a daylight floor.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dsc_bios::Write;
use dsc_config::{Catalogue, DeviceInventory, Module, Profile};
use dsc_engine::{Engine, ACFT_NAME_LEN};

const PTO2: &str = "TAKEOFF_PLANEL_2";
const FLAG: u8 = 3;
const BACKLIGHT: u8 = 0;
const CONSOLE: u16 = 108;
const MODULE: &str = "A-10C";
const AIRCRAFT: &str = "A-10C_2";
const SIGNAL: &str = "LCP_CONSOLE";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn devices() -> DeviceInventory {
    DeviceInventory::load(&root().join("data/devices.json")).expect("devices.json should parse")
}

fn profile() -> Profile {
    Profile::load(&root().join("crates/dsc-engine/tests/fixtures/console-gates.json"))
        .expect("console-gates.json should parse")
}

/// Only the console dimmer. Every other signal the profile reads is absent, so
/// those bindings resolve to nothing and leave their lamps alone, which is the
/// behaviour we want under test anyway.
fn catalogue() -> Catalogue {
    let json = format!(
        r#"{{
            "module": "{MODULE}",
            "aircraft": ["{AIRCRAFT}"],
            "signals": [
                {{
                    "id": "{SIGNAL}",
                    "control_type": "analog_dial",
                    "outputs": [{{ "address": {CONSOLE}, "mask": 65535, "shift": 0, "max_value": 65535 }}]
                }}
            ]
        }}"#
    );
    Catalogue::from_modules(vec![
        serde_json::from_str::<Module>(&json).expect("fixture module should parse"),
    ])
}

fn acft_name(name: &str) -> Vec<Write> {
    let mut bytes = name.as_bytes().to_vec();
    bytes.resize(ACFT_NAME_LEN as usize, 0);
    (0..ACFT_NAME_LEN / 2)
        .map(|i| {
            let b = i as usize * 2;
            Write {
                address: i * 2,
                value: u16::from_le_bytes([bytes[b], bytes[b + 1]]),
            }
        })
        .collect()
}

/// Settle the aircraft change, set the console, and report the panel.
fn panel_at(console: u16) -> Vec<(u8, u8)> {
    let mut engine = Engine::new(devices(), catalogue(), vec![profile()]);
    engine.set_connected(vec![PTO2.to_string()]);

    let mut now = Instant::now();
    engine.ingest(&acft_name(AIRCRAFT), now);
    now += Duration::from_millis(50);
    engine.ingest(
        &[Write {
            address: CONSOLE,
            value: console,
        }],
        now,
    );
    now += Duration::from_secs(1);

    let sweep = engine.tick(now);
    sweep
        .writes
        .iter()
        .filter(|w| w.id.device == PTO2)
        .map(|w| (w.id.index, w.value))
        .collect()
}

fn value(panel: &[(u8, u8)], index: u8) -> u8 {
    panel
        .iter()
        .find(|(i, _)| *i == index)
        .map(|(_, v)| *v)
        .unwrap_or_else(|| panic!("index {index} was not written by the sweep"))
}

#[test]
fn a_dark_console_drives_the_flag_lamps_full_bright() {
    // Console off means daylight, not "lamps off". A dim flag lamp in daylight
    // is the failure this whole test file exists for.
    let panel = panel_at(0);
    assert_eq!(value(&panel, FLAG), 255, "FLAG must be full bright with the console off");
}

#[test]
fn a_lit_console_dims_the_flag_lamps_with_it() {
    assert_eq!(value(&panel_at(65535), FLAG), 255);
    assert_eq!(value(&panel_at(32768), FLAG), 127);
}

#[test]
fn the_panel_labels_still_follow_the_console_all_the_way_down() {
    // Backlight is deliberately NOT given the daylight floor: labels unlit in
    // daylight is correct, and it is the one dimmer that governs no lamp.
    let panel = panel_at(0);
    assert_eq!(value(&panel, BACKLIGHT), 0, "panel labels follow the console down to zero");
}

