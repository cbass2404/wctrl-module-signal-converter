//! `Profiles::merge_new` writes to files the user owns, so it is pinned hard.
//!
//! The rule it has to keep: add what is missing, change nothing else. A user
//! who configured a panel months ago must find that configuration exactly as
//! they left it after an update adds support for a panel they also own.

use std::path::{Path, PathBuf};

use wctrl_config::{DeviceInventory, Profile, Profiles};

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "wctrl-merge-{name}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("defaults")).unwrap();
    std::fs::create_dir_all(dir.join("active")).unwrap();
    dir
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
      "schema_version": 1,
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

#[test]
fn a_profile_written_before_a_device_existed_gains_rows_for_it() {
    let dir = scratch("gains");
    let active = dir.join("active/old.json");
    std::fs::write(&active, old_profile()).unwrap();

    let notes = Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory())
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
        "all three UFC lamps appear, including the two that share the vendor's \
         name across parts: {ufc:?}"
    );
    assert!(ufc.contains(&"LCDBacklight"));
    assert!(ufc.contains(&"HUD_INST_PNL_Backlight"));
}

#[test]
fn what_the_user_already_decided_is_untouched() {
    let dir = scratch("untouched");
    let active = dir.join("active/old.json");
    std::fs::write(&active, old_profile()).unwrap();

    let before = Profile::load(&active).unwrap();
    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory())
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

    profiles.merge_new(&inventory()).unwrap();
    let once = std::fs::read_to_string(&active).unwrap();
    let notes = profiles.merge_new(&inventory()).unwrap();

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
          "schema_version": 1, "name": "Old",
          "aircraft": ["FA-18C_hornet"], "module": "FA-18C_hornet",
          "bindings": [
            { "device": "TAKEOFF_PLANEL_2", "led": "Backlight",
              "conditions": [], "off": 99, "note": "shipped" },
            { "device": "CarrierAce_UFC", "led": "LCDBacklight",
              "conditions": [{ "source": "UFC_BRT",
                               "on_when": { "scale": [0, 65535] } }] }
          ]
        }"#,
    )
    .unwrap();

    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory())
        .unwrap();
    let after = Profile::load(&dir.join("active/old.json")).unwrap();

    let lcd = after
        .bindings
        .iter()
        .find(|b| b.led == "LCDBacklight")
        .unwrap();
    assert_eq!(lcd.conditions.len(), 1, "the shipped binding came across");

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
        .merge_new(&inventory())
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
          "schema_version": 1, "name": "Old",
          "aircraft": ["FA-18C_hornet"], "module": "FA-18C_hornet",
          "bindings": [
            { "device": "SOME_PANEL_NOT_PLUGGED_IN", "led": "LAMP",
              "conditions": [], "off": 7 }
          ]
        }"#,
    )
    .unwrap();

    Profiles::new(dir.join("defaults"), dir.join("active"))
        .merge_new(&inventory())
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
