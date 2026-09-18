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
//! These tests load the shipped profiles rather than a fixture, because the
//! thing worth protecting is the binding an owner actually flies with.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use wctrl_bios::Write;
use wctrl_config::{Catalogue, DeviceInventory, Module, Profile};
use wctrl_engine::{Engine, ACFT_NAME_LEN};

const PTO2: &str = "TAKEOFF_PLANEL_2";
const FLAG: u8 = 3;
const BACKLIGHT: u8 = 0;
const SL: u8 = 2;
const CONSOLE: u16 = 108;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn devices() -> DeviceInventory {
    DeviceInventory::load(&root().join("data/devices.json")).expect("devices.json should parse")
}

fn profile(file: &str) -> Profile {
    Profile::load(&root().join("data/defaults").join(file))
        .unwrap_or_else(|e| panic!("{file} should parse: {e}"))
}

/// Only the console dimmer. Every other signal the profile reads is absent, so
/// those bindings resolve to nothing and leave their lamps alone, which is the
/// behaviour we want under test anyway.
fn catalogue(module: &str, aircraft: &str, signal: &str) -> Catalogue {
    let json = format!(
        r#"{{
            "module": "{module}",
            "aircraft": ["{aircraft}"],
            "signals": [
                {{
                    "id": "{signal}",
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
fn panel_at(file: &str, module: &str, aircraft: &str, signal: &str, console: u16) -> Vec<(u8, u8)> {
    let mut engine = Engine::new(devices(), catalogue(module, aircraft, signal), vec![profile(file)]);
    engine.set_connected(vec![PTO2.to_string()]);

    let mut now = Instant::now();
    engine.ingest(&acft_name(aircraft), now);
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
    for file in ["a-10c-2.json", "fa-18c-hornet.json"] {
        let (module, aircraft, signal) = spec(file);
        let panel = panel_at(file, module, aircraft, signal, 0);
        assert_eq!(
            value(&panel, FLAG),
            255,
            "{file}: FLAG must be full bright with the console off"
        );
    }
}

#[test]
fn a_lit_console_dims_the_flag_lamps_with_it() {
    for file in ["a-10c-2.json", "fa-18c-hornet.json"] {
        let (module, aircraft, signal) = spec(file);
        assert_eq!(value(&panel_at(file, module, aircraft, signal, 65535), FLAG), 255);
        assert_eq!(value(&panel_at(file, module, aircraft, signal, 32768), FLAG), 127);
    }
}

#[test]
fn the_panel_labels_still_follow_the_console_all_the_way_down() {
    // Backlight is deliberately NOT given the daylight floor: labels unlit in
    // daylight is correct, and it is the one dimmer that governs no lamp.
    for file in ["a-10c-2.json", "fa-18c-hornet.json"] {
        let (module, aircraft, signal) = spec(file);
        let panel = panel_at(file, module, aircraft, signal, 0);
        assert_eq!(
            value(&panel, BACKLIGHT),
            0,
            "{file}: panel labels follow the console down to zero"
        );
    }
}

#[test]
fn every_shipped_profile_binds_both_gates() {
    // An unbound gate is swept to 0 and silently blanks the lamps beneath it.
    // SL hides all 14 indicators, FLAG hides 7 of them.
    //
    // This walks the profiles directory rather than a list, because these
    // profiles ship as a baseline for other people's panels. A new one added
    // without both gates bound is a panel that looks broken on someone else's
    // desk, and they have no way to know why.
    let dir = root().join("data/defaults");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("data/defaults should exist") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let p = Profile::load(&path).unwrap_or_else(|e| panic!("{name} should parse: {e}"));

        // Only panels the profile actually drives need their gates bound.
        if !p.bindings.iter().any(|b| b.device == PTO2) {
            continue;
        }
        for gate in ["SL", "FLAG"] {
            let bound = p
                .bindings
                .iter()
                .any(|b| b.device == PTO2 && b.led == gate && !b.is_placeholder());
            assert!(bound, "{name} leaves {gate} unbound, which blanks lamps beneath it");
        }
        checked += 1;
    }
    assert!(checked > 0, "no shipped profiles were checked, so this proves nothing");
    let _ = SL;
}

#[test]
fn every_shipped_gate_survives_a_dark_cockpit() {
    // Bound is not enough. A gate that follows a cockpit dimmer down to zero
    // blanks its lamps in daylight exactly as an unbound one does. The AH-64D
    // shipped that way for both gates, and SL is the worse of the two: it
    // takes every indicator on the panel with it.
    //
    // Every signal reads zero here, which is the daylight cockpit: knobs down
    // and the seat position at its first value.
    let devices = devices();
    let pto2 = devices.device(PTO2).expect("PTO2 is in the inventory");
    let dir = root().join("data/defaults");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).expect("data/defaults should exist") {
        let path = entry.expect("readable entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
        let p = Profile::load(&path).unwrap_or_else(|e| panic!("{name} should parse: {e}"));

        for gate in ["SL", "FLAG"] {
            let Some(b) = p.bindings.iter().find(|b| b.device == PTO2 && b.led == gate) else {
                continue;
            };
            let (_, led) = pto2.led(gate).expect("gate is in the inventory");
            let value = p.resolve_binding(b, led, |_| Some(0));
            assert!(
                value.is_some_and(|v| v > 0),
                "{name}: {gate} resolves to {value:?} with the cockpit dark, which blanks its lamps in daylight"
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "no shipped gates were checked, so this proves nothing");
}

fn spec(file: &str) -> (&'static str, &'static str, &'static str) {
    match file {
        "a-10c-2.json" => ("A-10C", "A-10C_2", "LCP_CONSOLE"),
        "fa-18c-hornet.json" => ("FA-18C_hornet", "FA-18C_hornet", "CONSOLES_DIMMER"),
        other => panic!("no spec for {other}"),
    }
}
