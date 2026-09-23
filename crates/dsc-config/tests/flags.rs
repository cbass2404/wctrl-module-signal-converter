//! Conditions and fields the installed DCS-BIOS cannot back, and what runs
//! instead.
//!
//! The rule: a bad condition takes its whole AND chain with it, because the
//! rest of the chain on its own could light the lamp when nobody meant it to.
//! In an `any_of`, each branch is its own chain, so only that branch goes.

use std::path::Path;

use dsc_config::{DeviceInventory, DisplayCatalogue, Module, Place, Profile, Unsound};

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

/// GEAR is on/off, MODE a three-position selector, DIM a dimmer, CHAN text.
fn module() -> Module {
    serde_json::from_str(
        r#"{
          "module": "TEST",
          "aircraft": ["TEST"],
          "signals": [
            {"id": "GEAR", "outputs": [{"address": 100, "mask": 1, "max_value": 1}]},
            {"id": "MODE", "outputs": [{"address": 102, "mask": 3, "max_value": 2}]},
            {"id": "DIM", "outputs": [{"address": 104, "mask": 65535, "max_value": 65535}]},
            {"id": "CHAN", "outputs": [{"address": 200, "max_length": 2, "type": "string"}]}
          ]
        }"#,
    )
    .expect("the fixture module parses")
}

fn profile(body: &str) -> Profile {
    serde_json::from_str(&format!(
        r#"{{"schema_version": 2, "name": "T", "aircraft": ["TEST"], "module": "TEST", {body}}}"#
    ))
    .expect("the fixture profile parses")
}

fn loads(p: &Profile) {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    p.validate(&module(), &devices, &displays, &dsc_config::PageLibrary::default()).expect("the profile loads");
}

#[test]
fn a_profile_that_reads_only_real_signals_flags_nothing() {
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "DIM", "on_when": {"scale": [0, 65535]}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "HOOK", "off": 0,
             "conditions": [{"source": "MODE", "on_when": {"in": [1, 2]}}]}
        ]"#,
    );
    assert!(p.flags(&module()).is_empty(), "{:?}", p.flags(&module()));
}

#[test]
fn a_missing_signal_turns_off_the_whole_chain() {
    // GEAR on its own would light the lamp whenever the gear is down, which
    // is not what "gear down AND the missing thing" asked for.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "HOOK", "off": 0,
             "conditions": [
                {"source": "GEAR", "on_when": {"equals": 1}},
                {"source": "GONE", "on_when": {"equals": 1}}
             ]}
        ]"#,
    );
    let flags = p.flags(&module());
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].source, "GONE");
    assert_eq!(flags[0].why, Unsound::Missing);
    assert_eq!(flags[0].place, Place::Condition { binding: 0, index: 1 });

    loads(&p);
    let run = p.runnable(&module());
    loads(&run);
    assert!(run.bindings[0].is_placeholder(), "{:?}", run.bindings[0]);
    // The file's copy keeps the row, so it works again once the source is fixed.
    assert_eq!(p.bindings[0].conditions.len(), 2);
}

#[test]
fn a_value_above_the_signals_range_is_flagged_the_same_way() {
    // MODE reports 0 to 2. A condition waiting for 3 was written for a
    // selector with another position, which this one is not.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "HOOK", "off": 0,
             "conditions": [{"source": "MODE", "on_when": {"equals": 3}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"scale": [0, 65535]}}]}
        ]"#,
    );
    let flags = p.flags(&module());
    assert_eq!(flags.len(), 2, "{flags:?}");
    assert_eq!(flags[0].why, Unsound::AboveRange { value: 3, max: 2 });
    assert_eq!(flags[1].why, Unsound::AboveRange { value: 65535, max: 1 });
    let run = p.runnable(&module());
    assert!(run.bindings.iter().all(|b| b.is_placeholder()));
}

#[test]
fn a_bad_alternative_is_dropped_and_the_others_still_work() {
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0, "pick": "latest",
             "any_of": [
                {"conditions": [{"source": "GONE", "on_when": {"scale": [0, 65535]}}]},
                {"conditions": [{"source": "DIM", "on_when": {"scale": [0, 65535]}}]}
             ]}
        ]"#,
    );
    let flags = p.flags(&module());
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].place, Place::Branch { binding: 0, branch: 0, index: 0 });

    let run = p.runnable(&module());
    loads(&run);
    assert_eq!(run.bindings[0].any_of.len(), 1);
    assert_eq!(run.bindings[0].any_of[0].conditions[0].source, "DIM");
}

#[test]
fn a_lamp_with_every_alternative_bad_is_turned_off_and_still_loads() {
    // `pick` means nothing with no alternatives, and would fail validation if
    // it were left behind, so it goes with them.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0, "pick": "latest",
             "any_of": [
                {"conditions": [{"source": "GONE", "on_when": {"scale": [0, 65535]}}]},
                {"conditions": [{"source": "ALSO_GONE", "on_when": {"scale": [0, 65535]}}]}
             ]}
        ]"#,
    );
    assert_eq!(p.flags(&module()).len(), 2);
    let run = p.runnable(&module());
    loads(&run);
    assert!(run.bindings[0].is_placeholder(), "{:?}", run.bindings[0]);
}

#[test]
fn a_lamp_mirroring_one_that_was_turned_off_still_loads() {
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "GONE", "on_when": {"scale": [0, 65535]}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "SL", "same_as": "Backlight"}
        ]"#,
    );
    let run = p.runnable(&module());
    loads(&run);
    assert!(run.bindings[0].is_placeholder());
    assert_eq!(run.bindings[1].same_as.as_deref(), Some("Backlight"));
}

#[test]
fn a_display_field_reading_a_missing_signal_is_left_out() {
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC", "cells": "0-1", "source": "CHAN"},
            {"device": "CarrierAce_UFC", "display": "UFC", "cells": "2-3", "source": "GONE"}
        ]"#,
    );
    let flags = p.flags(&module());
    assert_eq!(flags.len(), 1);
    assert_eq!(flags[0].place, Place::Field { readout: 1 });
    let run = p.runnable(&module());
    assert_eq!(run.readouts.len(), 1);
    assert_eq!(run.readouts[0].sources(), vec!["CHAN"]);
}
