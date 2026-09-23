//! `Profiles::merge_new` writes to files the user owns, so it is pinned hard.
//!
//! The rule it has to keep: add what is missing, change nothing else. A user
//! who configured a panel months ago must find that configuration exactly as
//! they left it after an update adds support for a panel they also own.

use std::path::{Path, PathBuf};

use dsc_config::{DeviceInventory, Profile, Profiles};

/// A folder of its own for one test, removed when the test ends, pass or fail,
/// so a run leaves nothing in the temp folder.
struct Scratch(PathBuf);

impl std::ops::Deref for Scratch {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(name: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!(
        "dsc-merge-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("defaults")).unwrap();
    std::fs::create_dir_all(dir.join("active")).unwrap();
    Scratch(dir)
}

fn inventory() -> DeviceInventory {
    DeviceInventory::load(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/devices.json"),
    )
    .expect("devices.json loads")
}

/// A profile from before the UFC existed: PTO2 lamps only, one of them tuned.
fn old_profile() -> String {
    r#"{
      "schema_version": 2,
      "name": "Old",
      "aircraft": ["FA-18C_hornet"],
      "module": "FA-18C_hornet",
      "bindings": [
        { "device": "TAKEOFF_PLANEL_2", "led": "Backlight",
          "conditions": [{ "source": "INST_PNL_DIMMER",
                           "on_when": { "scale": [0, 65535] } }],
          "off": 12, "note": "mine" },
        { "device": "TAKEOFF_PLANEL_2", "led": "HOOK", "conditions": [] }
      ]
    }"#
    .to_string()
}

/// A profile that predates a display field the default has gained.
///
/// The A-10C and AH-64D both gained an MCDU divider and no lamp, which is the
/// case that found this: the field was merged into the loaded profile and then
/// thrown away, because only added bindings decided whether the file was
/// written. It happened again on every start and said nothing.
#[test]
fn a_profile_gains_a_display_field_the_default_has_added() {
    let dir = scratch("field");
    let shipped = r#"{
      "schema_version": 2,
      "name": "Shipped",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      "bindings": [],
      "readouts": [
        { "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
          "divider": true, "colour": "green" }
      ]
    }"#;
    let mine = r#"{
      "schema_version": 2,
      "name": "Shipped",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      "bindings": [],
      "readouts": [
        { "device": "MCDU_Captain", "display": "MCDU", "cells": "96-119",
          "source": "CDU_LINE0", "colour": "green" }
      ]
    }"#;
    std::fs::write(dir.join("defaults/a-10c.json"), shipped).unwrap();
    std::fs::write(dir.join("active/a-10c.json"), mine).unwrap();

    let notes = Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .expect("merge runs");
    assert!(
        notes.iter().any(|n| n.contains("display field")),
        "the added field is reported: {notes:?}"
    );

    // Written to disk, not merely merged in memory.
    let after = Profile::load(&dir.join("active/a-10c.json")).expect("still loads");
    assert!(
        after.readouts.iter().any(|r| r.divider),
        "the divider reached the file: {:?}",
        after.readouts.iter().map(|r| r.cells.to_string()).collect::<Vec<_>>()
    );
    assert!(
        after.readouts.iter().any(|r| r.sources() == vec!["CDU_LINE0"]),
        "and the user's own field is still there"
    );
}

/// The other half: a field the user has already claimed those cells for is not
/// replaced by the shipped one, and does not count as a change.
#[test]
fn a_shipped_field_never_displaces_one_the_user_put_there() {
    let dir = scratch("claimed");
    let shipped = r#"{
      "schema_version": 2,
      "name": "Shipped",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      "bindings": [],
      "readouts": [
        { "device": "MCDU_Captain", "display": "MCDU", "cells": "72-95",
          "divider": true, "colour": "green" }
      ]
    }"#;
    let mine = r#"{
      "schema_version": 2,
      "name": "Shipped",
      "aircraft": ["A-10C"],
      "module": "A-10C",
      "bindings": [],
      "readouts": [
        { "device": "MCDU_Captain", "display": "MCDU", "cells": "80-90",
          "source": "CDU_LINE9", "colour": "white" }
      ]
    }"#;
    std::fs::write(dir.join("defaults/a-10c.json"), shipped).unwrap();
    std::fs::write(dir.join("active/a-10c.json"), mine).unwrap();

    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .expect("merge runs");

    let after = Profile::load(&dir.join("active/a-10c.json")).expect("still loads");
    assert_eq!(after.readouts.len(), 1, "nothing was added over the user's field");
    assert_eq!(after.readouts[0].sources(), vec!["CDU_LINE9"]);
}

