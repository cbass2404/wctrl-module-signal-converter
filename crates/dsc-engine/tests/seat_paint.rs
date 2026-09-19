//! A display field bound to one crew station.
//!
//! DCS-BIOS exports the whole cockpit whatever seat you are in, so a multicrew
//! aircraft publishes both stations at once and a field has no way to tell
//! which reading is yours. `SEAT_POSITION` is how DCS-BIOS answers that, and it
//! is spelled the same in every module that has one.
//!
//! Built on a synthetic module rather than `data/catalogue`, which is generated
//! per machine and not committed.

use std::path::Path;
use std::time::{Duration, Instant};

use dsc_bios::Write as BiosWrite;
use dsc_config::{Catalogue, DeviceInventory, DisplayCatalogue, Module, Profile, Screen};
use dsc_engine::Engine;

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

/// Pack an ASCII field the way DCS-BIOS does, two bytes per word.
fn text_at(address: u16, s: &str) -> Vec<BiosWrite> {
    let b = s.as_bytes();
    (0..b.len().div_ceil(2))
        .map(|i| BiosWrite {
            address: address + (i as u16) * 2,
            value: u16::from(b[i * 2]) | (u16::from(*b.get(i * 2 + 1).unwrap_or(&0)) << 8),
        })
        .collect()
}

/// A two-seat module with one channel field per station, both always exported.
fn module() -> Module {
    serde_json::from_str(
        r#"{
          "module": "TEST_TWOSEAT",
          "aircraft": ["TEST_TWOSEAT"],
          "signals": [
            {
              "id": "SEAT_POSITION",
              "control_type": "metadata",
              "outputs": [{"address": 100, "mask": 65535, "shift": 0, "max_value": 1, "max_length": null}]
            },
            {
              "id": "PLT_CHAN",
              "control_type": "display",
              "outputs": [{"address": 200, "mask": null, "max_value": null, "max_length": 2, "type": "string"}]
            },
            {
              "id": "OP_CHAN",
              "control_type": "display",
              "outputs": [{"address": 210, "mask": null, "max_value": null, "max_length": 2, "type": "string"}]
            }
          ]
        }"#,
    )
    .expect("the fixture module parses")
}

/// Both stations pointed at the same comm window, which is the whole point.
fn profile() -> Profile {
    serde_json::from_str(
        r#"{
          "name": "Two seat",
          "aircraft": ["TEST_TWOSEAT"],
          "module": "TEST_TWOSEAT",
          "readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "34",
             "source": "PLT_CHAN", "seat": 0},
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "34",
             "source": "OP_CHAN", "seat": 1}
          ]
        }"#,
    )
    .expect("the fixture profile parses")
}

fn engine() -> (Engine, DisplayCatalogue) {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let cat = Catalogue::from_modules(vec![module()]);
    let mut e = Engine::new(devices, cat, vec![profile()]).with_displays(displays.clone());
    e.set_connected(vec!["CarrierAce_UFC".into()]);
    (e, displays)
}

/// The whole 96-byte buffer the engine would leave on the glass.
fn painted(e: &mut Engine, writes: &[BiosWrite], at: Instant) -> Vec<u8> {
    let mut batch = e.ingest(writes, at);
    if batch.lcd.is_empty() {
        batch = e.tick(at + Duration::from_secs(5));
    }
    let mut buffer = vec![0u8; 96];
    for w in &batch.lcd {
        let start = w.group as usize * 4;
        buffer[start..start + w.bytes.len()].copy_from_slice(&w.bytes);
    }
    buffer
}

/// What the glass should read with one value in the comm window.
fn expected(displays: &DisplayCatalogue, value: &str) -> Vec<u8> {
    let map = displays.get("UFC1").unwrap();
    let mut screen = Screen::new(map);
    screen.draw(map, 34, value).expect("the value draws");
    screen.bytes().to_vec()
}

#[test]
fn the_window_shows_the_station_you_are_sitting_in() {
    let (mut e, displays) = engine();

    // Both stations are exported at once, which is the situation the seat
    // exists to resolve: nothing else in the stream says which one is yours.
    let mut writes = text_at(0, "TEST_TWOSEAT\0\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(200, " 7"));
    writes.extend(text_at(210, " 4"));
    writes.push(BiosWrite { address: 100, value: 0 });

    let t0 = Instant::now();
    assert_eq!(
        painted(&mut e, &writes, t0),
        expected(&displays, " 7"),
        "in the pilot's seat the window shows the pilot's channel"
    );

    // Move to the other station. Nothing else changes.
    let moved = vec![BiosWrite { address: 100, value: 1 }];
    assert_eq!(
        painted(&mut e, &moved, t0 + Duration::from_secs(10)),
        expected(&displays, " 4"),
        "moving seats moves the window with it"
    );
}

#[test]
fn a_field_bound_to_a_seat_stays_dark_until_the_seat_is_known() {
    let (mut e, displays) = engine();

    // The aircraft and both channels have arrived; SEAT_POSITION has not.
    // Painting either one would be a guess, and a wrong guess here looks right,
    // which is worse than a dark window.
    let mut writes = text_at(0, "TEST_TWOSEAT\0\0\0\0\0\0\0\0\0\0\0\0");
    writes.extend(text_at(200, " 7"));
    writes.extend(text_at(210, " 4"));

    let buffer = painted(&mut e, &writes, Instant::now());
    assert_eq!(buffer, vec![0u8; 96], "nothing is on the glass yet");
    assert_ne!(buffer, expected(&displays, " 7"));
    assert_ne!(buffer, expected(&displays, " 4"));
}
