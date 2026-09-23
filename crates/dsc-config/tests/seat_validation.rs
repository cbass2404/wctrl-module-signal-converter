//! What a profile may say about crew stations, and what it may not.
//!
//! A seat is only meaningful where DCS-BIOS reports one, which is 5 of the 50
//! catalogued modules. Accepting it elsewhere would mean a field that never
//! paints and nothing anywhere saying why.

use std::path::Path;

use dsc_config::{DeviceInventory, DisplayCatalogue, Module, Profile, SEAT_SIGNAL};

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn module(with_seat: bool) -> Module {
    let seat = if with_seat {
        r#"{
            "id": "SEAT_POSITION",
            "control_type": "metadata",
            "outputs": [{"address": 100, "mask": 65535, "shift": 0, "max_value": 1, "max_length": null}]
        },"#
    } else {
        ""
    };
    serde_json::from_str(&format!(
        r#"{{
          "module": "TEST",
          "aircraft": ["TEST"],
          "signals": [
            {seat}
            {{
              "id": "PLT_CHAN",
              "control_type": "display",
              "outputs": [{{"address": 200, "mask": null, "max_value": null, "max_length": 2, "type": "string"}}]
            }},
            {{
              "id": "OP_CHAN",
              "control_type": "display",
              "outputs": [{{"address": 210, "mask": null, "max_value": null, "max_length": 2, "type": "string"}}]
            }}
          ]
        }}"#
    ))
    .expect("the fixture module parses")
}

fn profile(readouts: &str) -> Profile {
    serde_json::from_str(&format!(
        r#"{{
          "schema_version": 2, "name": "T", "aircraft": ["TEST"], "module": "TEST",
          "readouts": [{readouts}]
        }}"#
    ))
    .expect("the fixture profile parses")
}

fn field(source: &str, cells: &str, seat: Option<u32>) -> String {
    let seat = seat.map(|s| format!(r#", "seat": {s}"#)).unwrap_or_default();
    format!(
        r#"{{"device": "CarrierAce_UFC", "display": "UFC1", "cells": "{cells}", "source": "{source}"{seat}}}"#
    )
}

fn check(m: &Module, p: &Profile) -> dsc_config::Result<()> {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    p.validate(m, &devices, &displays, &dsc_config::PageLibrary::default())
}

#[test]
fn two_seats_may_share_one_window() {
    // The reason the field exists. They cannot both be occupied, so they cannot
    // both be painting, so the usual one-owner-per-cell rule does not apply.
    let m = module(true);
    let p = profile(&format!(
        "{}, {}",
        field("PLT_CHAN", "34", Some(0)),
        field("OP_CHAN", "34", Some(1))
    ));
    check(&m, &p).expect("different seats may share cells");
}

#[test]
fn one_seat_may_not_claim_a_window_twice() {
    let m = module(true);
    let p = profile(&format!(
        "{}, {}",
        field("PLT_CHAN", "34", Some(0)),
        field("OP_CHAN", "34", Some(0))
    ));
    let err = check(&m, &p).expect_err("the same seat still has one owner per cell");
    assert!(format!("{err}").contains("overlap"), "{err}");
}

#[test]
fn a_seated_field_still_collides_with_an_unseated_one() {
    // A field with no seat paints from every station, so it is live whichever
    // seat the other one wants.
    let m = module(true);
    let p = profile(&format!(
        "{}, {}",
        field("PLT_CHAN", "34", Some(0)),
        field("OP_CHAN", "34", None)
    ));
    let err = check(&m, &p).expect_err("an unseated field is always live");
    assert!(format!("{err}").contains("overlap"), "{err}");
}

#[test]
fn a_seat_is_refused_where_the_module_does_not_report_one() {
    let m = module(false);
    let p = profile(&field("PLT_CHAN", "34", Some(0)));
    let err = check(&m, &p).expect_err("a seat that can never resolve is an error");
    let text = format!("{err}");
    assert!(text.contains(SEAT_SIGNAL), "the message names the signal: {text}");

    // And the same profile is fine once the seat is dropped.
    check(&m, &profile(&field("PLT_CHAN", "34", None))).expect("no seat, no problem");
}

#[test]
fn a_seat_the_module_does_not_have_is_refused() {
    // The fixture reports 0 or 1. Asking for the door gunner of an aircraft
    // with two seats is a typo, and it would simply never paint.
    let m = module(true);
    let err = check(&m, &profile(&field("PLT_CHAN", "34", Some(2))))
        .expect_err("seat 2 does not exist here");
    assert!(format!("{err}").contains("seat 2"), "{err}");
}