#[test]
fn a_profile_written_before_a_device_existed_gains_rows_for_it() {
    let dir = scratch("gains");
    let active = dir.join("active/old.json");
    std::fs::write(&active, old_profile()).unwrap();

    let notes = Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .expect("merge runs");
    assert_eq!(notes.len(), 1, "one profile was touched: {notes:?}");

    let after = Profile::load(&active).unwrap();
    let ufc: Vec<&str> = after
        .bindings
        .iter()
        .filter(|b| b.device == "CarrierAce_UFC")
        .map(|b| b.led.as_str())
        .collect();
    assert_eq!(
        ufc.len(),
        3,
        "every UFC lamp appears, the two that share the vendor's name across \
         parts among them: {ufc:?}"
    );
    assert!(ufc.contains(&"INST_PNL_Backlight"));
    assert!(ufc.contains(&"HUD_INST_PNL_Backlight"));
    // The LCD backlight lights a screen, so it arrives held at full rather
    // than unassigned: at 0 the page on it could not be read.
    let lcd = after
        .bindings
        .iter()
        .find(|b| b.device == "CarrierAce_UFC" && b.led == "LCDBacklight")
        .expect("the LCD backlight is a row");
    assert!(lcd.always && lcd.off == 255, "{lcd:?}");
}

#[test]
fn what_the_user_already_decided_is_untouched() {
    let dir = scratch("untouched");
    let active = dir.join("active/old.json");
    std::fs::write(&active, old_profile()).unwrap();

    let before = Profile::load(&active).unwrap();
    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .unwrap();
    let after = Profile::load(&active).unwrap();

    for old in &before.bindings {
        let now = after
            .bindings
            .iter()
            .find(|b| b.device == old.device && b.led == old.led)
            .unwrap_or_else(|| panic!("{} {} vanished", old.device, old.led));
        assert_eq!(now.conditions.len(), old.conditions.len());
        assert_eq!(now.off, old.off, "{}: off value changed", old.led);
        assert_eq!(now.note, old.note, "{}: note changed", old.led);
    }
    assert_eq!(after.name, before.name);
    assert_eq!(after.aircraft, before.aircraft);
}

#[test]
fn running_it_twice_changes_nothing_the_second_time() {
    let dir = scratch("idempotent");
    let active = dir.join("active/old.json");
    std::fs::write(&active, old_profile()).unwrap();
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));

    profiles.merge_new(&inventory(), "test").unwrap();
    let once = std::fs::read_to_string(&active).unwrap();
    let notes = profiles.merge_new(&inventory(), "test").unwrap();

    assert!(notes.is_empty(), "nothing left to do: {notes:?}");
    assert_eq!(once, std::fs::read_to_string(&active).unwrap());
}

