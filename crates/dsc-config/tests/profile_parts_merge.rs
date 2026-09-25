//! `merge::merge` rewrites a profile the user owns, on their say-so, so what it
//! takes and what it leaves is pinned here.
//!
//! The rule: a picked lamp takes the source's row if it assigns one;
//! everything not picked is left exactly as it was. Page slots are pinned in
//! `page_sharing.rs`.

use std::path::Path;

use dsc_config::merge::{self, LampPick, Pick, SlotPick};
use dsc_config::{DeviceInventory, PageSlots, Profile, Slot};

fn inventory() -> DeviceInventory {
    DeviceInventory::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json"))
        .expect("devices.json loads")
}

fn lamps(device: &str, leds: &[&str]) -> Vec<LampPick> {
    leds.iter().map(|l| LampPick { device: device.into(), led: (*l).into() }).collect()
}

fn profile(json: &str) -> Profile {
    serde_json::from_str(json).expect("the test profile parses")
}

/// Two profiles on one module, the F-14 and F-14BU case, set up differently.
fn source() -> Profile {
    profile(
        r#"{
      "name": "Source", "aircraft": ["F-14B"], "module": "F-14",
      "bindings": [
        { "device": "TAKEOFF_PLANEL_2", "led": "HOOK",
          "conditions": [{ "source": "HOOK_LIGHT", "on_when": { "gte": 1 } }] },
        { "device": "TAKEOFF_PLANEL_2", "led": "NOSE",
          "conditions": [{ "source": "NOSE_LIGHT", "on_when": { "gte": 1 } }] },
        { "device": "TAKEOFF_PLANEL_2", "led": "Backlight", "conditions": [] },
        { "device": "CarrierAce_UFC", "led": "LCDBacklight", "always": true }
      ]
    }"#,
    )
}

fn target() -> Profile {
    profile(
        r#"{
      "name": "Target", "aircraft": ["F-14BU"], "module": "F-14",
      "bindings": [
        { "device": "TAKEOFF_PLANEL_2", "led": "HOOK",
          "conditions": [{ "source": "OTHER_HOOK", "on_when": { "gte": 1 } }] },
        { "device": "TAKEOFF_PLANEL_2", "led": "Backlight", "always": true, "note": "mine" },
        { "device": "CarrierAce_UFC", "led": "LCDBacklight", "conditions": [] }
      ],
      "follows": { "MCDU_CoPilot": "MCDU_Captain" }
    }"#,
    )
}

#[test]
fn parts_offer_only_what_the_source_sets_up() {
    let parts = merge::parts(&source(), &inventory());
    let lights: Vec<(&str, Vec<&str>)> = parts
        .lights
        .iter()
        .map(|l| (l.device.as_str(), l.lamps.iter().map(|p| p.led.as_str()).collect()))
        .collect();
    assert!(lights.contains(&("TAKEOFF_PLANEL_2", vec!["NOSE", "HOOK"])), "{lights:?}");
    assert!(lights.contains(&("CarrierAce_UFC", vec!["LCDBacklight"])), "{lights:?}");
}

#[test]
fn a_picked_panel_takes_assigned_lamps_and_keeps_the_rest() {
    let pick = Pick { lights: lamps("TAKEOFF_PLANEL_2", &["HOOK", "NOSE", "Backlight"]), slots: vec![] };
    let merged = merge::merge(&target(), &source(), &pick, &inventory()).unwrap();
    let row = |led: &str| {
        merged.profile.bindings.iter().find(|b| b.device == "TAKEOFF_PLANEL_2" && b.led == led).unwrap()
    };
    assert_eq!(row("HOOK").conditions[0].source, "HOOK_LIGHT", "replaced");
    assert_eq!(row("NOSE").conditions[0].source, "NOSE_LIGHT", "added");
    assert_eq!(row("Backlight").note, "mine", "unassigned in the source, so the target's stays");
    let ufc = merged.profile.bindings.iter().find(|b| b.device == "CarrierAce_UFC").unwrap();
    assert!(!ufc.always, "a panel not picked is untouched");
    let c = &merged.changes[0];
    assert_eq!((c.added, c.replaced, c.removed, c.unchanged), (1, 1, 0, 0));
    assert_eq!(merged.profile.name, "Target");
    assert_eq!(merged.profile.aircraft, vec!["F-14BU".to_string()]);
    assert_eq!(merged.profile.follows.len(), 1, "followers are the target's own business");
}

#[test]
fn a_lamp_not_picked_keeps_the_target_row() {
    let pick = Pick { lights: lamps("TAKEOFF_PLANEL_2", &["NOSE"]), slots: vec![] };
    let merged = merge::merge(&target(), &source(), &pick, &inventory()).unwrap();
    let hook = merged
        .profile
        .bindings
        .iter()
        .find(|b| b.device == "TAKEOFF_PLANEL_2" && b.led == "HOOK")
        .unwrap();
    assert_eq!(hook.conditions[0].source, "OTHER_HOOK");
    let c = &merged.changes[0];
    assert_eq!((c.added, c.replaced, c.removed, c.unchanged), (1, 0, 0, 0));
}

#[test]
fn merging_onto_a_follower_says_it_will_not_be_used() {
    let mut source = source();
    let mut slots = PageSlots::empty(6);
    slots.slots[0] = Some(Slot::new("a"));
    source.screens.insert("MCDU_CoPilot".into(), slots);
    let pick = Pick { slots: vec![SlotPick { device: "MCDU_CoPilot".into(), slot: 1 }], ..Pick::default() };
    let merged = merge::merge(&target(), &source, &pick, &inventory()).unwrap();
    assert!(merged.notes.iter().any(|n| n.contains("follows")), "{:?}", merged.notes);
}

#[test]
fn profiles_on_different_modules_cannot_merge() {
    let mut other = source();
    other.module = "A-10C".into();
    let err = merge::merge(&target(), &other, &Pick::default(), &inventory()).unwrap_err();
    assert!(err.contains("A-10C"), "{err}");
}
