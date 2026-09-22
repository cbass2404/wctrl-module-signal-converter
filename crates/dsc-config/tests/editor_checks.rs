//! What the editor asks for: every fault at once, named where it is.
//!
//! `validate` answers the daemon's question, which is whether to load the file
//! at all, so it stops at the first fault. The editor asks a different one: it
//! is showing the user a list to work through, and reporting one fault at a
//! time would mean fixing something only to be told the same bad news again.
//!
//! These cover `problems` specifically: that it finds what `validate` finds,
//! that it keeps going, and that it says each thing once.

use std::path::Path;

use dsc_config::{DeviceInventory, DisplayCatalogue, Module, Profile};

fn r(p: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(p)
}

fn module() -> Module {
    serde_json::from_str(
        r#"{
          "module": "TEST",
          "aircraft": ["TEST"],
          "signals": [
            {
              "id": "GEAR",
              "control_type": "led",
              "outputs": [{"address": 100, "mask": 1, "shift": 0, "max_value": 1, "max_length": null}]
            },
            {
              "id": "CHAN",
              "control_type": "display",
              "outputs": [{"address": 200, "mask": null, "max_value": null, "max_length": 2, "type": "string"}]
            },
            {
              "id": "KNOB",
              "control_type": "selector",
              "outputs": [{"address": 300, "mask": 224, "shift": 5, "max_value": 5, "max_length": null}]
            }
          ]
        }"#,
    )
    .expect("the fixture module parses")
}

fn profile(body: &str) -> Profile {
    serde_json::from_str(&format!(
        r#"{{"name": "T", "aircraft": ["TEST"], "module": "TEST", {body}}}"#
    ))
    .expect("the fixture profile parses")
}

fn found(p: &Profile) -> Vec<String> {
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    p.problems(&module(), &devices, &displays)
        .iter()
        .map(|e| e.to_string())
        .collect()
}

#[test]
fn a_clean_profile_has_nothing_to_report() {
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]}
        ]"#,
    );
    assert!(found(&p).is_empty(), "{:?}", found(&p));
}

#[test]
fn every_fault_is_reported_not_just_the_first() {
    // The point of the whole exercise. A user given one of these, who fixes it
    // and is handed the next, has been made to do the work three times.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "NOT_A_LAMP", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "SL", "off": 0, "always": true,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "Master_Caution", "off": 0, "on": 200,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 3, "{problems:?}");
    assert!(problems.iter().any(|m| m.contains("NOT_A_LAMP")), "{problems:?}");
    assert!(problems.iter().any(|m| m.contains("SL")), "{problems:?}");
    assert!(problems.iter().any(|m| m.contains("200")), "{problems:?}");
}

#[test]
fn validate_still_stops_at_the_first_one() {
    // The daemon's contract is unchanged: one error, and the profile is skipped.
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "NOT_A_LAMP", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]}
        ]"#,
    );
    p.validate(&module(), &devices, &displays)
        .expect_err("a bad profile is still an error");

    // A signal this DCS-BIOS lacks is not one: the profile loads, flagged.
    let other_release = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "NOT_A_SIGNAL", "on_when": {"equals": 1}}]}
        ]"#,
    );
    other_release
        .validate(&module(), &devices, &displays)
        .expect("a missing signal flags the row rather than refusing the profile");

    let clean = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]}
        ]"#,
    );
    clean
        .validate(&module(), &devices, &displays)
        .expect("a clean profile still loads");
}

#[test]
fn a_condition_with_no_signal_chosen_is_said_in_those_terms() {
    // The editor creates one of these the moment "Add condition" is clicked, so
    // it is the most common thing a half-finished profile carries. Reporting it
    // as `unknown signal ""` would be true and useless.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [
                {"source": "", "on_when": {"equals": 1}},
                {"source": "", "on_when": {"equals": 1}}
             ]}
        ]"#,
    );
    let problems = found(&p);
    // Twice unfinished is still one lamp to go and finish.
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("Backlight"), "{:?}", problems[0]);
    assert!(problems[0].contains("no signal chosen"), "{:?}", problems[0]);
}

#[test]
fn a_field_with_no_signal_chosen_names_where_it_is() {
    // Adding a field lands on a run of cells before it has anything in it,
    // and the cells are the only thing on screen that identifies it.
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-33", "source": ""}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("30-33"), "{:?}", problems[0]);
    assert!(problems[0].contains("nothing in it"), "{:?}", problems[0]);
}

#[test]
fn an_overlap_is_one_problem_not_two() {
    // Both fields are at fault and either one could be moved, but there is one
    // thing wrong. Listing it from each end would read as two conflicts.
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-33", "source": "CHAN"},
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "32-35", "source": "CHAN"}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("overlap"), "{:?}", problems[0]);
}

#[test]
fn a_mirror_chain_is_found_without_the_target_hiding_it() {
    // Two dimmers pointed at each other, which is what a two-dimmer panel
    // produces if the editor lets both lamps mirror the other one.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0, "same_as": "FLAG"},
            {"device": "TAKEOFF_PLANEL_2", "led": "FLAG", "off": 0, "same_as": "Backlight"}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 2, "both ends are a chain: {problems:?}");
    assert!(problems.iter().all(|m| m.contains("mirrors")), "{problems:?}");
}