#[test]
fn bindings_a_shipped_default_gained_are_carried_across() {
    let dir = scratch("fromdefault");
    std::fs::write(dir.join("active/old.json"), old_profile()).unwrap();
    // The shipped profile now configures a lamp the user's copy leaves blank,
    // and one it has already tuned differently.
    std::fs::write(
        dir.join("defaults/old.json"),
        r#"{
          "schema_version": 2, "name": "Old",
          "aircraft": ["FA-18C_hornet"], "module": "FA-18C_hornet",
          "bindings": [
            { "device": "TAKEOFF_PLANEL_2", "led": "Backlight",
              "conditions": [], "off": 99, "note": "shipped" },
            { "device": "CarrierAce_UFC", "led": "INST_PNL_Backlight",
              "conditions": [{ "source": "INST_PNL_DIMMER",
                               "on_when": { "scale": [0, 65535] } }] }
          ]
        }"#,
    )
    .unwrap();

    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .unwrap();
    let after = Profile::load(&dir.join("active/old.json")).unwrap();

    let ufc = after
        .bindings
        .iter()
        .find(|b| b.device == "CarrierAce_UFC" && b.led == "INST_PNL_Backlight")
        .unwrap();
    assert_eq!(ufc.conditions.len(), 1, "the shipped binding came across");

    let backlight = after
        .bindings
        .iter()
        .find(|b| b.device == "TAKEOFF_PLANEL_2" && b.led == "Backlight")
        .unwrap();
    assert_eq!(backlight.off, 12, "the user's value won, not the shipped one");
    assert_eq!(backlight.note, "mine");
}

#[test]
fn a_profile_that_will_not_parse_is_left_alone() {
    let dir = scratch("broken");
    let broken = dir.join("active/broken.json");
    std::fs::write(&broken, "{ this is not json").unwrap();

    let notes = Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .expect("a broken file does not fail the whole merge");

    assert!(notes.is_empty());
    assert_eq!(
        std::fs::read_to_string(&broken).unwrap(),
        "{ this is not json",
        "rewriting a file we cannot parse is how an editing slip becomes data loss"
    );
}

#[test]
fn rows_for_an_unplugged_panel_are_kept() {
    let dir = scratch("unplugged");
    let active = dir.join("active/old.json");
    std::fs::write(
        &active,
        r#"{
          "schema_version": 2, "name": "Old",
          "aircraft": ["FA-18C_hornet"], "module": "FA-18C_hornet",
          "bindings": [
            { "device": "SOME_PANEL_NOT_PLUGGED_IN", "led": "LAMP",
              "conditions": [], "off": 7 }
          ]
        }"#,
    )
    .unwrap();

    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory(), "test")
        .unwrap();
    let after = Profile::load(&active).unwrap();

    let kept = after
        .bindings
        .iter()
        .find(|b| b.device == "SOME_PANEL_NOT_PLUGGED_IN");
    assert!(
        kept.is_some(),
        "unplugging a panel for an evening is not a decision to discard how it \
         was configured"
    );
    assert_eq!(kept.unwrap().off, 7);
}

#[test]
fn claimed_aircraft_is_keyed_by_aircraft_not_module() {
    // Two profiles on one module, claiming different aircraft, is how the
    // F/A-18E rides the Hornet's outputs. Both claims must show, and neither
    // must hide the other behind the shared module.
    let dir = scratch("claimed");
    let active = dir.join("active");
    let write = |file: &str, name: &str, aircraft: &str| {
        std::fs::write(
            active.join(file),
            format!(
                r#"{{"name": "{name}", "aircraft": [{aircraft}], "module": "FA-18C_hornet", "bindings": []}}"#
            ),
        )
        .unwrap();
    };
    write("fa-18c-hornet.json", "Hornet", r#""FA-18C_hornet""#);
    write("fa-18e.json", "Super Hornet", r#""FA-18E""#);
    // A broken file claims nothing, since it could not be flown either.
    std::fs::write(active.join("broken.json"), "{ not json").unwrap();

    let claimed = Profiles::new(dir.join("defaults"), &active).claimed_aircraft();
    assert_eq!(claimed.len(), 2, "{claimed:?}");
    assert_eq!(claimed.get("FA-18C_hornet").map(String::as_str), Some("Hornet"));
    assert_eq!(claimed.get("FA-18E").map(String::as_str), Some("Super Hornet"));
}

#[test]
fn any_profile_deletes_but_nothing_outside_the_folder() {
    let dir = scratch("delete");
    std::fs::write(dir.join("defaults/old.json"), old_profile()).unwrap();
    std::fs::write(dir.join("active/old.json"), old_profile()).unwrap();
    std::fs::write(dir.join("active/mine.json"), old_profile()).unwrap();
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));

    // Nothing outside the active folder is reachable.
    assert!(profiles.delete("../defaults/old.json").is_err());
    assert!(dir.join("defaults/old.json").is_file());

    profiles.delete("mine.json").expect("a profile the user made deletes");
    assert!(!dir.join("active/mine.json").exists());
    profiles.delete("old.json").expect("so does a shipped one");
    assert!(!dir.join("active/old.json").exists());
}

/// A profile file for the A-10C module claiming `aircraft`.
fn a10(name: &str, aircraft: &[&str]) -> String {
    let list: Vec<String> = aircraft.iter().map(|a| format!("{a:?}")).collect();
    format!(
        r#"{{"name": "{name}", "aircraft": [{}], "module": "A-10C", "bindings": []}}"#,
        list.join(", ")
    )
}

fn aircraft_of(path: &Path) -> Vec<String> {
    Profile::load(path).unwrap().aircraft
}

#[test]
fn a_default_whose_aircraft_are_all_claimed_is_not_seeded() {
    // The user split the shipped A-10C profile, moved both aircraft into their
    // own profiles and deleted the shipped one. Seeding it back would give
    // both aircraft two profiles, and the daemon would fly whichever sorted
    // first.
    let dir = scratch("seed-claimed");
    std::fs::write(dir.join("defaults/a-10c.json"), a10("A-10C", &["A-10C_2", "A-10C"])).unwrap();
    std::fs::write(dir.join("active/warthog.json"), a10("Warthog", &["A-10C_2", "A-10C"])).unwrap();
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));

    assert!(profiles.seed().unwrap().is_empty());
    assert!(!dir.join("active/a-10c.json").exists());
}

#[test]
fn a_default_comes_back_for_only_the_aircraft_nothing_claims() {
    // The shipped A-10C profile was deleted after its A-10C II went to a
    // profile of the user's; the A-10C was left with nowhere to go, so the
    // shipped profile returns for it and for it alone.
    let dir = scratch("seed-partial");
    std::fs::write(dir.join("defaults/a-10c.json"), a10("A-10C", &["A-10C_2", "A-10C"])).unwrap();
    std::fs::write(dir.join("active/mine.json"), a10("Mine", &["A-10C_2"])).unwrap();
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));

    let seeded = profiles.seed().unwrap();
    assert_eq!(seeded.len(), 1, "{seeded:?}");
    assert!(seeded[0].contains("A-10C_2"), "says what it left out: {seeded:?}");
    assert_eq!(aircraft_of(&dir.join("active/a-10c.json")), vec!["A-10C".to_string()]);
    assert_eq!(aircraft_of(&dir.join("active/mine.json")), vec!["A-10C_2".to_string()]);
}

#[test]
fn reset_takes_back_only_the_aircraft_no_other_profile_has() {
    // Copy to... moved the A-10C II out of the shipped profile. Resetting the
    // shipped half puts its lamps back, not its claim on the A-10C II.
    let dir = scratch("reset-claims");
    std::fs::write(dir.join("defaults/a-10c.json"), a10("A-10C", &["A-10C_2", "A-10C"])).unwrap();
    std::fs::write(dir.join("active/a-10c.json"), a10("Renamed", &["A-10C"])).unwrap();
    std::fs::write(dir.join("active/a-10c-ii.json"), a10("A-10C II", &["A-10C_2"])).unwrap();
    let profiles = Profiles::new(dir.join("defaults"), dir.join("active"));

    profiles.reset_to_default("a-10c.json").unwrap();
    let reset = Profile::load(&dir.join("active/a-10c.json")).unwrap();
    assert_eq!(reset.name, "A-10C", "the rest goes back to how it shipped");
    assert_eq!(reset.aircraft, vec!["A-10C".to_string()]);
    assert_eq!(aircraft_of(&dir.join("active/a-10c-ii.json")), vec!["A-10C_2".to_string()]);

    // With the other profile gone, reset takes the whole shipped list again.
    profiles.delete("a-10c-ii.json").unwrap();
    profiles.reset_to_default("a-10c.json").unwrap();
    assert_eq!(aircraft_of(&dir.join("active/a-10c.json")).len(), 2);
}