#[test]
fn a_backlight_can_match_one_on_another_device() {
    // One knob for the pit: the MFD bezel follows the PTO2's backlight rather
    // than carrying a copy of its condition.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]},
            {"device": "CarrierAce_MFD_L", "led": "INST_PNL_Backlight", "off": 0,
             "same_as": "Backlight", "same_as_device": "TAKEOFF_PLANEL_2"}
        ]"#,
    );
    assert!(found(&p).is_empty(), "{:?}", found(&p));
}

#[test]
fn a_mirror_on_another_device_is_checked_there() {
    // The lamp is looked for on the device named, not on the mirroring one:
    // the MFD has no `Backlight`, and the Orion's `A/A` is an indicator.
    let missing = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "same_as": "Backlight", "same_as_device": "CarrierAce_MFD_L"}
        ]"#,
    );
    let problems = found(&missing);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("CarrierAce_MFD_L"), "{:?}", problems[0]);

    let indicator = profile(
        r#""bindings": [
            {"device": "Orion_Throttle_Base_II", "led": "A/A", "off": 0,
             "conditions": [{"source": "GEAR", "on_when": {"equals": 1}}]},
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "same_as": "A/A", "same_as_device": "Orion_Throttle_Base_II"}
        ]"#,
    );
    let problems = found(&indicator);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("only lamps that dim"), "{:?}", problems[0]);
}

#[test]
fn a_loop_across_two_panels_is_still_a_chain() {
    // Across devices the rule is the same: the target must read signals of its
    // own, so two panels pointed at each other are refused at both ends.
    let p = profile(
        r#""bindings": [
            {"device": "TAKEOFF_PLANEL_2", "led": "Backlight", "off": 0,
             "same_as": "INST_PNL_Backlight", "same_as_device": "CarrierAce_MFD_L"},
            {"device": "CarrierAce_MFD_L", "led": "INST_PNL_Backlight", "off": 0,
             "same_as": "Backlight", "same_as_device": "TAKEOFF_PLANEL_2"}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 2, "both ends are a chain: {problems:?}");
    assert!(problems.iter().all(|m| m.contains("mirrors something itself")), "{problems:?}");
}

#[test]
fn a_lamp_cannot_reach_itself_through_a_panel_that_follows() {
    // The MFD_R takes the MFD_L's setup, so its backlight is the MFD_L's, and
    // pointing the MFD_L at it is pointing the lamp at itself.
    let p = profile(
        r#""follows": {"CarrierAce_MFD_R": "CarrierAce_MFD_L"},
        "bindings": [
            {"device": "CarrierAce_MFD_L", "led": "INST_PNL_Backlight", "off": 0,
             "same_as": "INST_PNL_Backlight", "same_as_device": "CarrierAce_MFD_R"}
        ]"#,
    );
    let problems = found(&p);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("mirrors something itself"), "{:?}", problems[0]);
}

#[test]
fn a_number_shown_as_sent_needs_no_range() {
    // A six position knob drawn as the digit DCS-BIOS sends. The editor
    // offers that, so the check has to let it through.
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-33", "source": "KNOB"}
        ]"#,
    );
    assert!(found(&p).is_empty(), "{:?}", found(&p));
}

#[test]
fn a_number_can_be_shown_by_its_aliases() {
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-33", "source": "KNOB",
             "value_aliases": {"0": "OFF", "3": "SEMI"}}
        ]"#,
    );
    assert!(found(&p).is_empty(), "{:?}", found(&p));
}

#[test]
fn what_dcs_bios_says_a_signal_is_cautions_rather_than_refuses() {
    // DCS-BIOS marks CHAN as text, so aliases for its values should mean
    // nothing. Its metadata is not right for every module, so the user is
    // told and the profile still loads.
    // A clean field comes first, so the caution has to find its way to the
    // second one rather than landing on whatever is at the top.
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "20-23", "source": "KNOB"},
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-31", "source": "CHAN",
             "value_aliases": {"0": "OFF"}}
        ]"#,
    );
    assert!(found(&p).is_empty(), "{:?}", found(&p));
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let cautions = p.field_cautions(&module(), &devices, &displays);
    assert_eq!(cautions.len(), 1, "{cautions:?}");
    assert_eq!(cautions[0].0, 1, "on the field it is about");
    assert!(cautions[0].1.contains("aliases for its values"), "{:?}", cautions[0]);
}

#[test]
fn a_field_too_narrow_for_its_text_is_cautioned_on_that_field() {
    let p = profile(
        r#""readouts": [
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "20-23", "source": "KNOB"},
            {"device": "CarrierAce_UFC", "display": "UFC1", "cells": "30-31", "text": "ABCDE"}
        ]"#,
    );
    let devices = DeviceInventory::load(&r("data/devices.json")).expect("devices");
    let displays = DisplayCatalogue::load_dir(&r("data/displays")).expect("displays");
    let cautions = p.field_cautions(&module(), &devices, &displays);
    assert_eq!(cautions.len(), 1, "{cautions:?}");
    assert_eq!(cautions[0].0, 1, "on the field it is about");
    assert!(cautions[0].1.starts_with("This field needs up to 5 cells"), "{:?}", cautions[0]);
    // And not in the profile's own list, which is for the profile as a whole.
    assert!(p.cautions(&devices).is_empty());
}
